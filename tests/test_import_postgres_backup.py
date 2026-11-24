from pathlib import Path
import sqlite3
import tempfile
import unittest

from scripts.import_postgres_backup import import_database, open_sqlite


class PostgresBackupImportTests(unittest.TestCase):
    def snapshot(self):
        return {
            "accounts": [
                dict(id=2, name="Banca", currency="EUR", initial_balance=1000),
                dict(id=4, name="Contanti", currency="EUR", initial_balance=100),
            ],
            "categories": [dict(id=3, name="Caffè", kind="EXPENSE")],
            "transactions": [dict(id=5, occurred_at="2026-11-24")],
            "transaction_entries": [
                dict(id=6, transaction_id=5, account_id=2, category_id=None,
                     amount=-350, description=None),
                dict(id=7, transaction_id=5, account_id=4, category_id=None,
                     amount=350, description=None),
            ],
            "voice_recordings": [
                dict(id=8, audio_data=b"\x00\xffWAV", mime_type="audio/wav",
                     duration_ms=10, created_at=9, transcription="Tre euro e cinquanta",
                     extraction='{"amount":3.5}'),
            ],
        }

    def test_imports_exact_values_and_preserves_sequence(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "koin.sqlite3"
            import_database(target, self.snapshot(), {2: 650, 4: 450}, 50)
            database = open_sqlite(target)
            try:
                row = database.execute("SELECT * FROM voice_recordings").fetchone()
                self.assertEqual(dict(row), self.snapshot()["voice_recordings"][0])
                sequence = database.execute(
                    "SELECT seq FROM sqlite_sequence WHERE name = 'voice_recordings'"
                ).fetchone()
                self.assertEqual(sequence[0], 50)
            finally:
                database.close()

    def test_failed_verification_and_constraint_leave_no_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "koin.sqlite3"
            with self.assertRaisesRegex(ValueError, "Account balances"):
                import_database(target, self.snapshot(), {2: 651, 4: 450}, 50)
            self.assertFalse(target.exists())
            broken = self.snapshot()
            broken["transaction_entries"][0]["account_id"] = 999
            with self.assertRaises(sqlite3.IntegrityError):
                import_database(target, broken, {2: 650, 4: 450}, 50)
            self.assertFalse(target.exists())

    def test_never_replaces_an_existing_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "koin.sqlite3"
            target.write_bytes(b"existing file")
            with self.assertRaises(FileExistsError):
                import_database(target, self.snapshot(), {2: 650, 4: 450}, 50)
            self.assertEqual(target.read_bytes(), b"existing file")
