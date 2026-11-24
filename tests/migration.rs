use anyhow::{Result, bail};
use chrono::NaiveDate;
use koin::{
    Category, CategoryType, Movement, Store, Transfer,
    voice::{Extraction, Extractor, Transcriber},
};
use std::cell::Cell;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

#[test]
fn ledger_persists_signed_movements_and_balances() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("data/koin.sqlite3");
    let mut store = Store::open(&path)?;
    assert!(store.accounts()?.is_empty());
    assert!(store.categories()?.is_empty());
    assert!(store.recent_entries(10)?.is_empty());
    let bank = store.create_account("Banca", "EUR", 10_000)?;
    assert!(store.create_account("Banca", "EUR", 0).is_err());
    store.create_category("Altro", CategoryType::Income)?;
    assert!(
        store
            .create_category("Altro", CategoryType::Income)
            .is_err()
    );

    let expense = || Movement {
        account: "Banca",
        category: "Altro",
        kind: CategoryType::Expense,
        date: date(20),
        amount: -1_234,
        description: None,
    };
    let id = store.add_movement(expense())?;
    assert!(store.add_movement(expense()).is_err());
    store.add_movement(Movement {
        description: Some(""),
        ..expense()
    })?;
    store.add_movement(Movement {
        kind: CategoryType::Income,
        date: date(21),
        amount: 500,
        ..expense()
    })?;
    drop(store);

    let store = Store::open(path)?;
    assert_eq!(store.account_balance(bank)?, 8_032);
    assert_eq!(store.accounts()?[0].balance, 8_032);
    assert_eq!(store.categories()?.len(), 2);
    let entries = store.recent_entries(10)?;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].date, date(21));
    assert_eq!(entries[1].description.as_deref(), Some(""));
    assert_eq!(entries[2].transaction_id, id);
    assert_eq!(entries[2].description, None);
    assert_eq!(store.recent_entries(1)?.len(), 1);
    Ok(())
}

#[test]
fn transfers_are_atomic_and_detect_duplicates() -> Result<()> {
    let mut store = Store::open(":memory:")?;
    let bank = store.create_account("Banca", "EUR", 1_000)?;
    let cash = store.create_account("Contanti", "EUR", 50)?;
    let transfer = || Transfer {
        from: "Banca",
        to: "Contanti",
        date: date(20),
        amount: 400,
        description: None,
    };
    let id = store.transfer(transfer())?;
    assert!(store.transfer(transfer()).is_err());
    assert!(
        store
            .transfer(Transfer {
                amount: 601,
                ..transfer()
            })
            .is_err()
    );
    assert!(
        store
            .transfer(Transfer {
                amount: 0,
                ..transfer()
            })
            .is_err()
    );
    assert!(
        store
            .transfer(Transfer {
                to: "Banca",
                ..transfer()
            })
            .is_err()
    );
    assert_eq!(store.account_balance(bank)?, 600);
    assert_eq!(store.account_balance(cash)?, 450);
    let entries = store.recent_entries(10)?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].amount, 400);
    assert_eq!(entries[1].amount, -400);
    for entry in entries {
        assert_eq!(entry.transaction_id, id);
        assert_eq!(entry.category, None);
        assert_eq!(
            entry.description.as_deref(),
            Some("Transfer between accounts")
        );
    }
    Ok(())
}

#[derive(Default)]
struct Services {
    transcriptions: Cell<usize>,
    fail_extraction: Cell<bool>,
}

impl Transcriber for Services {
    fn transcribe(&self, audio: &[u8], mime: &str) -> Result<String> {
        assert_eq!(audio, b"wav");
        assert_eq!(mime, "audio/wav");
        self.transcriptions.set(self.transcriptions.get() + 1);
        Ok(" Spesa di 12,50 ".into())
    }
}

