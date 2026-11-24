mod clients;

pub use clients::{Ollama, Whisper};

use crate::{Category, CategoryType, Movement, Store, Transfer, ledger};
use anyhow::{Context, Result, ensure};
use chrono::{Local, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior::Immediate, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extraction {
    pub account_name: Option<String>,
    pub account_to: Option<String>,
    pub category_type: Option<String>,
    pub category_name: Option<String>,
    pub amount: Option<f64>,
    pub occurred_at: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct Recording {
    pub id: i64,
    pub mime_type: String,
    pub duration_ms: i64,
    pub created_at_ms: i64,
    pub transcription: String,
    pub extraction: Option<Extraction>,
}

#[derive(Debug)]
pub struct ProcessingFailure {
    pub recording_id: i64,
    pub error: anyhow::Error,
}

pub trait Transcriber {
    fn transcribe(&self, audio: &[u8], mime_type: &str) -> Result<String>;
}

pub trait Extractor {
    fn extract(
        &self,
        text: &str,
        accounts: &[String],
        categories: &[Category],
    ) -> Result<Extraction>;
}

const REVIEW_FIELDS: &str = "id, mime_type, duration_ms, created_at, transcription, extraction";

fn read_recording(row: &Row<'_>) -> rusqlite::Result<Recording> {
    let extraction = row
        .get::<_, Option<String>>(5)?
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err))
        })?;
    Ok(Recording {
        id: row.get(0)?,
        mime_type: row.get(1)?,
        duration_ms: row.get(2)?,
        created_at_ms: row.get(3)?,
        transcription: row.get(4)?,
        extraction,
    })
}

impl Store {
    pub fn add_recording(&self, audio: &[u8], mime_type: &str, duration_ms: i64) -> Result<i64> {
        validate_recording(audio, mime_type, duration_ms)?;
        insert_recording(&self.connection, audio, mime_type, duration_ms)
    }

    pub fn save_upload(
        &mut self,
        key: &str,
        fingerprint: &str,
        audio: &[u8],
        mime_type: &str,
        duration_ms: i64,
    ) -> Result<UploadOutcome> {
        validate_recording(audio, mime_type, duration_ms)?;
        let tx = self.connection.transaction_with_behavior(Immediate)?;
        let receipt: Option<(String, i64)> = tx.query_row(
            "SELECT fingerprint, recording_id FROM audio_upload_receipts WHERE request_key = ?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        if let Some((original, id)) = receipt {
            return Ok(if original == fingerprint {
                UploadOutcome::Replayed(id)
            } else {
                UploadOutcome::Conflict
            });
        }
        let id = insert_recording(&tx, audio, mime_type, duration_ms)?;
        let inserted = tx.execute(
            "INSERT INTO audio_upload_receipts (request_key, fingerprint, recording_id) VALUES (?1, ?2, ?3)",
            params![key, fingerprint, id],
        )?;
        ensure!(inserted == 1, "Could not insert upload receipt");
        tx.commit()?;
        Ok(UploadOutcome::Created(id))
    }

