package http

import (
	"bytes"
	"context"
	"encoding/json"
	"html/template"
	"io"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"net/textproto"
	"testing"
	"time"

	"koin/internal/repository"
	"koin/internal/service"

	"github.com/gin-gonic/gin"
)

type recordingRepositoryStub struct {
	audioData     []byte
	mimeType      string
	durationMS    int64
	transcription string
	deletedID     int64
}

func (repo *recordingRepositoryStub) CreateVoiceRecording(_ context.Context, audioData []byte, mimeType string, durationMS int64, transcription string) (int64, error) {
	repo.audioData = audioData
	repo.mimeType = mimeType
	repo.durationMS = durationMS
	repo.transcription = transcription
	return 123, nil
}

func (repo *recordingRepositoryStub) ListReviewableVoiceRecordings(context.Context) ([]repository.ReviewableVoiceRecording, error) {
	return nil, nil
}

func (repo *recordingRepositoryStub) GetReviewableVoiceRecording(_ context.Context, id int64) (repository.ReviewableVoiceRecording, error) {
	return repository.ReviewableVoiceRecording{ID: id, Transcription: "test", CreatedAt: time.Now()}, nil
}

func (repo *recordingRepositoryStub) GetVoiceRecordingAudio(context.Context, int64) ([]byte, string, error) {
	return repo.audioData, repo.mimeType, nil
}

func (repo *recordingRepositoryStub) DeletePendingVoiceRecording(_ context.Context, id int64) error {
	repo.deletedID = id
	return nil
}

func TestCreateVoiceRecordingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	transcriptionServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected transcription POST, got %s", r.Method)
		}
		if err := r.ParseMultipartForm(1 << 20); err != nil {
			t.Errorf("parse transcription request: %v", err)
			http.Error(w, "invalid request", http.StatusBadRequest)
			return
		}
		file, _, err := r.FormFile("file")
		if err != nil {
			t.Errorf("read transcription audio: %v", err)
			http.Error(w, "missing audio", http.StatusBadRequest)
			return
		}
		defer file.Close()
		audio, err := io.ReadAll(file)
		if err != nil {
			t.Errorf("read transcription audio: %v", err)
		}
		if string(audio) != "recorded audio" {
			t.Errorf("unexpected transcription input %q", audio)
		}
		_, _ = io.WriteString(w, `{"text":"trascrizione immediata"}`)
	}))
	defer transcriptionServer.Close()
	transcriber, err := service.NewVoiceTranscriptionClient(transcriptionServer.URL)
	if err != nil {
		t.Fatal(err)
	}
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

	createVoiceRecordingHandler(repo, transcriber)(context)

	if recorder.Code != http.StatusCreated {
		t.Fatalf("expected status %d, got %d: %s", http.StatusCreated, recorder.Code, recorder.Body.String())
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
	if repo.transcription != "trascrizione immediata" {
		t.Errorf("expected transcription to be saved immediately, got %q", repo.transcription)
	}

	var response map[string]int64
	if err := json.Unmarshal(recorder.Body.Bytes(), &response); err != nil {
		t.Fatal(err)
	}
	if response["id"] != 123 {
		t.Errorf("expected recording ID 123, got %d", response["id"])
	}
}

func TestDeletePendingVoiceRecordingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	repo := &recordingRepositoryStub{}
	recorder := httptest.NewRecorder()
	context, _ := gin.CreateTestContext(recorder)
	context.Request = httptest.NewRequest(http.MethodDelete, "/forms/recordings/42", nil)
	context.Params = gin.Params{{Key: "id", Value: "42"}}

	newVoiceRecordingReviewHandler(repo, nil, nil).delete(context)

	if context.Writer.Status() != http.StatusNoContent {
		t.Fatalf("expected status %d, got %d: %s", http.StatusNoContent, context.Writer.Status(), recorder.Body.String())
	}
	if repo.deletedID != 42 {
		t.Errorf("expected recording ID 42 to be deleted, got %d", repo.deletedID)
	}
}

func TestTemplatesParse(t *testing.T) {
	if _, err := template.ParseGlob("templates/*.html"); err != nil {
		t.Fatalf("parse HTML templates: %v", err)
	}
}

func TestPagesArePublicAndAuthenticationRoutesAreRemoved(t *testing.T) {
	t.Chdir("../../../")
	router := NewRouter(RouterDeps{
		Controller: NewController(nil),
		Recordings: &recordingRepositoryStub{},
	})

	for _, path := range []string{"/forms", "/forms/accounts", "/forms/categories", "/forms/transactions", "/forms/recordings"} {
		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, path, nil))
		if response.Code != http.StatusOK {
			t.Errorf("GET %s: expected status %d, got %d", path, http.StatusOK, response.Code)
		}
	}

	for _, path := range []string{"/login", "/signup", "/logout"} {
		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, path, nil))
		if response.Code != http.StatusNotFound {
			t.Errorf("GET %s: expected status %d, got %d", path, http.StatusNotFound, response.Code)
		}
	}
}
