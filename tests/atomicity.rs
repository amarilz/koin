use anyhow::{Result, ensure};
use chrono::NaiveDate;
use koin::{
    CategoryType, Movement, Store, Transfer,
    voice::{Extraction, Transcriber},
};
use rusqlite::Connection;
use std::{path::PathBuf, sync::Barrier, thread};

struct Database {
    _dir: tempfile::TempDir,
    path: PathBuf,
    inspector: Connection,
    bank: i64,
    cash: i64,
}

impl Database {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("ledger.sqlite3");
        let store = Store::create_new(&path)?;
        let bank = store.create_account("Banca", "EUR", 1000)?;
        let cash = store.create_account("Contanti", "EUR", 100)?;
        let inspector = Connection::open(&path)?;
        Ok(Self {
            _dir: dir,
            path,
            inspector,
            bank,
            cash,
        })
    }

    fn store(&self) -> Result<Store> {
        Store::open_existing(&self.path)
    }

    fn journal_counts(&self) -> Result<(i64, i64)> {
        Ok(self.inspector.query_row(
            "SELECT (SELECT count(*) FROM transactions), (SELECT count(*) FROM transaction_entries)",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    fn check_integrity(&self) -> Result<()> {
        let result: String = self
            .inspector
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        assert_eq!(result, "ok");
        let foreign_key_errors: i64 = self.inspector.query_row(
            "SELECT count(*) FROM pragma_foreign_key_check",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(foreign_key_errors, 0);
        Ok(())
    }

    fn reject_credit(&self) -> Result<()> {
        self.inspector.execute_batch(
            "CREATE TRIGGER reject_credit BEFORE INSERT ON transaction_entries
             WHEN NEW.amount > 0 AND NEW.category_id IS NULL
             BEGIN
                 SELECT CASE WHEN EXISTS (
                     SELECT 1 FROM transaction_entries
                     WHERE transaction_id = NEW.transaction_id AND amount = -NEW.amount
                 ) THEN RAISE(ABORT, 'injected failure after debit')
                   ELSE RAISE(ABORT, 'debit was never attempted') END;
             END;",
        )?;
        Ok(())
    }
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
}

fn movement(amount: i64) -> Movement<'static> {
    Movement {
        account: "Banca",
        category: "Spesa",
        kind: CategoryType::Expense,
        date: date(),
        amount,
        description: Some("Test contabile"),
    }
}

fn transfer(amount: i64) -> Transfer<'static> {
    Transfer {
        from: "Banca",
        to: "Contanti",
        date: date(),
        amount,
        description: None,
    }
}

#[test]
fn account_and_category_constraints_preserve_committed_rows() -> Result<()> {
    let db = Database::new()?;
    let store = db.store()?;
    let category = store.create_category("Stipendio", CategoryType::Income)?;
    let other = store.create_category("Stipendio", CategoryType::Expense)?;
    assert_ne!(category.id, other.id);

    for (name, currency) in [
        ("Banca", "EUR"),
        ("", "EUR"),
        ("Nome", ""),
        ("Nome", "EURO"),
    ] {
        assert!(store.create_account(name, currency, i64::MAX).is_err());
    }
    assert!(store.create_account(&"è".repeat(101), "EUR", 0).is_err());
    for name in ["", "Stipendio", &"è".repeat(101)] {
        assert!(store.create_category(name, CategoryType::Income).is_err());
    }
    assert!(Store::create_new(&db.path).is_err());
    assert!(db.store()?.account_balance(-1).is_err());
    drop(store);

    let reopened = db.store()?;
    let accounts = reopened.accounts()?;
    assert_eq!(accounts.len(), 2);
    assert_eq!(
        (
            accounts[0].id,
            accounts[0].currency.as_str(),
            accounts[0].initial_balance
        ),
        (db.bank, "EUR", 1000)
    );
    assert_eq!(reopened.categories()?, [category, other]);
    assert_eq!(db.journal_counts()?, (0, 0));
    db.check_integrity()
}

#[test]
fn movement_rolls_back_new_category_and_header_when_entry_fails() -> Result<()> {
    let db = Database::new()?;
    let mut store = db.store()?;
    store.add_movement(Movement {
        category: "Preesistente",
        ..movement(-25)
    })?;
    db.inspector.execute_batch(
        "CREATE TRIGGER reject_entry BEFORE INSERT ON transaction_entries
         BEGIN SELECT RAISE(ABORT, 'injected entry failure'); END;",
    )?;
    let error = store.add_movement(movement(-300)).unwrap_err();
    assert!(format!("{error:#}").contains("injected entry failure"));
    drop(store);
    let mut store = db.store()?;
    assert_eq!(db.journal_counts()?, (1, 1));
    assert_eq!(store.account_balance(db.bank)?, 975);
    assert_eq!(store.categories()?.len(), 1);
    db.inspector.execute_batch("DROP TRIGGER reject_entry;")?;
    store.add_movement(movement(-300))?;
    assert_eq!(store.account_balance(db.bank)?, 675);
    assert_eq!(db.journal_counts()?, (2, 2));
    db.check_integrity()
}

#[test]
fn transfer_rolls_back_a_written_debit_when_the_credit_fails() -> Result<()> {
    let db = Database::new()?;
    db.reject_credit()?;
    let error = db.store()?.transfer(transfer(400)).unwrap_err();
    assert!(format!("{error:#}").contains("injected failure after debit"));

    let mut reopened = db.store()?;
    assert_eq!(db.journal_counts()?, (0, 0));
    assert_eq!(reopened.account_balance(db.bank)?, 1000);
    assert_eq!(reopened.account_balance(db.cash)?, 100);
    db.inspector.execute_batch("DROP TRIGGER reject_credit;")?;
    let id = reopened.transfer(transfer(1000))?;
    let entries = db.store()?.recent_entries(10)?;
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.transaction_id == id));
    assert_eq!(entries.iter().map(|entry| entry.amount).sum::<i64>(), 0);
    assert_eq!(reopened.account_balance(db.bank)?, 0);
    assert_eq!(reopened.account_balance(db.cash)?, 1100);
    db.check_integrity()
}

