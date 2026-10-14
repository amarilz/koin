package http

import (
	"bytes"
	"context"
	"encoding/json"
	"html/template"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"net/textproto"
	"testing"

	"github.com/gin-gonic/gin"
)

type recordingRepositoryStub struct {
	userID     int64
	audioData  []byte
	mimeType   string
	durationMS int64
}

func (repo *recordingRepositoryStub) CreateVoiceRecording(_ context.Context, userID int64, audioData []byte, mimeType string, durationMS int64) (int64, error) {
	repo.userID = userID
	repo.audioData = audioData
	repo.mimeType = mimeType
	repo.durationMS = durationMS
	return 123, nil
}

func TestCreateVoiceRecordingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	repo := &recordingRepositoryStub{}
	body := new(bytes.Buffer)
	writer := multipart.NewWriter(body)
	file, err := writer.CreatePart(textproto.MIMEHeader{
		"Content-Disposition": {`form-data; name="audio"; filename="recording.webm"`},
		"Content-Type":        {"audio/webm"},
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.Write([]byte("recorded audio")); err != nil {
		t.Fatal(err)
	}
	if err := writer.WriteField("durationMs", "1500"); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}

	request := httptest.NewRequest(http.MethodPost, "/forms/recordings", body)
	request.Header.Set("Content-Type", writer.FormDataContentType())
	recorder := httptest.NewRecorder()
	context, _ := gin.CreateTestContext(recorder)
	context.Request = request
	context.Set("userID", int64(42))

	createVoiceRecordingHandler(repo)(context)

	if recorder.Code != http.StatusCreated {
		t.Fatalf("expected status %d, got %d: %s", http.StatusCreated, recorder.Code, recorder.Body.String())
	}
	if repo.userID != 42 {
		t.Errorf("expected user ID 42, got %d", repo.userID)
	}
	if !bytes.Equal(repo.audioData, []byte("recorded audio")) {
		t.Errorf("unexpected audio data %q", repo.audioData)
	}
	if repo.mimeType != "audio/webm" {
		t.Errorf("expected multipart file content type audio/webm, got %q", repo.mimeType)
	}
	if repo.durationMS != 1500 {
		t.Errorf("expected duration 1500ms, got %d", repo.durationMS)
	}

	var response map[string]int64
	if err := json.Unmarshal(recorder.Body.Bytes(), &response); err != nil {
		t.Fatal(err)
	}
	if response["id"] != 123 {
		t.Errorf("expected recording ID 123, got %d", response["id"])
	}
}

func TestCreateVoiceRecordingHandlerRequiresAuthenticatedUser(t *testing.T) {
	gin.SetMode(gin.TestMode)
	recorder := httptest.NewRecorder()
	context, _ := gin.CreateTestContext(recorder)
	context.Request = httptest.NewRequest(http.MethodPost, "/forms/recordings", nil)

	createVoiceRecordingHandler(&recordingRepositoryStub{})(context)

	if recorder.Code != http.StatusUnauthorized {
		t.Fatalf("expected status %d, got %d", http.StatusUnauthorized, recorder.Code)
	}
}

func TestTemplatesParse(t *testing.T) {
	if _, err := template.ParseGlob("templates/*.html"); err != nil {
		t.Fatalf("parse HTML templates: %v", err)
	}
}
