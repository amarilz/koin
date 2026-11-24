use anyhow::{Result, bail, ensure};
use koin::{
    Category, Store,
    voice::{Extraction, Extractor, Transcriber},
};
use std::{cell::Cell, sync::Barrier, thread};

const AUDIO: &[u8] = b"\x00original wave\xff";
const TEXT: &str = "Accredito per rimborso";

struct Speech;
impl Transcriber for Speech {
    fn transcribe(&self, audio: &[u8], mime: &str) -> Result<String> {
        assert_eq!(audio, AUDIO);
        assert_eq!(mime, "audio/wav");
        Ok(TEXT.to_owned())
    }
}

struct Proposal;
impl Extractor for Proposal {
    fn extract(
        &self,
        text: &str,
        _accounts: &[String],
        _categories: &[Category],
    ) -> Result<Extraction> {
        assert_eq!(text, TEXT);
        Ok(proposal())
    }
}

fn proposal() -> Extraction {
    Extraction {
        account_name: Some("Banca".into()),
        category_name: Some("Rimborso".into()),
        category_type: Some("INCOME".into()),
        amount: Some(12.34),
        occurred_at: Some("2026-11-24".into()),
        description: Some("Rimborso".into()),
        ..Extraction::default()
    }
}

#[test]
fn failed_and_empty_transcripts_are_retryable_without_blocking_other_recordings() -> Result<()> {
    struct UnreliableSpeech(Cell<usize>);
    impl Transcriber for UnreliableSpeech {
        fn transcribe(&self, audio: &[u8], mime: &str) -> Result<String> {
            let call = self.0.replace(self.0.get() + 1);
            match call {
                0 => bail!("Whisper unavailable"),
                1 => Ok("  \n  ".into()),
                _ => Speech.transcribe(audio, mime),
            }
        }
    }

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let mut store = Store::create_new(&path)?;
    store.create_account("Banca", "EUR", 100)?;
    let failed = store.add_recording(AUDIO, "audio/wav", 11)?;
    let blank = store.add_recording(AUDIO, "audio/wav", 22)?;
    let succeeds = store.add_recording(AUDIO, "audio/wav", 33)?;
    assert!(store.confirm_recording(failed, &proposal()).is_err());
    assert!(store.discard_recording(blank).is_err());
    let failures = store.process_pending(&UnreliableSpeech(Cell::new(0)), &Proposal)?;
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.recording_id)
            .collect::<Vec<_>>(),
        [failed, blank]
    );
    assert!(
        failures[1]
            .error
            .to_string()
            .contains("Empty transcription")
    );
    assert_eq!(store.reviewable_recordings()?[0].id, succeeds);
    drop(store);

    let store = Store::open_existing(&path)?;
    assert!(store.process_pending(&Speech, &Proposal)?.is_empty());
    for (id, duration_ms) in [(failed, 11), (blank, 22), (succeeds, 33)] {
        let record = store.recording_for_review(id)?;
        assert_eq!(record.duration_ms, duration_ms);
        assert_eq!(record.transcription, TEXT);
        assert_eq!(record.extraction, Some(proposal()));
        assert_eq!(
            store.review_audio(id)?,
            (AUDIO.to_vec(), "audio/wav".into())
        );
    }
    assert!(
        store
            .process_pending(&UnreliableSpeech(Cell::new(0)), &Proposal)?
            .is_empty()
    );
    assert!(store.recent_entries(10)?.is_empty());
    Ok(())
}

#[test]
fn rejected_audio_reviews_and_duplicate_movements_preserve_original_recording() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let mut store = Store::create_new(&path)?;
    let account = store.create_account("Banca", "EUR", 500)?;
    let first = store.add_recording(AUDIO, "audio/wav", 11)?;
    let second = store.add_recording(AUDIO, "audio/wav", 22)?;
    store.process_pending(&Speech, &Proposal)?;
    store.confirm_recording(first, &proposal())?;

    let invalid = [
        Extraction {
            amount: None,
            ..proposal()
        },
        Extraction {
            amount: Some(0.0),
            ..proposal()
        },
        Extraction {
            amount: Some(-1.0),
            ..proposal()
        },
        Extraction {
            amount: Some(0.001),
            ..proposal()
        },
        Extraction {
            amount: Some(f64::NAN),
            ..proposal()
        },
        Extraction {
            amount: Some(f64::INFINITY),
            ..proposal()
        },
        Extraction {
            amount: Some(1e12 + 1.0),
            ..proposal()
        },
        Extraction {
            occurred_at: Some("2026-02-29".into()),
            ..proposal()
        },
        Extraction {
            occurred_at: Some("2026-1-02".into()),
            ..proposal()
        },
        Extraction {
            account_name: Some(" inesistente ".into()),
            ..proposal()
        },
        Extraction {
            account_name: Some("  ".into()),
            ..proposal()
        },
        Extraction {
            category_type: Some("INVALID".into()),
            ..proposal()
        },
        Extraction {
            category_name: None,
            ..proposal()
        },
        Extraction {
            category_type: Some("TRANSFER".into()),
            ..proposal()
        },
        Extraction {
            category_type: Some("TRANSFER".into()),
            account_to: Some("Banca".into()),
            ..proposal()
        },
        proposal(),
    ];
    for reviewed in invalid {
        assert!(
            store.confirm_recording(second, &reviewed).is_err(),
            "{reviewed:?}"
        );
        let snapshot = Store::open_existing(&path)?;
        let recording = snapshot.recording_for_review(second)?;
        assert_eq!(recording.transcription, TEXT);
        assert_eq!(recording.extraction, Some(proposal()));
        assert_eq!(snapshot.review_audio(second)?.0, AUDIO);
        assert_eq!(snapshot.account_balance(account)?, 1734);
        assert_eq!(snapshot.recent_entries(10)?.len(), 1);
        assert_eq!(snapshot.categories()?.len(), 1);
    }
    store.confirm_recording(
        second,
        &Extraction {
            amount: Some(2.0),
            ..proposal()
        },
    )?;
    assert_eq!(Store::open_existing(&path)?.account_balance(account)?, 1934);
    Ok(())
}