#[test]
fn deferred_commit_failure_preserves_audio_after_the_ledger_writes() -> Result<()> {
    struct Speech;
    impl Transcriber for Speech {
        fn transcribe(&self, audio: &[u8], mime: &str) -> Result<String> {
            assert_eq!(audio, b"wave");
            assert_eq!(mime, "audio/wav");
            Ok("Quattro euro".into())
        }
    }

    let db = Database::new()?;
    let mut store = db.store()?;
    let id = store.add_recording(b"wave", "audio/wav", 1234)?;
    store.transcribe_recording(id, &Speech)?;
    db.inspector.execute_batch(
        "CREATE TABLE fail_at_commit (
            invalid_transaction INTEGER REFERENCES transactions(id) DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TRIGGER reject_commit AFTER INSERT ON transaction_entries
         BEGIN INSERT INTO fail_at_commit VALUES (-1); END;",
    )?;
    let reviewed = Extraction {
        account_name: Some("Banca".into()),
        category_name: Some("Spesa".into()),
        category_type: Some("EXPENSE".into()),
        occurred_at: Some(date().to_string()),
        amount: Some(4.0),
        ..Extraction::default()
    };

    let error = store.confirm_recording(id, &reviewed).unwrap_err();
    assert!(format!("{error:#}").contains("FOREIGN KEY constraint failed"));
    drop(store);
    let mut reopened = db.store()?;
    assert_eq!(
        reopened.review_audio(id)?,
        (b"wave".to_vec(), "audio/wav".into())
    );
    let recording = reopened.recording_for_review(id)?;
    assert_eq!(recording.transcription, "Quattro euro");
    assert_eq!(recording.duration_ms, 1234);
    assert!(reopened.categories()?.is_empty());
    assert_eq!(reopened.account_balance(db.bank)?, 1000);
    assert_eq!(db.journal_counts()?, (0, 0));
    db.check_integrity()?;

    db.inspector.execute_batch("DROP TRIGGER reject_commit;")?;
    reopened.confirm_recording(id, &reviewed)?;
    assert!(reopened.confirm_recording(id, &reviewed).is_err());
    assert!(db.store()?.recording_for_review(id).is_err());
    assert_eq!(reopened.account_balance(db.bank)?, 600);
    assert_eq!(db.journal_counts()?, (1, 1));
    db.check_integrity()
}

