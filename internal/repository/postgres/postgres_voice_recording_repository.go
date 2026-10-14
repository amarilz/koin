package postgres

import (
	"context"
	"database/sql"
	"fmt"
)

type VoiceRecordingRepository struct {
	db *sql.DB
}

func NewVoiceRecordingRepository(db *sql.DB) *VoiceRecordingRepository {
	return &VoiceRecordingRepository{db: db}
}

func (repo *VoiceRecordingRepository) CreateVoiceRecording(ctx context.Context, audioData []byte, mimeType string, durationMS int64) (int64, error) {
	var id int64
	err := repo.db.QueryRowContext(ctx, `
		INSERT INTO voice_recordings (audio_data, mime_type, duration_ms)
		VALUES ($1, $2, $3)
		RETURNING id
	`, audioData, mimeType, durationMS).Scan(&id)
	if err != nil {
		return 0, fmt.Errorf("create voice recording: %w", err)
	}
	return id, nil
}
