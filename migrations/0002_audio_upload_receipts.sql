CREATE TABLE audio_upload_receipts (
    request_key TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    recording_id INTEGER NOT NULL
) STRICT;
