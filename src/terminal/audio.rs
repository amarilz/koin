use super::{Console, Flow, FlowError, format_cents, parse_cents};
use crate::audio_services::{ollama, whisper};
use anyhow::Context;
use chrono::{DateTime, Local, NaiveDate};
use koin::{
    Store,
    voice::{Extraction, Recording},
};
use std::io::{BufRead, Write};

pub(super) fn run(console: &mut Console<impl BufRead, impl Write>, store: &mut Store) -> Flow<()> {
    let entries = [
        "Registra dal microfono",
        "Rivedi registrazioni",
        "Elabora registrazioni in attesa",
    ];
    let mut selected = 0;
    loop {
        let action = console.menu("Audio", &entries, &mut selected)?;
        let outcome = match action {
            0 => record(console, store),
            1 => review_recordings(console, store),
            2 => process_pending(console, store),
            _ => return Ok(()),
        };
        match outcome {
            Ok(()) | Err(FlowError::Cancel) => {}
            Err(FlowError::Exit) => return Err(FlowError::Exit),
            Err(FlowError::Failure(error)) => console.line(format_args!("Errore: {error:#}"))?,
        }
        console.show_results()?;
    }
}

fn record(console: &mut Console<impl BufRead, impl Write>, store: &mut Store) -> Flow<()> {
    let screen = console
        .screen
        .as_mut()
        .context("La registrazione richiede un terminale interattivo")?;
    let (wav, duration_ms) = screen.record()?;
    let id = store.add_recording(&wav, "audio/wav", duration_ms)?;
    console.section("Registrazione audio");
    activity(console, "Trascrizione con Whisper…")?;
    if let Err(error) = whisper().and_then(|client| store.transcribe_recording(id, &client)) {
        console.line(format_args!(
            "Registrazione #{id} salvata. Trascrizione in attesa: {error:#}"
        ))?;
        return Ok(());
    }
    activity(console, "Estrazione del movimento con Ollama…")?;
    if let Err(error) = ollama().and_then(|client| store.extract_recording(id, &client)) {
        console.line(format_args!(
            "Trascrizione salvata. Estrazione in attesa: {error:#}"
        ))?;
        console.show_results()?;
    }
    review(console, store, id)
}

fn process_pending(console: &mut Console<impl BufRead, impl Write>, store: &Store) -> Flow<()> {
    console.section("Elaborazione audio");
    let transcriber = whisper()?;
    let extractor = ollama()?;
    activity(console, "Elaborazione delle registrazioni in attesa…")?;
    let failures = store.process_pending(&transcriber, &extractor)?;
    if failures.is_empty() {
        console.line("Nessuna registrazione rimasta in attesa.")?;
    }
    for failure in failures {
        console.line(format_args!(
            "Registrazione #{} ancora in attesa: {:#}",
            failure.recording_id, failure.error
        ))?;
    }
    Ok(())
}

fn activity(console: &mut Console<impl BufRead, impl Write>, message: &str) -> Flow<()> {
    if let Some(screen) = &mut console.screen {
        screen.activity(&screen.title.clone(), message, "")?;
    } else {
        console.line(message)?;
        console.output.flush()?;
    }
    Ok(())
}

