package service

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
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
