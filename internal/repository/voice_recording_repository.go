package repository

import (
	"context"
	"encoding/json"
	"time"
)

type VoiceRecordingRepository interface {
	CreateVoiceRecording(ctx context.Context, audioData []byte, mimeType string, durationMS int64) (int64, error)
	SaveVoiceRecordingTranscription(ctx context.Context, id int64, transcription string) error
	SaveTransactionExtraction(ctx context.Context, id int64, extraction json.RawMessage) error
}

type PendingVoiceRecording struct {
	ID       int64
	MimeType string
}

type ReviewableVoiceRecording struct {
	ID            int64           `json:"id"`
	Transcription string          `json:"transcription"`
	MimeType      string          `json:"mimeType"`
	DurationMS    int64           `json:"durationMs"`
	CreatedAt     time.Time       `json:"createdAt"`
	Extraction    json.RawMessage `json:"extraction,omitempty"`
}

type VoiceRecordingReviewRepository interface {
	VoiceRecordingRepository
	ListVoiceRecordingsPendingTranscription(ctx context.Context) ([]PendingVoiceRecording, error)
	GetPendingVoiceRecordingAudio(ctx context.Context, id int64) ([]byte, string, error)
	ListReviewableVoiceRecordings(ctx context.Context) ([]ReviewableVoiceRecording, error)
	ListPendingTransactionExtractions(ctx context.Context) ([]ReviewableVoiceRecording, error)
	GetReviewableVoiceRecording(ctx context.Context, id int64) (ReviewableVoiceRecording, error)
	GetVoiceRecordingAudio(ctx context.Context, id int64) ([]byte, string, error)
	DeletePendingVoiceRecording(ctx context.Context, id int64) error
}
