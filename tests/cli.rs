use anyhow::{Result, ensure};
use koin::Store;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn run(mut command: Command, input: &str) -> Result<String> {
    let mut child = command
        .env("KOIN_UPLOAD_LISTEN", "127.0.0.1:0")
        .env("TRANSCRIPTION_SERVICE_URL", "invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn koin_with_database(database: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_koin"));
    command.arg("--database").arg(database);
    command
}

#[test]
fn menus_record_income_expenses_and_transfers() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("ledger.sqlite3");
    Store::create_new(&path)?;
    let output = run(
        koin_with_database(&path),
        concat!(
            "1\n",
            "1\nBanca\n\n100,00\n",
            "1\nContanti\nEUR\n\n",
            "0\n",
            "2\n1\n2\nAlimentari\n0\n",
            "3\n2\n1\n1\n12,345\n12,35\n2026-11-20\nSpesa\n",
            "3\n1\n1\n1\nStipendio\n25.50\n2026-11-21\n\n",
            "4\n1\n1\n20\n2026-11-22\nPrelievo\n",
            "4\n1\n1\n999\n2026-11-22\n\n",
            "3\n2\n1\n2\nCategoria annullata\n/cancel\n",
            "0\n",
        ),
    )?;
    let main_menu = output.split("\nAccounts and balances\n").next().unwrap();
    for nested in [
        "Create account",
        "Create category",
        "Record income",
        "Record expense",
    ] {
        assert!(!main_menu.contains(nested));
    }
    assert!(main_menu.contains("3. Record transaction"));
    assert!(output.contains("Banca  100.00 EUR"));
    for message in [
        "No accounts",
        "Expense  Alimentari",
        "at most two decimal places",
        "Transfer recorded",
        "Insufficient balance",
        "Operation canceled",
    ] {
        assert!(output.contains(message), "Missing {message:?} in {output}");
    }

    let restarted = run(koin_with_database(&path), "1\n0\n5\n0\n0\n")?;
    for row in [
        "Total  113.15 EUR",
        "Banca  93.15 EUR",
        "Contanti  20.00 EUR",
        "2026-11-22  Contanti  Transfer  +20.00 EUR  Prelievo",
        "2026-11-22  Banca  Transfer  -20.00 EUR  Prelievo",
        "2026-11-21  Banca  Stipendio  +25.50 EUR",
        "2026-11-20  Banca  Alimentari  -12.35 EUR  Spesa",
    ] {
        assert!(restarted.contains(row), "Missing {row:?} in {restarted}");
    }
    assert!(!restarted.contains("#3"));
    let store = Store::open_existing(path)?;
    assert_eq!(store.categories()?.len(), 2);
    assert_eq!(store.recent_entries(20)?.len(), 4);
    Ok(())
}

#[cfg(unix)]
#[test]
fn first_run_remembers_the_database_and_never_replaces_existing_files() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("books/Koin personale.sqlite3");
    let unrelated = temp.path().join("unrelated.sqlite3");
    rusqlite::Connection::open(&unrelated)?.execute("CREATE TABLE notes (value TEXT)", [])?;
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_koin"));
        command
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .env("XDG_CONFIG_HOME", temp.path().join("config"))
            .env("XDG_DATA_HOME", temp.path().join("data"));
        command
    };
    let setup = run(
        command(),
        "1\nunrelated.sqlite3\n2\nunrelated.sqlite3\n1\nbooks/Koin personale.sqlite3\n1\n1\nBanca\n\n10\n0\n0\n",
    )?;
    assert_eq!(setup.matches("Could not use").count(), 2);
    assert!(setup.contains("Account created"));
    let restarted = run(command(), "1\n0\n0\n")?;
    assert!(!restarted.contains("Create a database"));
    assert!(restarted.contains("Banca  10.00 EUR"));
    assert_eq!(Store::open_existing(&path)?.accounts()?.len(), 1);

    std::fs::rename(&path, path.with_file_name("spostato.sqlite3"))?;
    let recovered = run(command(), "2\nbooks/spostato.sqlite3\n1\n0\n0\n")?;
    assert!(recovered.contains("Could not open"));
    assert!(recovered.contains("Banca  10.00 EUR"));
    assert!(!path.exists());
    Ok(())
}

