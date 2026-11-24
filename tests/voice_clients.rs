use anyhow::{Context, Result, ensure};
use koin::{
    CategoryType, Store,
    voice::{Ollama, Whisper},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

struct Request {
    line: String,
    content_type: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut impl Read) -> Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut content_type = String::new();
    let mut length = None;
    loop {
        let mut header = String::new();
        ensure!(
            reader.read_line(&mut header)? > 0,
            "Incomplete HTTP headers"
        );
        if header == "\r\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.to_ascii_lowercase().as_str() {
                "content-type" => content_type = value.trim().to_owned(),
                "content-length" => length = Some(value.trim().parse()?),
                _ => {}
            }
        }
    }
    let mut body = vec![0; length.context("Missing Content-Length")?];
    reader.read_exact(&mut body)?;
    Ok(Request {
        line,
        content_type,
        body,
    })
}

#[test]
fn whisper_and_ollama_use_the_expected_http_contracts() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let server = thread::spawn(move || -> Result<()> {
        for response in [
            json!({"text": "  Ho speso cinque euro  "}),
            json!({"message": {"content": json!({
                "accountName": "Contanti", "categoryType": "EXPENSE", "categoryName": "Spesa",
                "amount": 5, "occurredAt": "2026-11-20"
            }).to_string()}}),
        ] {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let request = read_request(&mut stream)?;
            if response.get("text").is_some() {
                assert_eq!(request.line, "POST /transcribe HTTP/1.1\r\n");
                assert!(
                    request
                        .content_type
                        .starts_with("multipart/form-data; boundary=")
                );
                let body = String::from_utf8(request.body)?;
                assert!(body.contains("name=\"file\"; filename=\"recording.wav\""));
                assert!(
                    body.contains("Content-Type: audio/wav")
                        || body.contains("content-type: audio/wav")
                );
                assert!(body.contains("\r\n\r\naudio-test\r\n"));
            } else {
                assert_eq!(request.line, "POST /api/chat HTTP/1.1\r\n");
                assert_eq!(request.content_type, "application/json");
                let request: Value = serde_json::from_slice(&request.body)?;
                assert_eq!(request["model"], "llama");
                assert_eq!(request["format"], "json");
                assert_eq!(request["stream"], false);
                assert_eq!(request["think"], false);
                let input: Value =
                    serde_json::from_str(request["messages"][1]["content"].as_str().unwrap())?;
                assert_eq!(
                    input,
                    json!({
                        "transcription": "Ho speso cinque euro", "accounts": ["Contanti"],
                        "categories": [{"name": "Spesa", "type": "EXPENSE"}],
                    })
                );
            }
            let response = response.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )?;
        }
        Ok(())
    });

    let mut store = Store::open(":memory:")?;
    let account = store.create_account("Contanti", "EUR", 2_000)?;
    store.create_category("Spesa", CategoryType::Expense)?;
    let id = store.add_recording(b"audio-test", "audio/wav", 800)?;
    let failures = store.process_pending(
        &Whisper::new(&format!("{base}/transcribe"))?,
        &Ollama::new(&base, "llama")?,
    )?;
    assert!(failures.is_empty(), "{failures:?}");
    server.join().expect("Local test server panicked")?;
    let extraction = store
        .recording_for_review(id)?
        .extraction
        .context("Missing extraction")?;
    store.confirm_recording(id, &extraction)?;
    assert_eq!(store.account_balance(account)?, 1_500);
    Ok(())
}

#[test]
fn failed_http_requests_and_invalid_responses_preserve_every_completed_stage() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let server = thread::spawn(move || -> Result<()> {
        let successful_proposal = json!({"message": {"content": json!({
            "accountName": "Contanti", "categoryType": "EXPENSE",
            "categoryName": "Pranzo", "amount": 7.25, "occurredAt": "2026-11-24",
        }).to_string()}})
        .to_string();
        let cases = [
            ("/transcribe", "503 Service Unavailable", "busy"),
            ("/transcribe", "200 OK", r#"{"unexpected":"field"}"#),
            ("/transcribe", "200 OK", r#"{"text":"Ho speso 7,25 euro"}"#),
            ("/api/chat", "502 Bad Gateway", "unavailable"),
            (
                "/api/chat",
                "200 OK",
                r#"{"message":{"content":"not JSON"}}"#,
            ),
            ("/api/chat", "200 OK", successful_proposal.as_str()),
        ];
        for (path, status, body) in cases {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let request = read_request(&mut stream)?;
            assert_eq!(request.line, format!("POST {path} HTTP/1.1\r\n"));
            if path == "/transcribe" {
                assert!(
                    request
                        .body
                        .windows(b"saved audio".len())
                        .any(|part| part == b"saved audio")
                );
            } else {
                let request: Value = serde_json::from_slice(&request.body)?;
                let content = request["messages"][1]["content"]
                    .as_str()
                    .context("Missing prompt")?;
                let input: Value = serde_json::from_str(content)?;
                assert_eq!(input["transcription"], "Ho speso 7,25 euro");
            }
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
        Ok(())
    });

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("audio.sqlite3");
    let store = Store::create_new(&path)?;
    let cash = store.create_account("Contanti", "EUR", 2000)?;
    let id = store.add_recording(b"saved audio", "audio/wav", 888)?;
    drop(store);

    let whisper = Whisper::new(&format!("{base}/transcribe"))?;
    let ollama = Ollama::new(&base, "llama")?;
    for (stage, expected_error) in [
        (0, "503"),
        (1, "missing field"),
        (2, "502"),
        (3, "Could not decode extracted transaction"),
    ] {
        let store = Store::open_existing(&path)?;
        let failures = store.process_pending(&whisper, &ollama)?;
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].recording_id, id);
        let error = format!("{:#}", failures[0].error);
        assert!(error.contains(expected_error), "{error}");
        assert!(store.recent_entries(10)?.is_empty());
        assert_eq!(store.account_balance(cash)?, 2000);
        let raw = rusqlite::Connection::open(&path)?;
        let stored: (Vec<u8>, i64) = raw.query_row(
            "SELECT audio_data, duration_ms FROM voice_recordings WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(stored, (b"saved audio".to_vec(), 888));
        if stage < 2 {
            assert!(store.reviewable_recordings()?.is_empty());
        } else {
            let review = store.recording_for_review(id)?;
            assert_eq!(review.transcription, "Ho speso 7,25 euro");
            assert!(review.extraction.is_none());
        }
    }
    let mut store = Store::open_existing(&path)?;
    assert!(store.process_pending(&whisper, &ollama)?.is_empty());
    server.join().expect("Local test server panicked")?;
    let proposal = store
        .recording_for_review(id)?
        .extraction
        .context("Missing proposal")?;
    store.confirm_recording(id, &proposal)?;
    let reopened = Store::open_existing(path)?;
    assert_eq!(reopened.account_balance(cash)?, 1275);
    assert!(reopened.reviewable_recordings()?.is_empty());
    assert_eq!(reopened.recent_entries(10)?.len(), 1);
    Ok(())
}
