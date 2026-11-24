use anyhow::{Context, Result, ensure};
use koin::Store;
use reqwest::{
    StatusCode,
    blocking::{
        Client, RequestBuilder, Response,
        multipart::{Form, Part},
    },
};
use rusqlite::Connection;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Server {
    child: Child,
    endpoint: String,
    client: Client,
}

impl Server {
    fn start(path: &Path, settings: &[(&str, &str)]) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .build()?;
        let child = Command::new(env!("CARGO_BIN_EXE_koin"))
            .args(["serve-audio", "--listen", "127.0.0.1:0", "--database"])
            .arg(path)
            .env_remove("KOIN_UPLOAD_TOKEN")
            .env("TRANSCRIPTION_SERVICE_URL", "invalid")
            .envs(settings.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut server = Self {
            child,
            endpoint: String::new(),
            client,
        };
        let stdout = server
            .child
            .stdout
            .take()
            .context("Missing server output")?;
        let (send, receive) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            let _ = send.send(BufReader::new(stdout).read_line(&mut line).map(|_| line));
        });
        let result = receive.recv_timeout(Duration::from_secs(5));
        if result.is_err() {
            let _ = server.child.kill();
        }
        reader.join().unwrap();
        let line = result??;
        let address = line
            .trim()
            .strip_prefix("Caricamento audio in ascolto su ")
            .context("Server did not start")?;
        server.endpoint = format!("{address}/forms/recordings");
        Ok(server)
    }

    fn post(&self) -> RequestBuilder {
        self.client.post(&self.endpoint)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn form(wav: Vec<u8>, mime: &str, duration: &str) -> Result<Form> {
    Ok(Form::new().text("durationMs", duration.to_owned()).part(
        "audio",
        Part::bytes(wav).file_name("voice.wav").mime_str(mime)?,
    ))
}

fn wav() -> Result<Form> {
    form(b"original audio".to_vec(), "audio/wav", "850")
}

fn id(response: Response) -> Result<i64> {
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>()?["id"]
        .as_i64()
        .context("Missing recording ID")
}

fn counts(db: &Connection) -> Result<(i64, i64)> {
    Ok(db.query_row(
        "SELECT (SELECT count(*) FROM voice_recordings), (SELECT count(*) FROM audio_upload_receipts)",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

#[test]
fn legacy_upload_retries_survive_server_restart_and_return_the_original_id() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    Store::create_new(&path)?;
    let database = Connection::open(&path)?;
    let server = Server::start(&path, &[])?;
    let original = id(server.post().multipart(wav()?).send()?)?;
    let payload: (Vec<u8>, String, i64) = database.query_row(
        "SELECT audio_data, mime_type, duration_ms FROM voice_recordings WHERE id = ?1",
        [original],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(
        payload,
        (b"original audio".to_vec(), "audio/wav".into(), 850)
    );
    drop(server);

    let server = Server::start(&path, &[])?;
    assert_eq!(id(server.post().multipart(wav()?).send()?)?, original);
    assert_eq!(counts(&database)?, (1, 1));
    database.execute(
        "UPDATE voice_recordings SET transcription = 'sample' WHERE id = ?1",
        [original],
    )?;
    Store::open_existing(&path)?.discard_recording(original)?;
    assert_eq!(id(server.post().multipart(wav()?).send()?)?, original);
    assert_eq!(counts(&database)?, (0, 1));
    Ok(())
}

#[test]
fn explicit_keys_deduplicate_concurrent_retries_and_allow_distinct_uploads() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    Store::create_new(&path)?;
    let server = Server::start(&path, &[])?;
    let (first, second) = thread::scope(|scope| {
        let upload = || -> Result<i64> {
            id(server
                .post()
                .header("Idempotency-Key", "first")
                .multipart(wav()?)
                .send()?)
        };
        let first = scope.spawn(upload);
        let second = scope.spawn(upload);
        (first.join().unwrap(), second.join().unwrap())
    });
    let first = first?;
    assert_eq!(first, second?);
    let conflict = server
        .post()
        .header("Idempotency-Key", "first")
        .multipart(form(b"other audio".to_vec(), "audio/wav", "850")?)
        .send()?;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let second = id(server
        .post()
        .header("Idempotency-Key", "second")
        .multipart(wav()?)
        .send()?)?;
    assert_ne!(first, second);
    assert_eq!(counts(&Connection::open(path)?)?, (2, 2));
    Ok(())
}

#[test]
fn invalid_uploads_and_failed_commits_leave_no_audio_or_receipt() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    Store::create_new(&path)?;
    let server = Server::start(&path, &[])?;
    let database = Connection::open(&path)?;
    for invalid in [
        Form::new().text("durationMs", "42"),
        form(vec![], "audio/wav", "0")?,
        form(vec![1], "text/plain", "0")?,
        form(vec![1], "audio/wav", "-1")?,
        form(vec![1], "audio/wav", "9223372036854775808")?,
        wav()?.text("durationMs", "0"),
    ] {
        assert_eq!(
            server.post().multipart(invalid).send()?.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        server.post().body("not multipart").send()?.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server
            .post()
            .header("Idempotency-Key", "bad key")
            .multipart(wav()?)
            .send()?
            .status(),
        StatusCode::BAD_REQUEST
    );
    let oversized = form(
        vec![0; koin::voice::MAX_RECORDING_SIZE + 1],
        "audio/wav",
        "0",
    )?;
    assert_eq!(
        server.post().multipart(oversized).send()?.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(counts(&database)?, (0, 0));

    database.execute_batch(
        "CREATE TABLE fail_commit (
            account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED
         );
         CREATE TRIGGER break_receipt_commit AFTER INSERT ON audio_upload_receipts
         BEGIN INSERT INTO fail_commit VALUES (-1); END;",
    )?;
    let retry = || server.post().header("Idempotency-Key", "commit-retry");
    assert_eq!(
        retry().multipart(wav()?).send()?.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(counts(&database)?, (0, 0));
    database.execute_batch("DROP TRIGGER break_receipt_commit;")?;
    let original = id(retry().multipart(wav()?).send()?)?;
    assert_eq!(id(retry().multipart(wav()?).send()?)?, original);
    assert_eq!(counts(&database)?, (1, 1));
    Ok(())
}

#[test]
fn configured_bearer_token_is_required_before_loading_an_upload() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    Store::create_new(&path)?;
    let server = Server::start(&path, &[("KOIN_UPLOAD_TOKEN", "test-upload")])?;
    assert_eq!(server.post().send()?.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        server
            .post()
            .bearer_auth("wrong")
            .multipart(wav()?)
            .send()?
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(counts(&Connection::open(&path)?)?, (0, 0));
    id(server
        .post()
        .bearer_auth("test-upload")
        .multipart(wav()?)
        .send()?)?;
    assert_eq!(counts(&Connection::open(path)?)?, (1, 1));
    Ok(())
}

#[test]
fn accepted_upload_is_processed_automatically_and_visible_to_the_cli_store() -> Result<()> {
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    let services = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", services.local_addr()?);
    let mock = thread::spawn(move || -> Result<()> {
        for (expected_path, response) in [
            ("/transcribe", r#"{"text":"Ho speso 7 euro"}"#),
            (
                "/api/chat",
                r#"{"message":{"content":"{\"accountName\":\"Banca\",\"categoryType\":\"EXPENSE\",\"categoryName\":\"Spesa\",\"amount\":7}"}}"#,
            ),
        ] {
            let (mut stream, _) = services.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            assert_eq!(line, format!("POST {expected_path} HTTP/1.1\r\n"));
            let mut content_length = None;
            loop {
                line.clear();
                ensure!(reader.read_line(&mut line)? != 0, "Incomplete HTTP request");
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = Some(value.trim().parse()?);
                }
            }
            let mut body = vec![0; content_length.context("Missing content length")?];
            reader.read_exact(&mut body)?;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )?;
        }
        Ok(())
    });
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let cli = Store::create_new(&path)?;
    cli.create_account("Banca", "EUR", 3000)?;
    let server = Server::start(
        &path,
        &[
            ("TRANSCRIPTION_SERVICE_URL", &format!("{base}/transcribe")),
            ("OLLAMA_BASE_URL", &base),
        ],
    )?;
    let recording = id(server.post().multipart(wav()?).send()?)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(record) = cli.recording_for_review(recording)
            && let Some(proposal) = record.extraction
        {
            assert_eq!(record.transcription, "Ho speso 7 euro");
            assert_eq!(proposal.amount, Some(7.0));
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Uploaded recording was never processed"
        );
        thread::sleep(Duration::from_millis(20));
    }
    mock.join().unwrap()?;
    assert!(cli.recent_entries(10)?.is_empty());
    Ok(())
}

#[test]
fn upgrading_an_existing_database_preserves_accounts_and_queued_audio() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("koin.db");
    let database = Connection::open(&path)?;
    database.execute_batch(include_str!("../migrations/0001_initial.sql"))?;
    database.pragma_update(None, "user_version", 1)?;
    database.execute("INSERT INTO accounts VALUES (42, 'Banca', 'EUR', 1200)", [])?;
    database.execute(
        "INSERT INTO voice_recordings (id, audio_data, mime_type, duration_ms, created_at, transcription)
         VALUES (85, ?1, 'audio/wav', 80, 0, 'original')",
        [b"original audio"],
    )?;
    let store = Store::open_existing(&path)?;
    assert_eq!(store.account_balance(42)?, 1200);
    assert_eq!(store.review_audio(85)?.0, b"original audio");
    assert_eq!(counts(&database)?, (1, 0));
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
        2
    );
    Ok(())
}
