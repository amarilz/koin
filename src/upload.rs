use crate::{audio_services, settings_path};
use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, FromRequest, Multipart, Request, State,
        multipart::MultipartError,
    },
    http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use koin::{
    Store,
    voice::{MAX_RECORDING_SIZE, UploadOutcome},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs,
    net::{IpAddr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    thread,
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

#[derive(Clone)]
struct Server {
    database: Arc<Path>,
    token: Option<Arc<str>>,
    wake: SyncSender<()>,
    uploads: Arc<Semaphore>,
    log_errors: bool,
}

pub fn run(args: impl Iterator<Item = OsString>) -> Result<()> {
    let mut args = args;
    let mut database = None;
    let mut address = listen_address()?;
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--database") => {
                database = Some(PathBuf::from(
                    args.next().context("Specifica il file SQLite")?,
                ))
            }
            Some("--listen") => {
                address = args
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .context("Specifica l’indirizzo IP e la porta")?
                    .parse()
                    .context("Indirizzo di ascolto invalido")?;
            }
            Some("--help" | "-h") => {
                println!("Uso: koin serve-audio [--database FILE] [--listen IP:PORTA]");
                return Ok(());
            }
            _ => bail!("Argomento sconosciuto. Usa koin serve-audio --help"),
        }
    }
    let database = match database {
        Some(path) => path,
        None => serde_json::from_slice(
            &fs::read(settings_path()?)
                .context("Avvia prima Koin per scegliere il database, oppure usa --database")?,
        )?,
    };
    Store::open_existing(&database)?;
    let receiver = start(&database, address, true)?;
    println!(
        "Caricamento audio in ascolto su http://{}",
        receiver.address
    );
    receiver.runtime.block_on(receiver.task)??;
    Ok(())
}

pub struct Receiver {
    pub address: SocketAddr,
    runtime: tokio::runtime::Runtime,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

fn listen_address() -> Result<SocketAddr> {
    match env::var("KOIN_UPLOAD_LISTEN") {
        Ok(value) => value
            .parse()
            .context("KOIN_UPLOAD_LISTEN deve essere nel formato IP:PORTA"),
        Err(env::VarError::NotPresent) => Ok(([0, 0, 0, 0], 8080).into()),
        Err(error) => Err(error.into()),
    }
}

pub fn start_cli(database: &Path, interactive: bool) -> Result<Receiver> {
    start(database, listen_address()?, !interactive)
}

fn start(database: &Path, address: SocketAddr, log_errors: bool) -> Result<Receiver> {
    let database: Arc<Path> = fs::canonicalize(database)
        .context("Impossibile trovare il database Koin")?
        .into();
    let token = match env::var("KOIN_UPLOAD_TOKEN") {
        Ok(token) => {
            ensure!(
                !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_graphic()),
                "KOIN_UPLOAD_TOKEN deve contenere caratteri ASCII senza spazi"
            );
            Some(Arc::from(token))
        }
        Err(env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };

    let listener = TcpListener::bind(address)
        .with_context(|| format!("Impossibile avviare il ricevitore audio su {address}"))?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let listener = {
        let _guard = runtime.enter();
        tokio::net::TcpListener::from_std(listener)?
    };
    let wake = processing_worker(database.clone(), log_errors)?;
    let server = Server {
        database,
        token,
        wake,
        uploads: Arc::new(Semaphore::new(4)),
        log_errors,
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/forms/recordings", post(upload))
        .layer(DefaultBodyLimit::max(MAX_RECORDING_SIZE + (1 << 20)))
        .with_state(server);
    let task = runtime.spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
    });
    Ok(Receiver {
        address,
        runtime,
        task,
    })
}

fn processing_worker(database: Arc<Path>, log_errors: bool) -> Result<SyncSender<()>> {
    let (wake, wait) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("audio-processing".into())
        .spawn(move || {
            if let Err(error) = (|| -> Result<()> {
                let transcriber = audio_services::whisper()?;
                let extractor = audio_services::ollama()?;
                let store = Store::open_existing(&database)?;
                loop {
                    match store.process_pending(&transcriber, &extractor) {
                        Ok(failures) if log_errors => {
                            for failure in failures {
                                eprintln!(
                                    "Audio #{} in attesa: {:#}",
                                    failure.recording_id, failure.error
                                );
                            }
                        }
                        Err(error) if log_errors => eprintln!("Errore nella coda audio: {error:#}"),
                        _ => {}
                    }
                    match wait.recv_timeout(Duration::from_secs(60)) {
                        Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
            })() && log_errors
            {
                eprintln!("Elaborazione audio non disponibile: {error:#}");
            }
        })?;
    Ok(wake)
}

fn local_peer(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

async fn health(ConnectInfo(peer): ConnectInfo<SocketAddr>) -> Response {
    if !local_peer(peer.ip()) {
        return problem(
            StatusCode::FORBIDDEN,
            "il servizio è riservato alla rete locale",
        );
    }
    Json(json!({"status": "ok"})).into_response()
}

async fn upload(
    State(server): State<Server>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
) -> Response {
    if !local_peer(peer.ip()) {
        return problem(
            StatusCode::FORBIDDEN,
            "il caricamento è riservato alla rete locale",
        );
    }
    if let Some(token) = &server.token {
        let supplied = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !supplied.is_some_and(|supplied| bool::from(supplied.as_bytes().ct_eq(token.as_bytes())))
        {
            return (
                [(
                    axum::http::header::WWW_AUTHENTICATE,
                    HeaderValue::from_static("Bearer"),
                )],
                problem(
                    StatusCode::UNAUTHORIZED,
                    "token di caricamento mancante o non valido",
                ),
            )
                .into_response();
        }
    }
    let Ok(_permit) = server.uploads.try_acquire() else {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "troppi caricamenti in corso",
        );
    };
    let key = match idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error.into_response(),
    };
    let form =
        match tokio::time::timeout(Duration::from_secs(60), read_form(request, &server)).await {
            Ok(Ok(form)) => form,
            Ok(Err(error)) => return error.into_response(),
            Err(_) => {
                return problem(
                    StatusCode::REQUEST_TIMEOUT,
                    "caricamento interrotto per timeout",
                );
            }
        };
    let fingerprint = form.fingerprint();
    let key = key.map_or_else(
        || format!("payload:{fingerprint}"),
        |key| format!("key:{key}"),
    );
    let database = server.database.clone();
    let result = tokio::task::spawn_blocking(move || {
        Store::open_existing(database)?.save_upload(
            &key,
            &fingerprint,
            &form.audio,
            &form.mime_type,
            form.duration,
        )
    })
    .await;
    match result {
        Ok(Ok(UploadOutcome::Created(id) | UploadOutcome::Replayed(id))) => {
            let _ = server.wake.try_send(());
            (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
        }
        Ok(Ok(UploadOutcome::Conflict)) => problem(
            StatusCode::CONFLICT,
            "chiave di idempotenza già usata con dati diversi",
        ),
        Ok(Err(error)) => {
            if server.log_errors {
                eprintln!("Errore nel salvataggio audio: {error:#}");
            }
            problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossibile salvare la registrazione",
            )
        }
        Err(error) => {
            if server.log_errors {
                eprintln!("Errore nel salvataggio audio: {error}");
            }
            problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossibile salvare la registrazione",
            )
        }
    }
}

