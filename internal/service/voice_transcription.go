package service

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"mime"
	"mime/multipart"
	"net/http"
	"net/textproto"
	"net/url"
	"path"
	"strings"
	"time"

	"koin/internal/repository"
)

const defaultTranscriptionInterval = time.Minute

type VoiceTranscriptionClient struct {
	endpoint   string
	httpClient *http.Client
}

func NewVoiceTranscriptionClient(endpoint string) (*VoiceTranscriptionClient, error) {
	parsed, err := url.ParseRequestURI(endpoint)
	if err != nil || (parsed.Scheme != "http" && parsed.Scheme != "https") || parsed.Host == "" {
		return nil, fmt.Errorf("invalid transcription service URL %q", endpoint)
	}

	return &VoiceTranscriptionClient{
		endpoint: endpoint,
		httpClient: &http.Client{
			Timeout: 10 * time.Minute,
		},
	}, nil
}

func (client *VoiceTranscriptionClient) Transcribe(ctx context.Context, audioData []byte, mimeType string) (string, error) {
	body := new(bytes.Buffer)
	writer := multipart.NewWriter(body)
	header := make(textproto.MIMEHeader)
	header.Set("Content-Disposition", fmt.Sprintf(`form-data; name="file"; filename="recording%s"`, audioFileExtension(mimeType)))
	header.Set("Content-Type", mimeType)
	part, err := writer.CreatePart(header)
	if err != nil {
		return "", fmt.Errorf("create transcription request file: %w", err)
	}
	if _, err := part.Write(audioData); err != nil {
		return "", fmt.Errorf("write transcription request file: %w", err)
	}
	if err := writer.Close(); err != nil {
		return "", fmt.Errorf("finish transcription request: %w", err)
	}

	request, err := http.NewRequestWithContext(ctx, http.MethodPost, client.endpoint, body)
	if err != nil {
		return "", fmt.Errorf("create transcription request: %w", err)
	}
	request.Header.Set("Content-Type", writer.FormDataContentType())

	response, err := client.httpClient.Do(request)
	if err != nil {
		return "", fmt.Errorf("call transcription service: %w", err)
	}
	defer response.Body.Close()

	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		responseBody, readErr := io.ReadAll(io.LimitReader(response.Body, 4096))
		if readErr != nil {
			return "", fmt.Errorf("transcription service returned status %d (read response: %w)", response.StatusCode, readErr)
		}
		return "", fmt.Errorf("transcription service returned status %d: %s", response.StatusCode, strings.TrimSpace(string(responseBody)))
	}

	var result struct {
		Text string `json:"text"`
	}
	if err := json.NewDecoder(io.LimitReader(response.Body, 1<<20)).Decode(&result); err != nil {
		return "", fmt.Errorf("decode transcription response: %w", err)
	}
	return strings.TrimSpace(result.Text), nil
}

func audioFileExtension(mimeType string) string {
	switch strings.ToLower(strings.TrimSpace(strings.SplitN(mimeType, ";", 2)[0])) {
	case "audio/webm":
		return ".webm"
	case "audio/mp4":
		return ".mp4"
	case "audio/mpeg":
		return ".mp3"
	case "audio/wav", "audio/x-wav":
		return ".wav"
	case "audio/ogg":
		return ".ogg"
	}

	extensions, err := mime.ExtensionsByType(mimeType)
	if err == nil && len(extensions) > 0 {
		return path.Ext(extensions[0])
	}
	return ".audio"
}

type VoiceTranscriptionService struct {
	recordings repository.VoiceRecordingTranscriptionRepository
	client     *VoiceTranscriptionClient
	interval   time.Duration
}

func NewVoiceTranscriptionService(recordings repository.VoiceRecordingTranscriptionRepository, client *VoiceTranscriptionClient) *VoiceTranscriptionService {
	return &VoiceTranscriptionService{
		recordings: recordings,
		client:     client,
		interval:   defaultTranscriptionInterval,
	}
}

func (service *VoiceTranscriptionService) Run(ctx context.Context) {
	ticker := time.NewTicker(service.interval)
	defer ticker.Stop()

	log.Printf("voice transcription scheduler started | interval=%s", service.interval)
	for {
		service.transcribePending(ctx)
		select {
		case <-ctx.Done():
			log.Println("voice transcription scheduler stopped")
			return
		case <-ticker.C:
		}
	}
}

func (service *VoiceTranscriptionService) transcribePending(ctx context.Context) {
	cycleStarted := time.Now()
	log.Println("voice transcription scheduler checking for pending recordings")
	var afterID int64
	processed := 0
	for {
		if err := ctx.Err(); err != nil {
			log.Printf("voice transcription scheduler cycle interrupted | processed=%d | error=%v", processed, err)
			return
		}
		recording, err := service.recordings.GetNextUntranscribedVoiceRecording(ctx, afterID)
		if err != nil {
			log.Printf("voice transcription scheduler failed to load pending recording | processed=%d | error=%v", processed, err)
			return
		}
		if recording == nil {
			if processed != 0 {
				log.Printf("voice transcription scheduler cycle completed | processed=%d | duration=%s", processed, time.Since(cycleStarted).Round(time.Millisecond))
			}
			return
		}
		afterID = recording.ID

		transcriptionStarted := time.Now()
		transcription, err := service.client.Transcribe(ctx, recording.AudioData, recording.MimeType)
		if err != nil {
			log.Printf("transcribe voice recording %d: %v", recording.ID, err)
			continue
		}
		if err := service.recordings.SaveVoiceRecordingTranscription(ctx, recording.ID, transcription); err != nil {
			log.Printf("save transcription for voice recording %d: %v", recording.ID, err)
			continue
		}
		processed++
		log.Printf(
			"voice recording transcribed | id=%d | audio_size=%d | duration=%s",
			recording.ID,
			len(recording.AudioData),
			time.Since(transcriptionStarted).Round(time.Millisecond),
		)
	}
}