#[test]
fn overflow_is_rejected_for_movement_and_transfer_without_a_partial_write() -> Result<()> {
    let db = Database::new()?;
    let mut store = db.store()?;
    let full = store.create_account("Pieno", "EUR", i64::MAX)?;
    let empty = store.create_account("Minimo", "EUR", i64::MIN)?;
    assert!(
        store
            .add_movement(Movement {
                account: "Pieno",
                ..movement(1)
            })
            .is_err()
    );
    assert!(
        store
            .add_movement(Movement {
                account: "Minimo",
                ..movement(-1)
            })
            .is_err()
    );
    assert!(
        store
            .transfer(Transfer {
                to: "Pieno",
                ..transfer(1)
            })
            .is_err()
    );
    assert!(store.categories()?.is_empty());
    assert_eq!(db.journal_counts()?, (0, 0));
    assert_eq!(db.store()?.account_balance(full)?, i64::MAX);
    assert_eq!(db.store()?.account_balance(empty)?, i64::MIN);

    // The movements alone overflow i64, but the opening balance offsets them.
    store.add_movement(Movement {
        account: "Minimo",
        ..movement(i64::MAX)
    })?;
    store.add_movement(Movement {
        account: "Minimo",
        ..movement(2)
    })?;
    let reopened = db.store()?;
    assert_eq!(reopened.account_balance(empty)?, 1);
    assert_eq!(
        reopened
            .accounts()?
            .iter()
            .find(|account| account.id == empty)
            .unwrap()
            .balance,
        1
    );
    db.check_integrity()
}

#[test]
fn racing_retries_record_a_single_movement() -> Result<()> {
    let db = Database::new()?;
    let barrier = Barrier::new(2);
    let (first, second) = (db.store()?, db.store()?);
    let results = thread::scope(|scope| {
        let write = |mut store: Store| {
            barrier.wait();
            store.add_movement(movement(-300))
        };
        let a = scope.spawn(move || write(first));
        let b = scope.spawn(move || write(second));
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
            .all(|e| e.to_string().contains("already exists"))
    );
    let store = db.store()?;
    assert_eq!(db.journal_counts()?, (1, 1));
    assert_eq!(store.account_balance(db.bank)?, 700);
    assert_eq!(store.categories()?.len(), 1);
    db.check_integrity()
}

#[test]
fn abrupt_process_exit_keeps_committed_rows_and_discards_uncommitted_rows() -> Result<()> {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        time::Duration,
    };

    const CRASH_DB: &str = "KOIN_TEST_CRASH_DB";
    if let Some(path) = std::env::var_os(CRASH_DB) {
        use std::io::Write;
        let mut store = Store::open_existing(&path)?;
        store.create_account("Registrato", "EUR", 500)?;
        store.transfer(transfer(200))?;
        store.add_recording(b"committed audio", "audio/wav", 100)?;
        drop(store);

        // Force SQLite to write dirty pages before commit, so killing this
        // process exercises on-disk journal recovery.
        let writer = Connection::open(path)?;
        writer.execute_batch(
            "PRAGMA cache_size = 1;
             PRAGMA cache_spill = ON;
             BEGIN IMMEDIATE;
             DELETE FROM voice_recordings;
             UPDATE accounts SET initial_balance = 0;
             INSERT INTO scratch VALUES (randomblob(262144));",
        )?;
        println!("READY TO KILL");
        std::io::stdout().flush()?;
        loop {
            thread::park();
        }
    }

    for mode in ["DELETE", "WAL"] {
        let db = Database::new()?;
        db.inspector.pragma_update(None, "journal_mode", mode)?;
        db.inspector
            .execute("CREATE TABLE scratch (payload BLOB)", [])?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "abrupt_process_exit_keeps_committed_rows_and_discards_uncommitted_rows",
                "--nocapture",
            ])
            .env(CRASH_DB, &db.path)
            .stdout(Stdio::piped())
            .spawn()?;
        let output = child.stdout.take().unwrap();
        let (signal, ready) = std::sync::mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(output)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if line == "READY TO KILL" {
                    let _ = signal.send(());
                    break;
                }
            }
        });
        let was_ready = ready.recv_timeout(Duration::from_secs(10));
        let _ = child.kill();
        let status = child.wait()?;
        reader.join().unwrap();
        was_ready?;
        assert!(!status.success());

        let reopened = db.store()?;
        assert_eq!(reopened.accounts()?.len(), 3);
        assert_eq!(reopened.account_balance(db.bank)?, 800);
        assert_eq!(reopened.account_balance(db.cash)?, 300);
        assert_eq!(db.journal_counts()?, (1, 2));
        let audio: Vec<u8> =
            db.inspector
                .query_row("SELECT audio_data FROM voice_recordings", [], |row| {
                    row.get(0)
                })?;
        assert_eq!(audio, b"committed audio");
        let scratch: i64 = db
            .inspector
            .query_row("SELECT count(*) FROM scratch", [], |row| row.get(0))?;
        assert_eq!(scratch, 0);
        db.check_integrity()?;
    }
    Ok(())
}

