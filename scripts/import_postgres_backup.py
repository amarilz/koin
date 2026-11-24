#!/usr/bin/env python3
"""Import a Koin pg_dump archive into a new, verified SQLite database."""

import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]
TABLES = {
    "accounts": """
        SELECT id, name, currency::text AS currency, initial_balance FROM public.accounts
    """,
    "categories": """
        SELECT id, name, type AS kind FROM public.category
    """,
    "transactions": """
        SELECT id, occurred_at::text AS occurred_at FROM public.transactions
    """,
    "transaction_entries": """
        SELECT id, transaction_id, account_id, category_id, amount, description
        FROM public.transaction_entries
    """,
    "voice_recordings": """
        SELECT id, encode(audio_data, 'hex') AS audio_data, mime_type, duration_ms,
               floor(extract(epoch FROM created_at) * 1000)::bigint AS created_at,
               transcription, extraction
        FROM public.voice_recordings
    """,
}


def docker(*args, **options):
    return subprocess.run(["docker", *args], check=True, **options)


def query(container, sql):
    result = docker(
        "exec", container, "psql", "-XAt", "--set=ON_ERROR_STOP=1",
        "--username=postgres", "--dbname=postgres", "--command", sql,
        stdout=subprocess.PIPE, encoding="utf-8",
    )
    return result.stdout


def snapshot(container):
    data = {}
    for table, sql in TABLES.items():
        rows = query(container, f"SELECT row_to_json(source) FROM ({sql}) AS source ORDER BY id")
        data[table] = [json.loads(line) for line in rows.split("\n") if line]
    for row in data["voice_recordings"]:
        row["audio_data"] = bytes.fromhex(row["audio_data"])
        if row["extraction"] is not None:
            if not isinstance(json.loads(row["extraction"]), dict):
                raise ValueError(f"Invalid extraction for recording #{row['id']}")
    balance_rows = query(container, """
        SELECT row_to_json(balance)
        FROM (
            SELECT a.id, a.initial_balance::numeric + coalesce(sum(e.amount), 0) AS amount
            FROM public.accounts a LEFT JOIN public.transaction_entries e ON e.account_id = a.id
            GROUP BY a.id ORDER BY a.id
        ) AS balance
    """)
    balances = {row["id"]: row["amount"] for row in
                (json.loads(line) for line in balance_rows.split("\n") if line)}
    voice_sequence = query(container, """
        SELECT pg_sequence_last_value(pg_get_serial_sequence('public.voice_recordings', 'id')::regclass)
    """).strip()
    return data, balances, int(voice_sequence) if voice_sequence else 0


def restore_snapshot(backup):
    container = docker(
        "run", "--detach", "--rm", "--pull=never", "--network=none",
        "--env", "POSTGRES_HOST_AUTH_METHOD=trust", "postgres:18",
        stdout=subprocess.PIPE, text=True,
    ).stdout.strip()
    try:
        deadline = time.monotonic() + 30
        while subprocess.run(
            ["docker", "exec", container, "pg_isready", "-h", "127.0.0.1",
             "-U", "postgres", "-d", "postgres"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        ).returncode:
            if time.monotonic() >= deadline:
                raise RuntimeError("Temporary PostgreSQL did not become ready")
            time.sleep(0.25)
        with backup.open("rb") as stream:
            docker(
                "exec", "-i", container, "pg_restore", "--username=postgres",
                "--dbname=postgres", "--no-owner", "--no-acl", "--exit-on-error",
                "--single-transaction", stdin=stream,
            )
        return snapshot(container)
    finally:
        docker("stop", container, stdout=subprocess.DEVNULL)


def open_sqlite(path):
    connection = sqlite3.connect(path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute("PRAGMA synchronous = FULL")
    return connection


def verify(connection, source, expected_balances):
    for table, expected in source.items():
        actual = [dict(row) for row in connection.execute(f"SELECT * FROM {table} ORDER BY id")]
        if actual != expected:
            raise ValueError(f"Data mismatch in {table}")

    balances = {row["id"]: row["initial_balance"]
                for row in connection.execute("SELECT id, initial_balance FROM accounts")}
    for row in connection.execute("SELECT account_id, amount FROM transaction_entries"):
        balances[row["account_id"]] += row["amount"]
    if balances != expected_balances:
        raise ValueError("Account balances do not match PostgreSQL")
    if connection.execute("PRAGMA foreign_key_check").fetchone():
        raise ValueError("SQLite contains an invalid foreign key")
    if [row[0] for row in connection.execute("PRAGMA integrity_check")] != ["ok"]:
        raise ValueError("SQLite integrity check failed")


def import_database(destination, source, balances, voice_sequence):
    with tempfile.TemporaryDirectory(dir=destination.parent, prefix=".koin-import-") as directory:
        staging = Path(directory) / "staging.sqlite3"
        # SQLite's default permissions would expose a staged file after publication.
        staging.touch(mode=0o600)
        connection = open_sqlite(staging)
        try:
            migrations = sorted((ROOT / "migrations").glob("*.sql"))
            schema = "\n".join(path.read_text(encoding="utf-8") for path in migrations)
            connection.executescript(
                f"BEGIN IMMEDIATE;\n{schema}\nPRAGMA user_version = {len(migrations)};"
            )
            for table, rows in source.items():
                if not rows:
                    continue
                columns = ", ".join(rows[0])
                parameters = ", ".join(f":{column}" for column in rows[0])
                connection.executemany(f"INSERT INTO {table} ({columns}) VALUES ({parameters})", rows)
            if voice_sequence:
                updated = connection.execute(
                    "UPDATE sqlite_sequence SET seq = max(seq, ?) WHERE name = 'voice_recordings'",
                    [voice_sequence],
                )
                if not updated.rowcount:
                    connection.execute("INSERT INTO sqlite_sequence VALUES ('voice_recordings', ?)",
                                       [voice_sequence])
            verify(connection, source, balances)
            connection.commit()
        except BaseException:
            connection.rollback()
            raise
        finally:
            connection.close()

        reopened = open_sqlite(staging)
        try:
            verify(reopened, source, balances)
        finally:
            reopened.close()
        # Publish atomically without replacing a file created by another process.
        os.link(staging, destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("backup", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    if not args.backup.is_file():
        parser.error("backup file does not exist")
    if os.path.lexists(args.destination):
        parser.error("destination already exists")
    if not args.destination.parent.is_dir():
        parser.error("destination directory does not exist")

    source, balances, voice_sequence = restore_snapshot(args.backup)
    if any(not -(2**63) <= balance < 2**63 for balance in balances.values()):
        raise ValueError("PostgreSQL has a balance outside the range supported by Koin")
    import_database(args.destination, source, balances, voice_sequence)
    for table, rows in source.items():
        print(f"{table}: {len(rows)}")
    print(f"Verified {len(balances)} account balances and all imported rows.")
    print(f"SQLite: {args.destination}")


if __name__ == "__main__":
    main()