fn review_recordings(
    console: &mut Console<impl BufRead, impl Write>,
    store: &mut Store,
) -> Flow<()> {
    console.section("Rivedi registrazioni");
    let recordings = store.reviewable_recordings()?;
    if recordings.is_empty() {
        console.line("Nessuna registrazione trascritta da rivedere.")?;
        return Ok(());
    }
    let labels: Vec<_> = recordings
        .iter()
        .map(|recording| {
            let date = DateTime::from_timestamp_millis(recording.created_at_ms)
                .map(|date| {
                    date.with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            let text: String = recording
                .transcription
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(60)
                .collect();
            format!("#{}  {date}  {text}", recording.id)
        })
        .collect();
    let index = console.select("Registrazioni da rivedere", &labels)?;
    review(console, store, recordings[index].id)
}

fn review(console: &mut Console<impl BufRead, impl Write>, store: &mut Store, id: i64) -> Flow<()> {
    let recording = store.recording_for_review(id)?;
    let title = format!("Registrazione #{id}");
    let entries = [
        "Rivedi e conferma movimento",
        "Leggi trascrizione",
        "Scarta registrazione",
    ];
    let mut selected = 0;
    loop {
        console.section(&title);
        match console.menu(&title, &entries, &mut selected)? {
            0 => match review_movement(console, store, &recording) {
                Ok(true) => return Ok(()),
                Ok(false) | Err(FlowError::Cancel) => continue,
                Err(FlowError::Exit) => return Err(FlowError::Exit),
                Err(FlowError::Failure(error)) => {
                    console.line(format_args!("Errore: {error:#}"))?;
                    console.show_results()?;
                }
            },
            1 => {
                for line in recording.transcription.lines() {
                    console.line(line)?;
                }
                if recording.transcription.is_empty() {
                    console.line("Trascrizione vuota.")?;
                }
                console.show_results()?;
            }
            2 => match console.select("Scartare la registrazione?", &["Scarta"]) {
                Ok(_) => {
                    store.discard_recording(id)?;
                    console.line("Registrazione scartata.")?;
                    return Ok(());
                }
                Err(FlowError::Cancel) => {}
                Err(error) => return Err(error),
            },
            _ => return Ok(()),
        }
    }
}

fn review_movement(
    console: &mut Console<impl BufRead, impl Write>,
    store: &mut Store,
    recording: &Recording,
) -> Flow<bool> {
    const KINDS: [&str; 3] = ["INCOME", "EXPENSE", "TRANSFER"];
    let empty = Extraction::default();
    let suggestion = recording.extraction.as_ref().unwrap_or(&empty);
    console.section("Rivedi movimento");
    let mut selection = KINDS
        .iter()
        .position(|kind| Some(*kind) == suggestion.category_type.as_deref())
        .unwrap_or(0);
    let kind = console.menu(
        "Tipo movimento",
        &["Entrata", "Uscita", "Trasferimento"],
        &mut selection,
    )?;
    let Some(kind) = KINDS.get(kind).copied() else {
        return Ok(false);
    };
    let account = required(console, "Conto", suggestion.account_name.as_deref())?;
    let (account_to, category) = if kind == "TRANSFER" {
        (
            Some(required(
                console,
                "Conto di destinazione",
                suggestion.account_to.as_deref(),
            )?),
            None,
        )
    } else {
        (
            None,
            Some(required(
                console,
                "Categoria",
                suggestion.category_name.as_deref(),
            )?),
        )
    };
    let proposed_amount = suggestion
        .amount
        .filter(|amount| *amount > 0.0 && *amount <= 1e12)
        .map(|amount| format_cents((amount * 100.0).round() as i64));
    let amount = loop {
        let value = input(console, "Importo", proposed_amount.as_deref())?;
        if let Some(amount) =
            parse_cents(&value).filter(|amount| *amount > 0 && *amount <= 100_000_000_000_000)
        {
            break amount;
        }
        console.line("Inserisci un importo positivo con al massimo due decimali.")?;
    };
    let today = Local::now().date_naive().to_string();
    let date = loop {
        let value = input(
            console,
            "Data (AAAA-MM-GG)",
            suggestion.occurred_at.as_deref().or(Some(&today)),
        )?;
        if NaiveDate::parse_from_str(&value, "%Y-%m-%d").is_ok_and(|date| date.to_string() == value)
        {
            break value;
        }
        console.line("Usa il formato AAAA-MM-GG.")?;
    };
    let description = input(
        console,
        "Descrizione facoltativa",
        suggestion.description.as_deref(),
    )?;
    console.line(format_args!(
        "{date}   {account}   {}",
        format_cents(amount)
    ))?;
    if let Some(target) = account_to.as_ref().or(category.as_ref()) {
        console.line(target)?;
    }
    console.select("Confermare il movimento?", &["Salva movimento"])?;
    let transaction = store.confirm_recording(
        recording.id,
        &Extraction {
            account_name: Some(account),
            account_to,
            category_type: Some(kind.to_owned()),
            category_name: category,
            amount: Some(amount as f64 / 100.0),
            occurred_at: Some(date),
            description: Some(description),
        },
    )?;
    console.line(format_args!("Movimento registrato (#{transaction})."))?;
    Ok(true)
}

fn required(
    console: &mut Console<impl BufRead, impl Write>,
    label: &str,
    initial: Option<&str>,
) -> Flow<String> {
    loop {
        let value = input(console, label, initial)?;
        if !value.is_empty() && value.chars().count() <= 100 {
            return Ok(value);
        }
        console.line("Inserisci un nome da 1 a 100 caratteri.")?;
    }
}

fn input(
    console: &mut Console<impl BufRead, impl Write>,
    label: &str,
    initial: Option<&str>,
) -> Flow<String> {
    let initial = initial.unwrap_or_default();
    if let Some(screen) = &mut console.screen {
        return screen.prompt(label, initial);
    }
    if initial.is_empty() {
        return console.prompt(label);
    }
    let value = console.prompt(format_args!("{label} [{initial}]"))?;
    Ok(if value.is_empty() {
        initial.to_owned()
    } else {
        value
    })
}
