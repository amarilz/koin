use super::{Extraction, Extractor, Transcriber};
use crate::Category;
use anyhow::{Context, Result, ensure};
use reqwest::{
    Url,
    blocking::{Client, Response, multipart},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use std::{io::Read, time::Duration};

const MAX_RESPONSE_BYTES: u64 = 1 << 20;

pub struct Whisper {
    client: Client,
    endpoint: Url,
}

impl Whisper {
    pub fn new(endpoint: &str) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(600))
                .build()?,
            endpoint: http_url(endpoint)?,
        })
    }
}

impl Transcriber for Whisper {
    fn transcribe(&self, audio: &[u8], mime_type: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Transcript {
            text: String,
        }
        let media_type = mime_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let extension = match media_type.as_str() {
            "audio/webm" => "webm",
            "audio/mp4" => "mp4",
            "audio/mpeg" => "mp3",
            "audio/wav" | "audio/x-wav" => "wav",
            "audio/ogg" => "ogg",
            _ => "audio",
        };
        let part = multipart::Part::bytes(audio.to_vec())
            .file_name(format!("recording.{extension}"))
            .mime_str(mime_type)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .multipart(multipart::Form::new().part("file", part))
            .send()
            .context("Could not call Whisper")?;
        let transcript: Transcript =
            read_json(response).context("Could not decode Whisper response")?;
        Ok(transcript.text.trim().to_owned())
    }
}

pub struct Ollama {
    client: Client,
    endpoint: Url,
    model: String,
}

impl Ollama {
    pub fn new(base_url: &str, model: &str) -> Result<Self> {
        ensure!(!model.trim().is_empty(), "Ollama model is required");
        let mut endpoint = http_url(base_url)?;
        ensure!(
            endpoint.query().is_none() && endpoint.fragment().is_none(),
            "Invalid Ollama base URL"
        );
        endpoint
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid Ollama base URL"))?
            .pop_if_empty()
            .extend(["api", "chat"]);
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(180))
                .build()?,
            endpoint,
            model: model.to_owned(),
        })
    }
}

impl Extractor for Ollama {
    fn extract(
        &self,
        text: &str,
        accounts: &[String],
        categories: &[Category],
    ) -> Result<Extraction> {
        #[derive(Deserialize)]
        struct Completion {
            message: Message,
        }
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }
        let input =
            json!({ "accounts": accounts, "categories": categories, "transcription": text });
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(&json!({
                "model": self.model,
                "stream": false,
                "think": false,
                "format": "json",
                "options": { "temperature": 0, "num_predict": 256 },
                "messages": [
                    { "role": "system", "content": include_str!("../extraction_prompt.txt") },
                    { "role": "user", "content": input.to_string() }
                ]
            }))
            .send()
            .context("Could not call Ollama")?;
        let result: Completion = read_json(response).context("Could not decode Ollama response")?;
        serde_json::from_str(&result.message.content)
            .context("Could not decode extracted transaction")
    }
}

fn http_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.has_host(),
        "Expected an HTTP(S) URL"
    );
    Ok(url)
}

fn read_json<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    ensure!(status.is_success(), "Service returned HTTP {status}");
    let mut body = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)?;
    ensure!(
        body.len() as u64 <= MAX_RESPONSE_BYTES,
        "Service response exceeds 1 MiB"
    );
    Ok(serde_json::from_slice(&body)?)
}