#[test]
fn single_row_inserts_report_commit_failure_instead_of_returning_a_phantom_id() -> Result<()> {
    let db = Database::new()?;
    let store = db.store()?;
    db.inspector.execute_batch(
        "CREATE TABLE invalid_reference (
            transaction_id INTEGER REFERENCES transactions(id) DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TRIGGER reject_account_commit AFTER INSERT ON accounts
         BEGIN INSERT INTO invalid_reference VALUES (-1); END;
         CREATE TRIGGER reject_category_commit AFTER INSERT ON categories
         BEGIN INSERT INTO invalid_reference VALUES (-1); END;
         CREATE TRIGGER reject_audio_commit AFTER INSERT ON voice_recordings
         BEGIN INSERT INTO invalid_reference VALUES (-1); END;",
    )?;
    let account = store.create_account("Nuovo", "EUR", 200);
    let category = store.create_category("Nuova", CategoryType::Income);
    let audio = store.add_recording(b"wav", "audio/wav", 200);
    assert!(
        account.is_err(),
        "Account insert returned {account:?} after a failed commit"
    );
    assert!(
        category.is_err(),
        "Category insert returned {category:?} after a failed commit"
    );
    assert!(
        audio.is_err(),
        "Audio insert returned {audio:?} after a failed commit"
    );
    assert_eq!(db.store()?.accounts()?.len(), 2);
    assert!(db.store()?.categories()?.is_empty());
    let audio_count: i64 =
        db.inspector
            .query_row("SELECT count(*) FROM voice_recordings", [], |row| {
                row.get(0)
            })?;
    assert_eq!(audio_count, 0);
    db.check_integrity()
}

#[test]
fn opening_a_newer_database_fails_without_downgrading_or_overwriting_it() -> Result<()> {
    let db = Database::new()?;
    db.inspector.pragma_update(None, "user_version", 100)?;
    assert!(db.store().is_err());
    assert!(Store::open(&db.path).is_err());
    assert!(Store::create_new(&db.path).is_err());
    let version: i64 = db
        .inspector
        .pragma_query_value(None, "user_version", |row| row.get(0))?;
    assert_eq!(version, 100);
    let saved: (String, i64) = db.inspector.query_row(
        "SELECT name, initial_balance FROM accounts WHERE id = ?1",
        [db.bank],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(saved, ("Banca".into(), 1000));
    db.check_integrity()
}
