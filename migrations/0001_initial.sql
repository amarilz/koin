CREATE TABLE accounts (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(name) <= 100),
    currency TEXT NOT NULL CHECK (length(currency) <= 3),
    initial_balance INTEGER NOT NULL
) STRICT;

CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(name) <= 100),
    kind TEXT NOT NULL CHECK (kind IN ('INCOME', 'EXPENSE', 'TRANSFER')),
    UNIQUE (name, kind)
) STRICT;

CREATE TABLE transactions (
    id INTEGER PRIMARY KEY,
    occurred_at TEXT NOT NULL
) STRICT;

CREATE TABLE transaction_entries (
    id INTEGER PRIMARY KEY,
    transaction_id INTEGER NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    amount INTEGER NOT NULL,
    description TEXT
) STRICT;

CREATE INDEX transactions_by_date ON transactions(occurred_at DESC);
CREATE INDEX entries_by_account ON transaction_entries(account_id);
CREATE INDEX entries_by_transaction ON transaction_entries(transaction_id);

CREATE TABLE voice_recordings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    audio_data BLOB NOT NULL,
    mime_type TEXT NOT NULL,
    duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0),
    created_at INTEGER NOT NULL,
    transcription TEXT,
    extraction TEXT
) STRICT;
