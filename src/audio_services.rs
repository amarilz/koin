use anyhow::Result;
use koin::voice::{Ollama, Whisper};
use std::env;

pub const LOCAL_WHISPER_URL: &str = "http://127.0.0.1:8000/transcribe";
pub const LOCAL_OLLAMA_URL: &str = "http://127.0.0.1:11434";

pub fn whisper_url() -> String {
    setting("TRANSCRIPTION_SERVICE_URL", LOCAL_WHISPER_URL)
}

pub fn ollama_url() -> String {
    setting("OLLAMA_BASE_URL", LOCAL_OLLAMA_URL)
}

pub fn whisper() -> Result<Whisper> {
    Whisper::new(&whisper_url())
}

pub fn ollama() -> Result<Ollama> {
    Ollama::new(&ollama_url(), &setting("OLLAMA_MODEL", "qwen3.5-9b"))
}

fn setting(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}
