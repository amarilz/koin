mod audio_services;
mod startup;
mod terminal;
mod upload;

use anyhow::{Context, Result, bail};
use koin::Store;
use std::{
    env, fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};
use terminal::{Console, FlowError};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Errore: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut arguments = env::args_os().skip(1);
    let explicit_path = match arguments.next().as_deref() {
        None => None,
        Some(flag) if flag == "--version" || flag == "-V" => {
            println!("koin {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(flag) if flag == "serve-audio" => return upload::run(arguments),
        Some(flag) if flag == "--help" || flag == "-h" => {
            println!(
                "Uso: koin [--database FILE]\n     koin serve-audio [--database FILE] [--listen IP:PORTA]\n     koin --version\n\nSenza argomenti, usa il database scelto al primo avvio."
            );
            return Ok(());
        }
        Some(flag) if flag == "--database" => Some(PathBuf::from(
            arguments.next().context("Specifica il file SQLite")?,
        )),
        Some(_) => bail!("Argomento sconosciuto. Usa koin --help"),
    };
    if arguments.next().is_some() {
        bail!("Troppi argomenti. Usa koin --help");
    }

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let mut console = Console::new(io::stdin().lock(), io::stdout());
    if interactive {
        console.use_alternate_screen()?;
    }
    let (store, database) = if let Some(path) = explicit_path {
        let store = Store::open_existing(&path)
            .with_context(|| format!("Impossibile aprire {}", path.display()))?;
        (store, path)
    } else {
        let settings = settings_path()?;
        let saved = match fs::read(&settings) {
            Ok(bytes) => match serde_json::from_slice::<PathBuf>(&bytes) {
                Ok(path) if path.is_absolute() => match Store::open_existing(&path) {
                    Ok(store) => Some((store, path)),
                    Err(error) => {
                        console.line(format_args!(
                            "Impossibile aprire {}: {error:#}",
                            path.display()
                        ))?;
                        None
                    }
                },
                _ => {
                    console.line("Il percorso salvato nelle impostazioni è invalido.")?;
                    None
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).context("Impossibile leggere le impostazioni"),
        };
        match saved {
            Some(store) => store,
            None => match setup(&mut console, &settings) {
                Ok(store) => store,
                Err(FlowError::Exit | FlowError::Cancel) => return Ok(()),
                Err(FlowError::Failure(error)) => return Err(error),
            },
        }
    };
    let receiver = upload::start_cli(&database, interactive)?;
    let mut audio_processes = startup::AudioProcesses::default();
    if interactive {
        match startup::start_default_services(&mut console)
            .and_then(|()| audio_processes.offer_ollama(&mut console))
        {
            Ok(()) => {}
            Err(FlowError::Exit | FlowError::Cancel) => return Ok(()),
            Err(FlowError::Failure(error)) => return Err(error),
        }
    }
    if interactive {
        console.line(format_args!(
            "Ricevitore Android attivo sulla porta {}.",
            receiver.address.port()
        ))?;
    } else {
        console.line(format_args!(
            "Caricamento audio in ascolto su http://{}",
            receiver.address
        ))?;
    }
    terminal::run(&mut console, store, &mut audio_processes)
}

fn settings_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("Impossibile trovare la cartella delle impostazioni")?
        .join("koin")
        .join("database.json"))
}

fn setup(
    console: &mut Console<impl io::BufRead, impl Write>,
    settings: &Path,
) -> terminal::Flow<(Store, PathBuf)> {
    let default = Store::default_database_path()?;
    let mut selected = 0;
    loop {
        let create = match console.menu(
            "Database di Koin",
            &["Crea un database", "Apri un database Koin esistente"],
            &mut selected,
        )? {
            0 => true,
            1 => false,
            _ => return Err(FlowError::Exit),
        };
        console.section(if create {
            "Crea un database"
        } else {
            "Apri un database Koin esistente"
        });
        let value = match console.prompt(format_args!("Percorso [{}]", default.display())) {
            Ok(value) => value,
            Err(FlowError::Cancel) => continue,
            Err(error) => return Err(error),
        };
        let path = if value.is_empty() {
            default.clone()
        } else if let Some(rest) = value.strip_prefix("~/") {
            dirs::home_dir()
                .context("Impossibile trovare la cartella utente")?
                .join(rest)
        } else {
            std::path::absolute(value)?
        };
        let opened = if create {
            Store::create_new(&path)
        } else {
            Store::open_existing(&path)
        };
        match opened {
            Ok(store) => {
                save_database_path(settings, &path)?;
                console.line(format_args!("Database: {}", path.display()))?;
                return Ok((store, path));
            }
            Err(error) => console.line(format_args!(
                "Impossibile usare {}: {error:#}",
                path.display()
            ))?,
        }
    }
}

fn save_database_path(settings: &Path, path: &Path) -> Result<()> {
    let parent = settings
        .parent()
        .context("Percorso impostazioni invalido")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, path)?;
    file.as_file().sync_all()?;
    file.persist(settings)?;
    Ok(())
}