impl Extractor for Services {
    fn extract(
        &self,
        text: &str,
        accounts: &[String],
        categories: &[Category],
    ) -> Result<Extraction> {
        assert_eq!(text, "Spesa di 12,50");
        assert_eq!(accounts, ["Banca", "Contanti"]);
        assert_eq!(categories[0].name, "Spesa");
        if self.fail_extraction.get() {
            bail!("Ollama unavailable");
        }
        Ok(Extraction {
            account_name: Some("Banca".into()),
            category_name: Some("Spesa".into()),
            category_type: Some("EXPENSE".into()),
            amount: Some(12.5),
            occurred_at: Some("2026-11-20".into()),
            ..Extraction::default()
        })
    }
}

#[test]
fn audio_is_retried_reviewed_confirmed_and_discarded() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("koin.sqlite3");
    let store = Store::open(&path)?;
    let bank = store.create_account("Banca", "EUR", 2_000)?;
    let cash = store.create_account("Contanti", "EUR", 0)?;
    store.create_category("Spesa", CategoryType::Expense)?;
    assert!(store.add_recording(b"", "audio/wav", 0).is_err());
    assert!(store.add_recording(b"wav", "text/plain", 0).is_err());
    assert!(store.add_recording(b"wav", "audio/wav", -1).is_err());
    let id = store.add_recording(b"wav", "audio/wav", 1_500)?;
    assert!(store.reviewable_recordings()?.is_empty());

    let services = Services::default();
    services.fail_extraction.set(true);
    let failures = store.process_pending(&services, &services)?;
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].recording_id, id);
    assert!(store.recording_for_review(id)?.extraction.is_none());
    drop(store);

    let mut store = Store::open(path)?;
    services.fail_extraction.set(false);
    assert!(store.process_pending(&services, &services)?.is_empty());
    assert_eq!(services.transcriptions.get(), 1);
    assert_eq!(
        store.review_audio(id)?,
        (b"wav".to_vec(), "audio/wav".to_owned())
    );
    let recording = store.recording_for_review(id)?;
    assert_eq!(recording.duration_ms, 1_500);
    let extraction = recording.extraction.unwrap();
    let rejected_transfer = Extraction {
        category_type: Some("TRANSFER".into()),
        account_to: Some("Contanti".into()),
        amount: Some(25.0),
        ..extraction.clone()
    };
    assert!(store.confirm_recording(id, &rejected_transfer).is_err());
    assert_eq!(
        store.recording_for_review(id)?.extraction.as_ref(),
        Some(&extraction)
    );
    assert!(store.recent_entries(10)?.is_empty());

    let edited = Extraction {
        amount: Some(12.55),
        ..extraction
    };
    let transaction = store.confirm_recording(id, &edited)?;
    assert!(store.confirm_recording(id, &edited).is_err());
    assert_eq!(store.account_balance(bank)?, 745);
    let entries = store.recent_entries(10)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].transaction_id, transaction);
    assert_eq!(entries[0].amount, -1255);

    let id = store.add_recording(b"wav", "audio/wav", 0)?;
    assert!(store.process_pending(&services, &services)?.is_empty());
    let transfer = Extraction {
        amount: Some(2.5),
        ..rejected_transfer
    };
    store.confirm_recording(id, &transfer)?;
    assert_eq!(store.account_balance(bank)?, 495);
    assert_eq!(store.account_balance(cash)?, 250);

    let discarded = store.add_recording(b"wav", "audio/wav", 0)?;
    store.transcribe_recording(discarded, &services)?;
    store.discard_recording(discarded)?;
    assert!(store.reviewable_recordings()?.is_empty());
    assert_eq!(store.recent_entries(10)?.len(), 3);
    Ok(())
}