fn idempotency_key(headers: &HeaderMap) -> std::result::Result<Option<String>, UploadError> {
    let mut values = headers.get_all("Idempotency-Key").iter();
    let Some(key) = values.next() else {
        return Ok(None);
    };
    let key = key
        .to_str()
        .map_err(|_| UploadError::bad("chiave di idempotenza non valida"))?;
    if key.is_empty()
        || key.len() > 128
        || !key.bytes().all(|byte| byte.is_ascii_graphic())
        || values.next().is_some()
    {
        return Err(UploadError::bad("chiave di idempotenza non valida"));
    }
    Ok(Some(key.to_owned()))
}

struct Form {
    audio: Vec<u8>,
    mime_type: String,
    duration: i64,
}

impl Form {
    fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update((self.mime_type.len() as u64).to_be_bytes());
        hash.update(self.mime_type.as_bytes());
        hash.update(self.duration.to_be_bytes());
        hash.update(&self.audio);
        format!("{:x}", hash.finalize())
    }
}

async fn read_form(request: Request, server: &Server) -> std::result::Result<Form, UploadError> {
    let mut multipart = Multipart::from_request(request, server)
        .await
        .map_err(|_| UploadError::bad("form multipart mancante o non valido"))?;
    let (mut file, mut duration) = (None, None);
    while let Some(mut field) = multipart.next_field().await? {
        match field.name() {
            Some("audio") if file.is_none() => {
                let media_type = field
                    .content_type()
                    .filter(|mime| mime.starts_with("audio/"))
                    .ok_or_else(|| UploadError::bad("il file caricato non è un audio valido"))?
                    .to_owned();
                let mut audio = Vec::new();
                while let Some(chunk) = field.chunk().await? {
                    if audio.len() + chunk.len() > MAX_RECORDING_SIZE {
                        return Err(UploadError::too_large());
                    }
                    audio.extend_from_slice(&chunk);
                }
                if audio.is_empty() {
                    return Err(UploadError::bad("la registrazione è vuota"));
                }
                file = Some((audio, media_type));
            }
            Some("durationMs") if duration.is_none() => {
                let mut value = Vec::new();
                while let Some(chunk) = field.chunk().await? {
                    if value.len() + chunk.len() > 20 {
                        return Err(UploadError::bad("durata della registrazione non valida"));
                    }
                    value.extend_from_slice(&chunk);
                }
                duration = Some(
                    std::str::from_utf8(&value)
                        .ok()
                        .and_then(|value| value.parse::<i64>().ok())
                        .filter(|value| *value >= 0)
                        .ok_or_else(|| UploadError::bad("durata della registrazione non valida"))?,
                );
            }
            Some("audio" | "durationMs") => {
                return Err(UploadError::bad("campi multipart duplicati"));
            }
            _ => {}
        }
    }
    let (audio, mime_type) =
        file.ok_or_else(|| UploadError::bad("file audio mancante o non valido"))?;
    let duration =
        duration.ok_or_else(|| UploadError::bad("durata della registrazione non valida"))?;
    Ok(Form {
        audio,
        mime_type,
        duration,
    })
}

struct UploadError(StatusCode, &'static str);

impl UploadError {
    fn bad(message: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }

    fn too_large() -> Self {
        Self(
            StatusCode::PAYLOAD_TOO_LARGE,
            "registrazione troppo grande (massimo 25 MiB)",
        )
    }
}

impl From<MultipartError> for UploadError {
    fn from(error: MultipartError) -> Self {
        if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            Self::too_large()
        } else {
            Self::bad("form multipart mancante o non valido")
        }
    }
}

impl IntoResponse for UploadError {
    fn into_response(self) -> Response {
        problem(self.0, self.1)
    }
}

fn problem(status: StatusCode, message: &'static str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}