#[test]
fn audio_review_can_edit_confirm_and_discard() -> Result<()> {
    use koin::{
        Category, CategoryType,
        voice::{Extraction, Extractor, Transcriber},
    };

    struct AudioServices;

    impl Transcriber for AudioServices {
        fn transcribe(&self, _audio: &[u8], _mime_type: &str) -> Result<String> {
            Ok("Ho speso cinque euro per il pranzo.".into())
        }
    }

    impl Extractor for AudioServices {
        fn extract(
            &self,
            _text: &str,
            _accounts: &[String],
            _categories: &[Category],
        ) -> Result<Extraction> {
            Ok(Extraction {
                account_name: Some("Banca".into()),
                category_type: Some("EXPENSE".into()),
                category_name: Some("Pranzo".into()),
                amount: Some(5.0),
                occurred_at: Some("2026-11-20".into()),
                description: Some("Pausa pranzo".into()),
                ..Extraction::default()
            })
        }
    }

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("audio.sqlite3");
    let store = Store::create_new(&path)?;
    let bank = store.create_account("Banca", "EUR", 10_000)?;
    store.create_category("Pranzo", CategoryType::Expense)?;
    let discarded = store.add_recording(b"wav", "audio/wav", 1000)?;
    let confirmed = store.add_recording(b"wav", "audio/wav", 2000)?;
    assert!(
        store
            .process_pending(&AudioServices, &AudioServices)?
            .is_empty()
    );

    let output = run(
        koin_with_database(&path),
        concat!(
            "6\n",
            "2\n1\n2\n",
            "1\n2\n\n\n6,25\n\nPranzo con Luca\n1\n",
            "2\n1\n3\n1\n",
            "0\n0\n",
        ),
    )?;
    assert!(output.contains("Ho speso cinque euro per il pranzo."));
    assert!(output.contains("Transaction recorded"));
    assert!(output.contains("Recording discarded"));
    assert!(store.recording_for_review(confirmed).is_err());
    assert!(store.recording_for_review(discarded).is_err());
    assert_eq!(store.account_balance(bank)?, 9_375);
    let entries = store.recent_entries(20)?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].description.as_deref(), Some("Pranzo con Luca"));
    Ok(())
}

#[test]
fn ordinary_cli_starts_the_android_receiver_and_releases_it_on_exit() -> Result<()> {
    use anyhow::Context;
    use reqwest::blocking::{
        Client,
        multipart::{Form, Part},
    };
    use std::{
        io::{BufRead, BufReader},
        net::{SocketAddr, TcpStream},
        time::Duration,
    };

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("koin.db");
    Store::create_new(&path)?;
    let mut child = koin_with_database(&path)
        .env("KOIN_UPLOAD_LISTEN", "0.0.0.0:0")
        .env_remove("KOIN_UPLOAD_TOKEN")
        .env("TRANSCRIPTION_SERVICE_URL", "invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = (|| -> Result<SocketAddr> {
        let mut output = BufReader::new(child.stdout.take().context("Missing CLI stdout")?);
        let mut line = String::new();
        output.read_line(&mut line)?;
        let bound: SocketAddr = line
            .trim()
            .strip_prefix("Audio upload receiver listening on http://")
            .context("CLI did not start the receiver")?
            .parse()?;
        assert!(bound.ip().is_unspecified());
        let address = SocketAddr::from(([127, 0, 0, 1], bound.port()));
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .no_proxy()
            .build()?;
        let health = client.get(format!("http://{address}/health")).send()?;
        assert_eq!(health.status(), reqwest::StatusCode::OK);
        assert_eq!(
            health.json::<serde_json::Value>()?,
            serde_json::json!({"status": "ok"})
        );
        let form = Form::new().text("durationMs", "850").part(
            "audio",
            Part::bytes(b"android audio".to_vec())
                .file_name("recording.wav")
                .mime_str("audio/wav")?,
        );
        let response = client
            .post(format!("http://{address}/forms/recordings"))
            .multipart(form)
            .send()?;
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
        let id = response.json::<serde_json::Value>()?["id"]
            .as_i64()
            .context("Missing recording ID")?;
        let database = rusqlite::Connection::open(&path)?;
        let audio: Vec<u8> = database.query_row(
            "SELECT audio_data FROM voice_recordings WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        assert_eq!(audio, b"android audio");
        child
            .stdin
            .take()
            .context("Missing CLI stdin")?
            .write_all(b"0\n")?;
        Ok(address)
    })();

    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let address = result?;
    assert!(status.success());
    assert!(TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err());
    Ok(())
}

