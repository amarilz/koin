package postgres

import (
	"context"
	"database/sql"
	"fmt"

	"koin/internal/repository"
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

func (repo *VoiceRecordingRepository) GetNextUntranscribedVoiceRecording(ctx context.Context, afterID int64) (*repository.PendingVoiceRecording, error) {
	recording := &repository.PendingVoiceRecording{}
	err := repo.db.QueryRowContext(ctx, `
		SELECT id, audio_data, mime_type
		FROM voice_recordings
		WHERE transcription IS NULL AND id > $1
		ORDER BY id
		LIMIT 1
	`, afterID).Scan(&recording.ID, &recording.AudioData, &recording.MimeType)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, fmt.Errorf("get next untranscribed voice recording: %w", err)
	}
	return recording, nil
}

func (repo *VoiceRecordingRepository) SaveVoiceRecordingTranscription(ctx context.Context, id int64, transcription string) error {
	result, err := repo.db.ExecContext(ctx, `
		UPDATE voice_recordings
		SET transcription = $2
		WHERE id = $1 AND transcription IS NULL
	`, id, transcription)
	if err != nil {
		return fmt.Errorf("save voice recording transcription: %w", err)
	}
	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check saved voice recording transcription: %w", err)
	}
	if rowsAffected == 0 {
		return fmt.Errorf("save voice recording transcription: recording %d was not pending", id)
	}
	return nil
}
