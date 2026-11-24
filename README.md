# Koin

Koin is a personal finance app for the terminal. Track account balances, record
income and expenses, transfer money between accounts, and review monthly reports.
Your data is stored locally in SQLite. You can also record a voice note or send
one from Android, then review and confirm the suggested transaction.

## Getting started

Install Koin from the repository with Rust and Cargo:

```sh
cargo install --path .
koin
```

On Linux, installation requires the ALSA development headers. On first launch,
choose a SQLite file to create or open. Koin remembers your choice.
Use the arrow keys and Enter to navigate, and Esc to go back or exit.

## Voice notes

Whisper transcribes the recording and Ollama suggests a transaction. Choose
**Audio → Record from microphone** to record locally, or send audio from
Android on the same network to `http://COMPUTER_IP:8080/forms/recordings`.
Use `GET /health` to check the connection and **Audio → Review recordings**
to review and confirm the result.

For local transcription, have Docker Compose, the
`whisper-transcription-service:1.0.0` image, and the Whisper model
`large-v3-turbo.pt` in `~/.cache/whisper`. Use `WHISPER_MODELS_DIR` to change
the model directory. Koin starts Whisper automatically and asks whether to
start Ollama if it is unavailable. You can also start Ollama later from the
**Audio** menu. Koin shuts down the audio services it started when you exit.

| Environment variable | Default |
| --- | --- |
| `TRANSCRIPTION_SERVICE_URL` | `http://127.0.0.1:8000/transcribe` |
| `OLLAMA_BASE_URL` | `http://127.0.0.1:11434` |
| `OLLAMA_MODEL` | `qwen3.5-9b` |