#[test]
fn history_menu_filters_and_resets_the_overview() -> Result<()> {
    use chrono::NaiveDate;
    use koin::{CategoryType, Movement};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("history.db");
    let mut store = Store::create_new(&path)?;
    store.create_account("Banca", "EUR", 2000)?;
    store.create_account("Contanti", "EUR", 2000)?;
    for (account, category, day, description) in [
        ("Banca", "Alimentari", 23, "Spesa vecchia"),
        ("Banca", "Trasporti", 24, "Biglietto"),
        ("Banca", "Alimentari", 24, "Spesa nuova"),
        ("Contanti", "Alimentari", 24, "Mercato"),
    ] {
        store.add_movement(Movement {
            account,
            category,
            description: Some(description),
            kind: CategoryType::Expense,
            date: NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            amount: -100,
        })?;
    }
    let output = run(
        koin_with_database(&path),
        concat!(
            "5\n",
            "1\n1\n2\n",
            "1\n2\n3\n",
            "1\n3\n5\n2026-11-24\n2026-11-23\n2026-11-24\n",
            "1\n4\n",
            "0\n0\n",
        ),
    )?;
    let screens: Vec<_> = output.split("\nTransaction history\n").skip(1).collect();
    assert_eq!(screens.len(), 5);
    assert!(screens[0].find("Mercato").unwrap() < screens[0].find("Spesa vecchia").unwrap());
    assert!(!screens[1].contains("Mercato"));
    assert!(!screens[2].contains("Biglietto"));
    assert!(!screens[3].contains("Spesa vecchia"));
    assert!(screens[3].contains("Spesa nuova"));
    assert!(screens[4].contains("Mercato") && screens[4].contains("Spesa vecchia"));
    assert!(!output.contains("0. Exit") && !output.contains("0. Back"));
    Ok(())
}

#[test]
fn history_date_presets_use_rolling_dates_and_can_be_cleared() -> Result<()> {
    use chrono::{Days, Local, Months};
    use koin::{CategoryType, Movement};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("history.db");
    let mut store = Store::create_new(&path)?;
    store.create_account("Banca", "EUR", 2000)?;
    let today = Local::now().date_naive();
    for (date, label) in [
        (today, "Oggi"),
        (today - Days::new(6), "Sesto giorno"),
        (today - Days::new(7), "Settimo giorno"),
        (today - Months::new(2), "Due mesi"),
        (today - Months::new(6), "Sei mesi"),
        (today - Months::new(13), "Tredici mesi"),
        (today + Days::new(1), "Futuro"),
    ] {
        store.add_movement(Movement {
            account: "Banca",
            category: "Spesa",
            date,
            description: Some(label),
            kind: CategoryType::Expense,
            amount: -100,
        })?;
    }

    let output = run(
        koin_with_database(&path),
        concat!(
            "5\n",
            "1\n3\n1\n",
            "1\n3\n2\n",
            "1\n3\n3\n",
            "1\n3\n4\n",
            "1\n3\n6\n",
            "0\n0\n",
        ),
    )?;
    let views: Vec<_> = output.split("\nTransaction history\n").skip(1).collect();
    assert_eq!(views.len(), 6);
    assert!(views[1].contains("Sesto giorno") && !views[1].contains("Settimo giorno"));
    assert!(views[2].contains("Settimo giorno") && !views[2].contains("Due mesi"));
    assert!(views[3].contains("Due mesi") && !views[3].contains("Sei mesi"));
    assert!(views[4].contains("Sei mesi") && !views[4].contains("Tredici mesi"));
    assert!(
        views[1..5]
            .iter()
            .all(|view| view.contains("Oggi") && !view.contains("Futuro"))
    );
    assert!(views[5].contains("Tredici mesi") && views[5].contains("Futuro"));
    Ok(())
}