#[test]
fn concurrent_confirmation_consumes_the_recording_once_even_with_different_edits() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let store = Store::create_new(&path)?;
    let account = store.create_account("Banca", "EUR", 500)?;
    let id = store.add_recording(AUDIO, "audio/wav", 100)?;
    assert!(store.process_pending(&Speech, &Proposal)?.is_empty());
    let first = Store::open_existing(&path)?;
    let second = Store::open_existing(&path)?;
    let barrier = Barrier::new(2);
    let results = thread::scope(|scope| {
        let confirm = |mut store: Store, amount| {
            barrier.wait();
            store.confirm_recording(
                id,
                &Extraction {
                    amount: Some(amount),
                    ..proposal()
                },
            )
        };
        let a = scope.spawn(move || confirm(first, 2.0));
        let b = scope.spawn(move || confirm(second, 3.0));
        [a.join().unwrap(), b.join().unwrap()]
    });
    ensure!(
        results.iter().filter(|result| result.is_ok()).count() == 1,
        "{results:?}"
    );
    assert!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.to_string().contains("Recording missing"))
    );
    let reopened = Store::open_existing(path)?;
    assert!(reopened.reviewable_recordings()?.is_empty());
    let entries = reopened.recent_entries(10)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(reopened.account_balance(account)?, 500 + entries[0].amount);
    Ok(())
}

#[test]
fn failed_database_updates_leave_audio_and_previous_stage_available_for_retry() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let store = Store::create_new(&path)?;
    let id = store.add_recording(AUDIO, "audio/wav", 77)?;
    let db = rusqlite::Connection::open(&path)?;
    db.execute_batch(
        "CREATE TRIGGER reject_transcription BEFORE UPDATE OF transcription ON voice_recordings
         BEGIN SELECT RAISE(ABORT, 'transcript write failed'); END;
         CREATE TRIGGER reject_extraction BEFORE UPDATE OF extraction ON voice_recordings
         BEGIN SELECT RAISE(ABORT, 'proposal write failed'); END;",
    )?;
    let failures = store.process_pending(&Speech, &Proposal)?;
    assert_eq!(failures.len(), 1);
    assert!(format!("{:#}", failures[0].error).contains("transcript write failed"));
    assert!(
        Store::open_existing(&path)?
            .reviewable_recordings()?
            .is_empty()
    );

    db.execute_batch("DROP TRIGGER reject_transcription;")?;
    let failures = Store::open_existing(&path)?.process_pending(&Speech, &Proposal)?;
    assert_eq!(failures.len(), 1);
    assert!(format!("{:#}", failures[0].error).contains("proposal write failed"));
    let reopened = Store::open_existing(&path)?;
    let record = reopened.recording_for_review(id)?;
    assert_eq!(record.transcription, TEXT);
    assert_eq!(record.duration_ms, 77);
    assert!(record.extraction.is_none());
    assert_eq!(reopened.review_audio(id)?.0, AUDIO);
    assert!(reopened.recent_entries(10)?.is_empty());

    db.execute_batch("DROP TRIGGER reject_extraction;")?;
    assert!(reopened.process_pending(&Speech, &Proposal)?.is_empty());
    assert_eq!(
        Store::open_existing(&path)?
            .recording_for_review(id)?
            .extraction,
        Some(proposal())
    );
    Ok(())
}

#[test]
fn late_transcription_does_not_overwrite_a_completed_transcript() -> Result<()> {
    struct Delayed<'a> {
        other: &'a Store,
        id: i64,
    }
    impl Transcriber for Delayed<'_> {
        fn transcribe(&self, _audio: &[u8], _mime: &str) -> Result<String> {
            self.other.transcribe_recording(self.id, &Speech)?;
            Ok("Una trascrizione arrivata in ritardo".into())
        }
    }

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let store = Store::create_new(&path)?;
    let other = Store::open_existing(&path)?;
    let id = store.add_recording(AUDIO, "audio/wav", 99)?;
    assert!(
        store
            .transcribe_recording(id, &Delayed { other: &other, id })
            .is_err()
    );
    let record = Store::open_existing(path)?.recording_for_review(id)?;
    assert_eq!(record.transcription, TEXT);
    assert_eq!(store.review_audio(id)?.0, AUDIO);
    Ok(())
}

#[test]
fn late_extraction_cannot_resurrect_a_discarded_recording() -> Result<()> {
    struct Delayed<'a> {
        other: &'a Store,
        id: i64,
    }
    impl Extractor for Delayed<'_> {
        fn extract(
            &self,
            _text: &str,
            _accounts: &[String],
            _categories: &[Category],
        ) -> Result<Extraction> {
            self.other.discard_recording(self.id)?;
            Ok(proposal())
        }
    }

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let store = Store::create_new(&path)?;
    let other = Store::open_existing(&path)?;
    let id = store.add_recording(AUDIO, "audio/wav", 100)?;
    store.transcribe_recording(id, &Speech)?;
    assert!(
        store
            .extract_recording(id, &Delayed { other: &other, id })
            .is_err()
    );
    let reopened = Store::open_existing(&path)?;
    assert!(reopened.reviewable_recordings()?.is_empty());
    assert!(reopened.recent_entries(10)?.is_empty());
    assert!(reopened.add_recording(AUDIO, "audio/wav", 100)? > id);
    Ok(())
}
