use anyhow::Result;
use koin::voice::{Ollama, Whisper};
use std::env;

pub fn whisper() -> Result<Whisper> {
    Whisper::new(&setting(
        "TRANSCRIPTION_SERVICE_URL",
        "http://127.0.0.1:8000/transcribe",
    ))
}

pub fn ollama() -> Result<Ollama> {
    Ollama::new(
        &setting("OLLAMA_BASE_URL", "http://127.0.0.1:11434"),
        &setting("OLLAMA_MODEL", "qwen3.5-9b"),
    )
}

fn setting(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}
