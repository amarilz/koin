# Koin

## Run locally

Build with Rust and Cargo. On Linux, install the ALSA development headers.
For a release binary, run `cargo build --locked --release`.
Use `cargo run --locked -- --version` to check the application version.

Run `cargo run --locked` from the repository. At first launch, choose whether
to create a SQLite database or open an existing Koin database. Press Enter
at the path prompt to use the suggested location. Koin remembers the selected
path. To open another existing database for one session, use
`cargo run --locked -- --database PATH`.

In a terminal, Koin opens an interactive screen. Use the arrow keys and Enter
to manage accounts and categories, record income and expenses, transfer money,
and view balances and recent entries. Press Esc to go back or cancel a form.
Enter amounts with a comma or a dot as the decimal separator.

With redirected input or output, Koin uses numbered menus that accept typed
choices. In this mode, type `/cancel` to cancel a form.

### Record audio

Open **Audio → Record from microphone** to start recording from the default
microphone. Press Enter to stop and save, or Esc to discard the recording.
On macOS, allow microphone access in **System Settings → Privacy & Security**
if prompted. On Linux, building CPAL requires the ALSA development headers.

Koin runs Whisper and then Ollama. Review or edit the proposed movement before
saving it to the ledger. Use **Review recordings** to reopen a transcription
or **Process pending recordings** to retry processing after an error.

At interactive startup, Koin starts Whisper with Docker Compose and asks
whether to start Ollama if the configured local endpoint is unavailable. The
default answer is **No**. You can start it later with **Audio → Start Ollama**.
Koin stops the process it started when the CLI exits. Whisper remains running
in Docker.
Receiver-only mode uses existing audio services.

The Whisper image must be available locally. By default, Compose mounts
`~/.cache/whisper` as the model directory. Set `WHISPER_MODELS_DIR` to use a
different directory. If Whisper is configured with a different URL, Koin uses that endpoint
directly. Koin can launch Ollama for a local HTTP endpoint.

The CLI reads the following environment variables.

| Variable | Default |
| --- | --- |
| `TRANSCRIPTION_SERVICE_URL` | `http://127.0.0.1:8000/transcribe` |
| `OLLAMA_BASE_URL` | `http://127.0.0.1:11434` |
| `OLLAMA_MODEL` | `qwen3.5-9b` |

### Receive audio over HTTP

Koin starts the HTTP receiver whenever the CLI opens its SQLite database.
From Android on the same network, send uploads directly to
`http://COMPUTER_IP:8080/forms/recordings`. The receiver listens on IPv4
interfaces by default and accepts connections from local addresses.
Set `KOIN_UPLOAD_LISTEN=127.0.0.1:8080` for access from the computer only.
For receiver-only mode, use `cargo run --locked -- serve-audio`.

The Android connection check can use `GET /health`, which returns HTTP 200
and `{"status":"ok"}` while the receiver is running.

Send a POST multipart form with an `audio` file (an `audio/*` media type,
up to 25 MiB) and a nonnegative integer `durationMs`. A committed upload
returns HTTP 201 and `{"id":123}`. Open **Audio → Review recordings**
in the CLI to review the result.

Clients can send an `Idempotency-Key` header and reuse it for retries of the
same upload. Reusing a key with different data returns HTTP 409. Without a
key, exact matches of audio bytes, media type, and duration return the
original ID, including after a restart. Use a new key to intentionally
submit identical data as a separate recording.

The existing Android client can upload without an authorization header.
If you configure `KOIN_UPLOAD_TOKEN`, clients must send
`Authorization: Bearer <token>`.

### Rust migration

The new business logic lives in the Rust library. Use `Store::open(path)` to
initialize an empty SQLite database or open an existing Koin database.
`Store::default_database_path()` returns the platform-specific local data path.

Run the integration suite with `cargo test --locked`. The HTTP contract test
uses a loopback server to simulate Whisper and Ollama.

The audio workflow starts with `Store::add_recording`. Pass a Whisper
transcription endpoint, an Ollama base URL, and a model name to the respective
clients, then call `Store::process_pending`. Review the saved proposal and pass
the corrected values to `Store::confirm_recording`, or use
`Store::discard_recording`.
