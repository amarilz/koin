package repository

import (
	"context"
	"time"
)

type VoiceRecordingRepository interface {
	CreateVoiceRecording(ctx context.Context, audioData []byte, mimeType string, durationMS int64, transcription string) (int64, error)
}

type ReviewableVoiceRecording struct {
	ID            int64     `json:"id"`
	Transcription string    `json:"transcription"`
	MimeType      string    `json:"mimeType"`
	DurationMS    int64     `json:"durationMs"`
	CreatedAt     time.Time `json:"createdAt"`
}

type VoiceRecordingReviewRepository interface {
	VoiceRecordingRepository
	ListReviewableVoiceRecordings(ctx context.Context) ([]ReviewableVoiceRecording, error)
	GetReviewableVoiceRecording(ctx context.Context, id int64) (ReviewableVoiceRecording, error)
	GetVoiceRecordingAudio(ctx context.Context, id int64) ([]byte, string, error)
	DeletePendingVoiceRecording(ctx context.Context, id int64) error
}
