package postgres

import (
	"context"
	"database/sql"
	"encoding/json"
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

func (repo *VoiceRecordingRepository) SaveVoiceRecordingTranscription(ctx context.Context, id int64, transcription string) error {
	result, err := repo.db.ExecContext(ctx, `
		UPDATE voice_recordings
		SET transcription = $2
		WHERE id = $1 AND transcription IS NULL
	`, id, transcription)
	if err != nil {
		return fmt.Errorf("save transcription for voice recording %d: %w", id, err)
	}
	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check transcription update for voice recording %d: %w", id, err)
	}
	if rowsAffected == 0 {
		return fmt.Errorf("%w: voice recording %d is missing or already transcribed", sql.ErrNoRows, id)
	}
	return nil
}

func (repo *VoiceRecordingRepository) SaveTransactionExtraction(ctx context.Context, id int64, extraction json.RawMessage) error {
	var extractionValue any
	if len(extraction) > 0 {
		extractionValue = string(extraction)
	}
	result, err := repo.db.ExecContext(ctx, `
		UPDATE voice_recordings
		SET extraction = $2
		WHERE id = $1 AND transcription IS NOT NULL
	`, id, extractionValue)
	if err != nil {
		return fmt.Errorf("save extraction for voice recording %d: %w", id, err)
	}
	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check extraction update for voice recording %d: %w", id, err)
	}
	if rowsAffected == 0 {
		return fmt.Errorf("%w: voice recording %d is missing or has no transcription", sql.ErrNoRows, id)
	}
	return nil
}

func (repo *VoiceRecordingRepository) ListVoiceRecordingsPendingTranscription(ctx context.Context) ([]repository.PendingVoiceRecording, error) {
	rows, err := repo.db.QueryContext(ctx, `
		SELECT id, mime_type
		FROM voice_recordings
		WHERE transcription IS NULL
		ORDER BY created_at, id
	`)
	if err != nil {
		return nil, fmt.Errorf("list voice recordings pending transcription: %w", err)
	}
	defer rows.Close()

	recordings := make([]repository.PendingVoiceRecording, 0)
	for rows.Next() {
		var recording repository.PendingVoiceRecording
		if err := rows.Scan(&recording.ID, &recording.MimeType); err != nil {
			return nil, fmt.Errorf("scan voice recording pending transcription: %w", err)
		}
		recordings = append(recordings, recording)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate voice recordings pending transcription: %w", err)
	}
	return recordings, nil
}

func (repo *VoiceRecordingRepository) GetPendingVoiceRecordingAudio(ctx context.Context, id int64) ([]byte, string, error) {
	var audioData []byte
	var mimeType string
	err := repo.db.QueryRowContext(ctx, `
		SELECT audio_data, mime_type
		FROM voice_recordings
		WHERE id = $1 AND transcription IS NULL
	`, id).Scan(&audioData, &mimeType)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, "", fmt.Errorf("%w: voice recording %d", sql.ErrNoRows, id)
		}
		return nil, "", fmt.Errorf("get pending audio for voice recording %d: %w", id, err)
	}
	return audioData, mimeType, nil
}

func (repo *VoiceRecordingRepository) ListReviewableVoiceRecordings(ctx context.Context) ([]repository.ReviewableVoiceRecording, error) {
	rows, err := repo.db.QueryContext(ctx, `
		SELECT id, transcription, mime_type, duration_ms, created_at, extraction
		FROM voice_recordings
		WHERE transcription IS NOT NULL
		ORDER BY created_at DESC, id DESC
	`)
	if err != nil {
		return nil, fmt.Errorf("list reviewable voice recordings: %w", err)
	}
	defer rows.Close()

	recordings := make([]repository.ReviewableVoiceRecording, 0)
	for rows.Next() {
		var recording repository.ReviewableVoiceRecording
		var extraction []byte
		if err := rows.Scan(&recording.ID, &recording.Transcription, &recording.MimeType, &recording.DurationMS, &recording.CreatedAt, &extraction); err != nil {
			return nil, fmt.Errorf("scan reviewable voice recording: %w", err)
		}
		recording.Extraction = json.RawMessage(extraction)
		recordings = append(recordings, recording)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate reviewable voice recordings: %w", err)
	}
	return recordings, nil
}

func (repo *VoiceRecordingRepository) ListPendingTransactionExtractions(ctx context.Context) ([]repository.ReviewableVoiceRecording, error) {
	rows, err := repo.db.QueryContext(ctx, `
		SELECT id, transcription, mime_type, duration_ms, created_at
		FROM voice_recordings
		WHERE transcription IS NOT NULL AND extraction IS NULL
		ORDER BY created_at, id
	`)
	if err != nil {
		return nil, fmt.Errorf("list voice recordings pending extraction: %w", err)
	}
	defer rows.Close()

	recordings := make([]repository.ReviewableVoiceRecording, 0)
	for rows.Next() {
		var recording repository.ReviewableVoiceRecording
		if err := rows.Scan(&recording.ID, &recording.Transcription, &recording.MimeType, &recording.DurationMS, &recording.CreatedAt); err != nil {
			return nil, fmt.Errorf("scan voice recording pending extraction: %w", err)
		}
		recordings = append(recordings, recording)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate voice recordings pending extraction: %w", err)
	}
	return recordings, nil
}

func (repo *VoiceRecordingRepository) GetVoiceRecordingAudio(ctx context.Context, id int64) ([]byte, string, error) {
	var audioData []byte
	var mimeType string
	err := repo.db.QueryRowContext(ctx, `
		SELECT audio_data, mime_type
		FROM voice_recordings
		WHERE id = $1 AND transcription IS NOT NULL
	`, id).Scan(&audioData, &mimeType)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, "", fmt.Errorf("%w: voice recording %d", sql.ErrNoRows, id)
		}
		return nil, "", fmt.Errorf("get audio for voice recording %d: %w", id, err)
	}
	return audioData, mimeType, nil
}

func (repo *VoiceRecordingRepository) GetReviewableVoiceRecording(ctx context.Context, id int64) (repository.ReviewableVoiceRecording, error) {
	var recording repository.ReviewableVoiceRecording
	var extraction []byte
	err := repo.db.QueryRowContext(ctx, `
		SELECT id, transcription, mime_type, duration_ms, created_at, extraction
		FROM voice_recordings
		WHERE id = $1 AND transcription IS NOT NULL
	`, id).Scan(&recording.ID, &recording.Transcription, &recording.MimeType, &recording.DurationMS, &recording.CreatedAt, &extraction)
	if err != nil {
		return repository.ReviewableVoiceRecording{}, fmt.Errorf("get reviewable voice recording %d: %w", id, err)
	}
	recording.Extraction = json.RawMessage(extraction)
	return recording, nil
}

func (repo *VoiceRecordingRepository) DeletePendingVoiceRecording(ctx context.Context, id int64) error {
	result, err := repo.db.ExecContext(ctx, `
		DELETE FROM voice_recordings
		WHERE id = $1 AND transcription IS NOT NULL
	`, id)
	if err != nil {
		return fmt.Errorf("delete pending voice recording %d: %w", id, err)
	}
	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("check deleted voice recording %d: %w", id, err)
	}
	if rowsAffected == 0 {
		return fmt.Errorf("%w: voice recording %d is missing or has no transcription", sql.ErrNoRows, id)
	}
	return nil
}