    pub fn reviewable_recordings(&self) -> Result<Vec<Recording>> {
        let mut query = self.connection.prepare(&format!(
            "SELECT {REVIEW_FIELDS} FROM voice_recordings WHERE transcription IS NOT NULL
             ORDER BY created_at DESC, id DESC"
        ))?;
        Ok(query
            .query_map([], read_recording)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn recording_for_review(&self, id: i64) -> Result<Recording> {
        Ok(self.connection.query_row(&format!(
            "SELECT {REVIEW_FIELDS} FROM voice_recordings WHERE id = ?1 AND transcription IS NOT NULL"
        ), [id], read_recording)?)
    }

    pub fn review_audio(&self, id: i64) -> Result<(Vec<u8>, String)> {
        Ok(self.connection.query_row(
            "SELECT audio_data, mime_type FROM voice_recordings WHERE id = ?1 AND transcription IS NOT NULL",
            [id], |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub fn transcribe_recording(&self, id: i64, client: &impl Transcriber) -> Result<()> {
        let (audio, mime): (Vec<u8>, String) = self.connection.query_row(
            "SELECT audio_data, mime_type FROM voice_recordings WHERE id = ?1 AND transcription IS NULL",
            [id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).context("Recording missing or already transcribed")?;
        let text = client.transcribe(&audio, &mime)?;
        let text = text.trim();
        ensure!(!text.is_empty(), "Empty transcription");
        let changed = self.connection.execute(
            "UPDATE voice_recordings SET transcription = ?2 WHERE id = ?1 AND transcription IS NULL",
            params![id, text],
        )?;
        ensure!(changed == 1, "Recording missing or already transcribed");
        Ok(())
    }

    pub fn extract_recording(&self, id: i64, client: &impl Extractor) -> Result<()> {
        let recording = self.recording_for_review(id)?;
        if recording.extraction.is_some() {
            return Ok(());
        }
        let mut accounts = self
            .connection
            .prepare("SELECT name FROM accounts ORDER BY id")?;
        let names = accounts
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let extraction = client.extract(&recording.transcription, &names, &self.categories()?)?;
        let changed = self.connection.execute(
            "UPDATE voice_recordings SET extraction = ?2 WHERE id = ?1 AND extraction IS NULL",
            params![id, serde_json::to_string(&extraction)?],
        )?;
        ensure!(changed == 1, "Recording changed during extraction");
        Ok(())
    }

    pub fn process_pending(
        &self,
        transcriber: &impl Transcriber,
        extractor: &impl Extractor,
    ) -> Result<Vec<ProcessingFailure>> {
        let mut query = self.connection.prepare(
            "SELECT id, transcription IS NULL FROM voice_recordings
             WHERE extraction IS NULL ORDER BY created_at, id",
        )?;
        let pending = query
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut failures = Vec::new();
        for (id, needs_transcription) in pending {
            let transcription = if needs_transcription {
                self.transcribe_recording(id, transcriber)
            } else {
                Ok(())
            };
            if let Err(error) = transcription.and_then(|()| self.extract_recording(id, extractor)) {
                failures.push(ProcessingFailure {
                    recording_id: id,
                    error,
                });
            }
        }
        Ok(failures)
    }

    pub fn discard_recording(&self, id: i64) -> Result<()> {
        delete_reviewable(&self.connection, id)
    }

    pub fn confirm_recording(&mut self, id: i64, reviewed: &Extraction) -> Result<i64> {
        let account = reviewed
            .account_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .context("Account is required")?;
        let amount = reviewed
            .amount
            .filter(|value| *value > 0.0 && *value <= 1e12)
            .context("A positive amount is required")?;
        let cents = (amount * 100.0).round() as i64;
        ensure!(cents > 0, "Amount must be at least one cent");
        let date = match reviewed.occurred_at.as_deref() {
            Some(date) if !date.trim().is_empty() => {
                let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
                ensure!(
                    parsed.format("%Y-%m-%d").to_string() == date,
                    "Expected YYYY-MM-DD"
                );
                parsed
            }
            _ => Local::now().date_naive(),
        };
        let description = reviewed
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let tx = self.connection.transaction_with_behavior(Immediate)?;
        delete_reviewable(&tx, id)?;
        let transaction = match reviewed.category_type.as_deref() {
            Some(kind @ ("INCOME" | "EXPENSE")) => {
                let category = reviewed
                    .category_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .context("Category is required")?;
                let (kind, amount) = if kind == "INCOME" {
                    (CategoryType::Income, cents)
                } else {
                    (CategoryType::Expense, -cents)
                };
                ledger::add_movement(
                    &tx,
                    Movement {
                        account,
                        category,
                        kind,
                        date,
                        amount,
                        description,
                    },
                )?
            }
            Some("TRANSFER") => {
                let to = reviewed
                    .account_to
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .context("Destination account is required")?;
                ledger::add_transfer(
                    &tx,
                    Transfer {
                        from: account,
                        to,
                        date,
                        amount: cents,
                        description,
                    },
                )?
            }
            _ => anyhow::bail!("Invalid transaction type"),
        };
        tx.commit()?;
        Ok(transaction)
    }
}

fn delete_reviewable(connection: &rusqlite::Connection, id: i64) -> Result<()> {
    let changed = connection.execute(
        "DELETE FROM voice_recordings WHERE id = ?1 AND transcription IS NOT NULL",
        [id],
    )?;
    ensure!(changed == 1, "Recording missing or awaiting transcription");
    Ok(())
}

pub const MAX_RECORDING_SIZE: usize = 25 << 20;

#[derive(Debug, PartialEq, Eq)]
pub enum UploadOutcome {
    Created(i64),
    Replayed(i64),
    Conflict,
}

fn validate_recording(audio: &[u8], mime_type: &str, duration_ms: i64) -> Result<()> {
    ensure!(
        !audio.is_empty() && audio.len() <= MAX_RECORDING_SIZE,
        "Audio must be between 1 byte and 25 MiB"
    );
    ensure!(mime_type.starts_with("audio/"), "Invalid audio media type");
    ensure!(duration_ms >= 0, "Invalid recording duration");
    Ok(())
}

fn insert_recording(
    connection: &Connection,
    audio: &[u8],
    mime_type: &str,
    duration_ms: i64,
) -> Result<i64> {
    let inserted = connection.execute(
        "INSERT INTO voice_recordings (audio_data, mime_type, duration_ms, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![audio, mime_type, duration_ms, Utc::now().timestamp_millis()],
    )?;
    ensure!(inserted == 1, "Could not insert recording");
    Ok(connection.last_insert_rowid())
}
