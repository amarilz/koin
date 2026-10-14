package repository

import "context"

type VoiceRecordingRepository interface {
	CreateVoiceRecording(ctx context.Context, audioData []byte, mimeType string, durationMS int64) (int64, error)
}
