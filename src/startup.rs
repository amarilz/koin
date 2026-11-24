use crate::{
    audio_services::{LOCAL_WHISPER_URL, ollama_url, whisper_url},
    terminal::{Console, Flow},
};
use anyhow::{Context, Result, bail};
use reqwest::{Url, blocking::Client};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const COMPOSE: &str = include_str!("../compose.yaml");
const LOG_CAPACITY: usize = 200;

#[derive(Clone, Default)]
pub struct OllamaLogs(Arc<Mutex<VecDeque<String>>>);

impl OllamaLogs {
    pub fn snapshot(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    fn append(&self, line: impl Into<String>) {
        let mut lines = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if lines.len() == LOG_CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line.into());
    }

    fn capture(&self, stream: impl Read + Send + 'static) {
        let logs = self.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut bytes = Vec::new();
            while reader
                .read_until(b'\n', &mut bytes)
                .is_ok_and(|count| count > 0)
            {
                let line = String::from_utf8_lossy(&bytes);
                let entry: String = line.trim_end().chars().take(2000).collect();
                if !entry.is_empty() {
                    logs.append(entry);
                }
                bytes.clear();
            }
        });
    }
}

#[derive(Default)]
pub struct AudioProcesses {
    ollama: Option<Child>,
    pub logs: OllamaLogs,
}

impl Drop for AudioProcesses {
    fn drop(&mut self) {
        if let Some(mut child) = self.ollama.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn start_default_services(console: &mut Console<impl BufRead, impl Write>) -> Flow<()> {
    if whisper_url() == LOCAL_WHISPER_URL {
        console.startup_status("Avvio di Whisper con Docker…")?;
        match start_whisper() {
            Ok(()) => console.line("Whisper avviato.")?,
            Err(error) => console.line(format_args!("Whisper non avviato: {error:#}"))?,
        }
    }
    Ok(())
}

impl AudioProcesses {
    pub fn offer_ollama(&mut self, console: &mut Console<impl BufRead, impl Write>) -> Flow<()> {
        let base = ollama_url();
        if ollama_bind(&base).is_none() {
            self.logs
                .append("Endpoint Ollama configurato esternamente.");
            return Ok(());
        }
        if ollama_ready(&base) {
            console.line("Ollama è già in esecuzione.")?;
            if self.ollama.is_none() {
                self.logs.append("Ollama è già in esecuzione. I log dei processi esterni non sono disponibili qui.");
            }
            return Ok(());
        }
        if console.menu("Avviare Ollama?", &["No", "Sì, avvia Ollama"], &mut 0)? == 1 {
            match self.start_ollama(console) {
                Ok(()) => {}
                Err(crate::terminal::FlowError::Failure(error)) => {
                    console.line(format_args!("Ollama non disponibile: {error:#}"))?;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub fn start_ollama(&mut self, console: &mut Console<impl BufRead, impl Write>) -> Flow<()> {
        let base = ollama_url();
        if ollama_ready(&base) {
            console.line("Ollama è già in esecuzione.")?;
            if self.ollama.is_none() {
                self.logs.append("Ollama è già in esecuzione. I log dei processi esterni non sono disponibili qui.");
            }
            return Ok(());
        }
        let bind = ollama_bind(&base)
            .context("Ollama può essere avviato da Koin solo per un indirizzo HTTP locale")?;
        console.section("Avvio di Ollama");
        console.startup_status("Avvio di Ollama…")?;
        start_ollama(&base, bind, self)?;
        console.line("Ollama avviato.")?;
        Ok(())
    }
}

fn docker_ready() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn start_whisper() -> Result<()> {
    let status = Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Docker e il plugin Compose devono essere installati")?;
    if !status.success() {
        bail!("Il plugin Docker Compose non è disponibile");
    }
    if !docker_ready() {
        start_docker()?;
    }
    let mut child = Command::new("docker")
        .args([
            "compose",
            "--project-name",
            "koin",
            "--file",
            "-",
            "up",
            "-d",
            "--pull",
            "never",
            "whisper",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Impossibile avviare Docker Compose")?;
    let write = child
        .stdin
        .take()
        .context("Docker Compose non accetta input")?
        .write_all(COMPOSE.as_bytes());
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("errore sconosciuto").trim();
        bail!("{detail}");
    }
    write?;
    Ok(())
}

fn start_docker() -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("Avvia Docker e riapri Koin");
    }
    let status = Command::new("open")
        .args(["-a", "Docker"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Impossibile aprire Docker Desktop")?;
    if !status.success() {
        bail!("Impossibile aprire Docker Desktop");
    }
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        if docker_ready() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(450));
    }
    bail!("Docker Desktop non è ancora pronto");
}

fn ollama_bind(base: &str) -> Option<SocketAddr> {
    let url = Url::parse(base).ok()?;
    if url.scheme() != "http"
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = match url.host_str()? {
        "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        host => host.trim_matches(['[', ']']).parse::<IpAddr>().ok()?,
    };
    if !host.is_loopback() {
        return None;
    }
    Some(SocketAddr::new(host, url.port()?))
}

fn ollama_ready(base: &str) -> bool {
    let Ok(mut endpoint) = Url::parse(base) else {
        return false;
    };
    endpoint.set_path("/api/tags");
    Client::builder()
        .timeout(Duration::from_millis(500))
        .no_proxy()
        .build()
        .and_then(|client| client.get(endpoint).send())
        .is_ok_and(|response| response.status().is_success())
}

fn start_ollama(base: &str, bind: SocketAddr, processes: &mut AudioProcesses) -> Result<()> {
    let running = match processes.ollama.as_mut() {
        Some(child) => child.try_wait()?.is_none(),
        None => false,
    };
    if !running {
        let mut child = Command::new("ollama")
            .arg("serve")
            .env("OLLAMA_HOST", bind.to_string())
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Impossibile eseguire ollama serve")?;
        if let Some(stdout) = child.stdout.take() {
            processes.logs.capture(stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            processes.logs.capture(stderr);
        }
        processes.ollama = Some(child);
    }
    let child = processes
        .ollama
        .as_mut()
        .context("Processo Ollama non disponibile")?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if ollama_ready(base) {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!("Ollama si è chiuso durante l’avvio ({status})");
        }
        thread::sleep(Duration::from_millis(250));
    }
    bail!("Ollama non ha ancora risposto su {bind}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_buffer_keeps_the_most_recent_lines_in_order() {
        let logs = OllamaLogs::default();
        let reader = logs.clone();
        for index in 0..LOG_CAPACITY + 10 {
            logs.append(index.to_string());
        }
        let snapshot = reader.snapshot();
        assert_eq!(snapshot.len(), LOG_CAPACITY);
        assert_eq!(snapshot.first().map(String::as_str), Some("10"));
        assert_eq!(snapshot.last(), Some(&(LOG_CAPACITY + 9).to_string()));
    }
}
