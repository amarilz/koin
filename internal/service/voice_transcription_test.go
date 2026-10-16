package service

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"koin/internal/repository"
)

func TestVoiceTranscriptionClientSendsAudioAndReadsTranscript(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if err := r.ParseMultipartForm(1 << 20); err != nil {
			t.Errorf("parse multipart request: %v", err)
			http.Error(w, "invalid multipart request", http.StatusBadRequest)
			return
		}
		file, header, err := r.FormFile("file")
		if err != nil {
			t.Errorf("read uploaded file: %v", err)
			http.Error(w, "missing file", http.StatusBadRequest)
			return
		}
		defer file.Close()
		if header.Filename != "recording.webm" {
			t.Errorf("expected filename recording.webm, got %q", header.Filename)
		}
		if header.Header.Get("Content-Type") != "audio/webm" {
			t.Errorf("expected audio/webm, got %q", header.Header.Get("Content-Type"))
		}
		audio, err := io.ReadAll(file)
		if err != nil {
			t.Errorf("read uploaded audio: %v", err)
		}
		if string(audio) != "audio bytes" {
			t.Errorf("unexpected uploaded audio %q", audio)
		}
		_ = json.NewEncoder(w).Encode(map[string]string{"text": "  ciao mondo  ", "language": "Italian"})
	}))
	defer server.Close()

	client, err := NewVoiceTranscriptionClient(server.URL + "/transcribe")
	if err != nil {
		t.Fatal(err)
	}
	transcription, err := client.Transcribe(context.Background(), []byte("audio bytes"), "audio/webm")
	if err != nil {
		t.Fatal(err)
	}
	if transcription != "ciao mondo" {
		t.Errorf("expected trimmed transcript, got %q", transcription)
	}
}

func TestVoiceTranscriptionServiceSavesPendingRecordings(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = io.WriteString(w, `{"text":"trascritto"}`)
	}))
	defer server.Close()

	client, err := NewVoiceTranscriptionClient(server.URL)
	if err != nil {
		t.Fatal(err)
	}
	recordings := &transcriptionRepositoryStub{
		recordings: []repository.PendingVoiceRecording{
			{ID: 1, AudioData: []byte("first"), MimeType: "audio/webm"},
			{ID: 2, AudioData: []byte("second"), MimeType: "audio/mp4"},
		},
	}
	service := NewVoiceTranscriptionService(recordings, client)
	service.transcribePending(context.Background())

	if len(recordings.transcriptions) != 2 {
		t.Fatalf("expected 2 saved transcriptions, got %d", len(recordings.transcriptions))
	}
	for _, transcript := range recordings.transcriptions {
		if transcript != "trascritto" {
			t.Errorf("unexpected transcription %q", transcript)
		}
	}
}

func TestAudioFileExtension(t *testing.T) {
	for mimeType, expected := range map[string]string{
		"audio/webm;codecs=opus": ".webm",
		"audio/mp4":              ".mp4",
		"audio/mpeg":             ".mp3",
	} {
		if got := audioFileExtension(mimeType); got != expected {
			t.Errorf("audioFileExtension(%q) = %q, want %q", mimeType, got, expected)
		}
	}
}

type transcriptionRepositoryStub struct {
	recordings     []repository.PendingVoiceRecording
	transcriptions map[int64]string
}

func (repo *transcriptionRepositoryStub) GetNextUntranscribedVoiceRecording(_ context.Context, afterID int64) (*repository.PendingVoiceRecording, error) {
	for i := range repo.recordings {
		if repo.recordings[i].ID > afterID {
			return &repo.recordings[i], nil
		}
	}
	return nil, nil
}

func (repo *transcriptionRepositoryStub) SaveVoiceRecordingTranscription(_ context.Context, id int64, transcription string) error {
	if repo.transcriptions == nil {
		repo.transcriptions = make(map[int64]string)
	}
	repo.transcriptions[id] = strings.TrimSpace(transcription)
	return nil
}
