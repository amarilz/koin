CREATE TABLE voice_recordings
(
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT       NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    audio_data  BYTEA        NOT NULL,
    mime_type   VARCHAR(100) NOT NULL,
    duration_ms BIGINT       NOT NULL CHECK (duration_ms >= 0),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX voice_recordings_user_created_at_idx
    ON voice_recordings (user_id, created_at DESC);