#[test]
fn report_menu_shows_expense_matrix_and_monthly_net() -> Result<()> {
    use chrono::Local;
    use koin::{CategoryType, Movement};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("report.db");
    let mut store = Store::create_new(&path)?;
    store.create_account("Banca", "EUR", 20_000)?;
    for (category, kind, amount) in [
        ("Casa", CategoryType::Expense, -1200),
        ("Prestito", CategoryType::Expense, -2400),
        ("Stipendio", CategoryType::Income, 2300),
    ] {
        store.add_movement(Movement {
            account: "Banca",
            category,
            kind,
            date: Local::now().date_naive(),
            amount,
            description: None,
        })?;
    }
    let previous_month = Local::now().date_naive() - chrono::Months::new(1);
    store.add_movement(Movement {
        account: "Banca",
        category: "Internet",
        kind: CategoryType::Expense,
        date: previous_month,
        amount: -400,
        description: None,
    })?;
    let output = run(koin_with_database(&path), "7\n1\n2\n0\n0\n")?;
    assert!(output.contains("Category | Monthly average"));
    assert!(output.contains("Casa | 1.00"));
    assert!(!output.contains("Prestito |"));
    assert!(output.contains("Monthly net income"));
    assert!(output.contains("+11.00"));
    assert!(output.contains("12-month net total: +7.00"));
    assert!(!output.contains("6-month average"));
    assert!(output.contains("████"));
    Ok(())
}

#[test]
fn account_overview_totals_balances_by_currency() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("balances.db");
    let store = Store::create_new(&path)?;
    store.create_account("Banca", "EUR", 12_500)?;
    store.create_account("Carta", "EUR", -500)?;
    store.create_account("Valuta estera", "USD", 6_000)?;

    let output = run(koin_with_database(&path), "1\n0\n0\n")?;
    assert!(output.contains("Total  120.00 EUR"));
    assert!(output.contains("Total  60.00 USD"));
    assert!(output.find("Total  60.00 USD").unwrap() < output.find("Banca  125.00 EUR").unwrap());
    Ok(())
}

#[test]
fn version_matches_release_version_without_opening_database_or_receiver() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_koin"))
        .arg("--version")
        .env("KOIN_UPLOAD_LISTEN", "invalid")
        .output()?;
    assert!(output.status.success());
    let release_version = include_str!("../VERSION").trim();
    assert_eq!(
        String::from_utf8(output.stdout)?.trim(),
        format!("koin {release_version}")
    );
    Ok(())
}

#[test]
fn set_current_balance_adjusts_opening_balance_without_changing_history() -> Result<()> {
    use chrono::NaiveDate;
    use koin::{CategoryType, Movement};

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("reconcile.db");
    let mut store = Store::create_new(&path)?;
    let bank = store.create_account("Bank", "EUR", 10_000)?;
    let cash = store.create_account("Cash", "EUR", 2_000)?;
    store.add_movement(Movement {
        account: "Bank",
        category: "Food",
        kind: CategoryType::Expense,
        date: NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
        amount: -1_500,
        description: None,
    })?;
    let history = store.recent_entries(10)?;
    let output = run(koin_with_database(&path), "1\n2\n1\n-3,50\n0\n0\n")?;
    assert!(output.contains("Balance set to -3.50 EUR."));
    assert!(output.contains("Total  16.50 EUR"));
    let reopened = Store::open_existing(&path)?;
    assert_eq!(reopened.accounts()?[0].initial_balance, 1_150);
    assert_eq!(reopened.account_balance(bank)?, -350);
    assert_eq!(reopened.account_balance(cash)?, 2_000);
    assert_eq!(reopened.recent_entries(10)?, history);
    assert!(store.reconcile_account_balance(bank, i64::MAX).is_err());
    assert_eq!(store.account_balance(bank)?, -350);
    Ok(())
}
