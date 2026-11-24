use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use rusqlite_migration::{M, Migrations};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Store {
    pub(crate) connection: Connection,
}

impl Store {
    pub fn default_database_path() -> Result<PathBuf> {
        Ok(dirs::data_local_dir()
            .context("Could not locate the local data directory")?
            .join("koin")
            .join("koin.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let connection =
            Connection::open(path).with_context(|| format!("Could not open {}", path.display()))?;
        Self::initialize(connection)
    }

    pub fn create_new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let result = Self::open(path);
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result
    }

    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let has_schema: bool = connection.query_row(
            "SELECT COUNT(*) = 5 FROM sqlite_schema WHERE type = 'table'
             AND name IN ('accounts', 'categories', 'transactions', 'transaction_entries', 'voice_recordings')",
            [],
            |row| row.get(0),
        )?;
        ensure!(has_schema, "Il file non contiene uno schema Koin");
        Self::initialize(connection)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        crate::ledger::register_balance_function(&connection)?;
        Migrations::new(vec![
            M::up(include_str!("../migrations/0001_initial.sql")),
            M::up(include_str!("../migrations/0002_audio_upload_receipts.sql")),
        ])
        .to_latest(&mut connection)
        .context("Could not migrate the Koin database")?;
        Ok(Self { connection })
    }
}
