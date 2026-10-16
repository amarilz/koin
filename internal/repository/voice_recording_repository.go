package repository

import "context"

type VoiceRecordingRepository interface {
	CreateVoiceRecording(ctx context.Context, audioData []byte, mimeType string, durationMS int64) (int64, error)
}

type PendingVoiceRecording struct {
	ID        int64
	AudioData []byte
	MimeType  string
}

type VoiceRecordingTranscriptionRepository interface {
	GetNextUntranscribedVoiceRecording(ctx context.Context, afterID int64) (*PendingVoiceRecording, error)
	SaveVoiceRecordingTranscription(ctx context.Context, id int64, transcription string) error
}