#[test]
fn simultaneous_transfers_cannot_spend_the_same_balance() -> Result<()> {
    use std::{sync::Barrier, thread};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("koin.sqlite3");
    let store = Store::open(&path)?;
    let bank = store.create_account("Banca", "EUR", 1_000)?;
    store.create_account("Contanti", "EUR", 0)?;
    store.create_account("Risparmio", "EUR", 0)?;
    let gate = Barrier::new(2);

    let (cash, savings) = thread::scope(|scope| {
        let send = |mut connection: Store, destination| {
            gate.wait();
            connection.transfer(Transfer {
                from: "Banca",
                to: destination,
                date: date(20),
                amount: 700,
                description: None,
            })
        };
        let first = Store::open(&path)?;
        let second = Store::open(&path)?;
        let cash = scope.spawn(move || send(first, "Contanti"));
        let savings = scope.spawn(move || send(second, "Risparmio"));
        Ok::<_, anyhow::Error>((cash.join().unwrap(), savings.join().unwrap()))
    })?;
    let failure = match (cash, savings) {
        (Ok(_), Err(error)) | (Err(error), Ok(_)) => error,
        outcomes => panic!("Expected exactly one successful transfer, got {outcomes:?}"),
    };
    assert_eq!(failure.to_string(), "Insufficient balance");
    assert_eq!(store.account_balance(bank)?, 300);
    assert_eq!(store.recent_entries(10)?.len(), 2);
    Ok(())
}

#[test]
fn full_history_combines_account_category_and_inclusive_date_filters() -> Result<()> {
    use koin::{HistoryCategory, HistoryFilter};

    let mut store = Store::open(":memory:")?;
    let bank = store.create_account("Banca", "EUR", 5_000)?;
    let cash = store.create_account("Contanti", "EUR", 0)?;
    let expense = store.create_category("Altro", CategoryType::Expense)?;
    store.create_category("Altro", CategoryType::Income)?;

    for day in 1..=25 {
        store.add_movement(Movement {
            account: "Banca",
            category: "Altro",
            kind: CategoryType::Expense,
            date: date(day),
            amount: -100,
            description: None,
        })?;
    }
    store.add_movement(Movement {
        account: "Contanti",
        category: "Altro",
        kind: CategoryType::Income,
        date: date(25),
        amount: 500,
        description: None,
    })?;
    store.transfer(Transfer {
        from: "Banca",
        to: "Contanti",
        date: date(25),
        amount: 300,
        description: None,
    })?;

    let all = store.history(HistoryFilter::default())?;
    assert_eq!(all.len(), 28);
    assert_eq!(all[0].account, "Contanti");
    assert_eq!(all[1].account, "Banca");
    assert_eq!(all.last().unwrap().date, date(1));
    assert!(all.windows(2).all(|pair| pair[0].date >= pair[1].date));
    assert_eq!(store.recent_entries(20)?.len(), 20);

    let combined = HistoryFilter {
        account_id: Some(bank),
        category: Some(HistoryCategory::Category(expense.id)),
        from: Some(date(23)),
        through: Some(date(25)),
    };
    let expenses = store.history(combined)?;
    assert_eq!(
        expenses.iter().map(|entry| entry.date).collect::<Vec<_>>(),
        [date(25), date(24), date(23)]
    );
    assert!(expenses.iter().all(|entry| entry.amount == -100));
    assert!(
        store
            .history(HistoryFilter {
                account_id: Some(cash),
                ..combined
            })?
            .is_empty()
    );

    let transfers = store.history(HistoryFilter {
        account_id: Some(cash),
        category: Some(HistoryCategory::Transfers),
        through: Some(date(25)),
        ..HistoryFilter::default()
    })?;
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].amount, 300);
    assert!(
        store
            .history(HistoryFilter {
                from: Some(date(26)),
                ..combined
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn legacy_default_transfer_description_still_prevents_duplicate_retries() -> Result<()> {
    let mut store = Store::open(":memory:")?;
    let bank = store.create_account("Banca", "EUR", 5_000)?;
    store.create_account("Contanti", "EUR", 0)?;
    let transfer = || Transfer {
        from: "Banca",
        to: "Contanti",
        date: date(24),
        amount: 500,
        description: None,
    };
    store.transfer(Transfer {
        description: Some("Trasferimento tra account"),
        ..transfer()
    })?;
    assert_eq!(
        store.transfer(transfer()).unwrap_err().to_string(),
        "Transfer already exists"
    );
    let entries = store.recent_entries(10)?;
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].description.as_deref(),
        Some("Trasferimento tra account")
    );
    assert_eq!(
        entries[0].display_description(),
        Some("Transfer between accounts")
    );
    assert_eq!(store.account_balance(bank)?, 4_500);
    Ok(())
}
