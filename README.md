# Koin

## Run locally

Requirements: Git, Go 1.25.6 or later, Make, and PostgreSQL 18 (installed locally or run with Docker).

Clone the project and enter its directory:

```sh
git clone https://github.com/amarilz/koin.git
cd koin
```

Download the Go dependencies:

```sh
go mod download
```

## Ollama

Voice recordings are transcribed and sent to Ollama for extraction when saved. If Ollama is unavailable, the recording remains pending; use the button on the recordings page to retry pending extractions. Ollama requests are queued and sent one at a time. Start Ollama in a separate terminal:

```sh
OLLAMA_HOST=0.0.0.0:11434 ollama serve
```

## Make commands

Run `make help` to see the available commands.
