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
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
	"koin/internal/repository"
	"koin/internal/service"

	"github.com/gin-gonic/gin"
)

type recordingRepositoryStub struct {
	audioData     []byte
	mimeType      string
	durationMS    int64
	transcription string
	extraction    json.RawMessage
	pending       []repository.ReviewableVoiceRecording
	deletedID     int64
}

func (repo *recordingRepositoryStub) CreateVoiceRecording(_ context.Context, audioData []byte, mimeType string, durationMS int64) (int64, error) {
	repo.audioData = audioData
	repo.mimeType = mimeType
	repo.durationMS = durationMS
	return 123, nil
}

func (repo *recordingRepositoryStub) SaveVoiceRecordingTranscription(_ context.Context, id int64, transcription string) error {
	repo.transcription = transcription
	repo.pending = append(repo.pending, repository.ReviewableVoiceRecording{
		ID: id, Transcription: transcription, MimeType: repo.mimeType, DurationMS: repo.durationMS,
	})
	return nil
}

func (repo *recordingRepositoryStub) SaveTransactionExtraction(_ context.Context, id int64, extraction json.RawMessage) error {
	repo.extraction = extraction
	for i := range repo.pending {
		if repo.pending[i].ID == id {
			repo.pending[i].Extraction = extraction
		}
	}
	return nil
}

func (repo *recordingRepositoryStub) ListReviewableVoiceRecordings(context.Context) ([]repository.ReviewableVoiceRecording, error) {
	return repo.pending, nil
}

func (repo *recordingRepositoryStub) ListVoiceRecordingsPendingTranscription(context.Context) ([]repository.PendingVoiceRecording, error) {
	if repo.transcription == "" && len(repo.audioData) > 0 {
		return []repository.PendingVoiceRecording{{ID: 123, MimeType: repo.mimeType}}, nil
	}
	return nil, nil
}

func (repo *recordingRepositoryStub) GetPendingVoiceRecordingAudio(context.Context, int64) ([]byte, string, error) {
	return repo.audioData, repo.mimeType, nil
}

func (repo *recordingRepositoryStub) ListPendingTransactionExtractions(context.Context) ([]repository.ReviewableVoiceRecording, error) {
	pending := make([]repository.ReviewableVoiceRecording, 0)
	for _, recording := range repo.pending {
		if len(recording.Extraction) == 0 {
			pending = append(pending, recording)
		}
	}
	return pending, nil
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

type extractionOptionsRepositoryStub struct{}

func (extractionOptionsRepositoryStub) GetAccount(context.Context, string) (dbgen.Account, error) {
	return dbgen.Account{}, nil
}

func (extractionOptionsRepositoryStub) CreateAccount(context.Context, dto.CreateAccountDto) (dbgen.Account, error) {
	return dbgen.Account{}, nil
}

func (extractionOptionsRepositoryStub) AddTransaction(context.Context, dbgen.Account, dbgen.Category, dto.AddTransactionDto) (int64, error) {
	return 0, nil
}

func (extractionOptionsRepositoryStub) GetAccounts(context.Context) ([]dbgen.Account, error) {
	return []dbgen.Account{{Name: "Contanti"}}, nil
}

func (extractionOptionsRepositoryStub) GetAccountBalance(context.Context, int64) (int64, error) {
	return 0, nil
}

func (extractionOptionsRepositoryStub) GetRecentTransactions(context.Context, int32) ([]dbgen.GetRecentTransactionEntriesRow, error) {
	return nil, nil
}

func (extractionOptionsRepositoryStub) TransferBetweenAccounts(context.Context, dbgen.Account, dbgen.Account, dto.TransferBetweenAccountsDto) (int64, error) {
	return 0, nil
}

func (extractionOptionsRepositoryStub) GetCategory(context.Context, string, dto.CategoryType) (dbgen.Category, error) {
	return dbgen.Category{}, nil
}

func (extractionOptionsRepositoryStub) CreateCategory(_ context.Context, name string, categoryType dto.CategoryType) (dbgen.Category, error) {
	return dbgen.Category{Name: name, Type: string(categoryType)}, nil
}

func (extractionOptionsRepositoryStub) GetCategories(context.Context) ([]dbgen.Category, error) {
	return []dbgen.Category{{Name: "Bar", Type: "EXPENSE"}}, nil
}

func TestCreateVoiceRecordingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	var transcriptionRequests atomic.Int32
	transcriptionUnavailable := false
	transcriptionServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		transcriptionRequests.Add(1)
		if transcriptionUnavailable {
			http.Error(w, "Whisper unavailable", http.StatusServiceUnavailable)
			return
		}
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
	extractionUnavailable := false
	extractionServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if extractionUnavailable {
			http.Error(w, "Ollama unavailable", http.StatusServiceUnavailable)
			return
		}
		var request struct {
			Messages []struct {
				Content string `json:"content"`
			} `json:"messages"`
		}
		if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
			t.Errorf("decode extraction request: %v", err)
			http.Error(w, "invalid request", http.StatusBadRequest)
			return
		}
		if len(request.Messages) != 2 || !strings.Contains(request.Messages[1].Content, "trascrizione immediata") {
			t.Errorf("expected transcript in extraction request, got %#v", request.Messages)
		}
		_, _ = io.WriteString(w, `{"done":true,"done_reason":"stop","message":{"role":"assistant","content":"{\"accountName\":\"Contanti\",\"accountTo\":null,\"categoryType\":\"EXPENSE\",\"categoryName\":\"Bar\",\"amount\":5,\"occurredAt\":null,\"description\":\"Caffè\"}"}}`)
	}))
	defer extractionServer.Close()
	transcriber, err := service.NewVoiceTranscriptionClient(transcriptionServer.URL)
	if err != nil {
		t.Fatal(err)
	}
	extractor, err := service.NewTransactionExtractionClient(extractionServer.URL, "test-model")
	if err != nil {
		t.Fatal(err)
	}
	optionsRepo := extractionOptionsRepositoryStub{}
	accounts := service.NewAccountService(optionsRepo, optionsRepo)
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
	requestBody := append([]byte(nil), body.Bytes()...)
	contentType := writer.FormDataContentType()

	request := httptest.NewRequest(http.MethodPost, "/forms/recordings", body)
	request.Header.Set("Content-Type", contentType)
	recorder := httptest.NewRecorder()
	ginContext, _ := gin.CreateTestContext(recorder)
	ginContext.Request = request

	processor := newVoiceRecordingProcessor(repo, transcriber, accounts, extractor, make(chan struct{}, 1))
	createVoiceRecordingHandler(repo, processor)(ginContext)

	if recorder.Code != http.StatusCreated {
		t.Fatalf("expected status %d, got %d: %s", http.StatusCreated, recorder.Code, recorder.Body.String())
	}
	if transcriptionRequests.Load() != 0 {
		t.Fatal("expected the request to return before transcription starts")
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
	if repo.transcription != "" {
		t.Errorf("expected transcription to remain pending after the response, got %q", repo.transcription)
	}

	processor.processPending(context.Background())
	if repo.transcription != "trascrizione immediata" {
		t.Errorf("expected background processing to save the transcription, got %q", repo.transcription)
	}
	if got, want := string(repo.extraction), `{"accountName":"Contanti","accountTo":null,"categoryType":"EXPENSE","categoryName":"Bar","amount":5,"occurredAt":null,"description":"Caffè"}`; got != want {
		t.Errorf("expected background extraction to be saved, got %q", got)
	}

	var response struct {
		ID int64 `json:"id"`
	}
	if err := json.Unmarshal(recorder.Body.Bytes(), &response); err != nil {
		t.Fatal(err)
	}
	if response.ID != 123 {
		t.Errorf("expected recording ID 123, got %d", response.ID)
	}

	extractionUnavailable = true
	pendingRepo := &recordingRepositoryStub{}
	pendingRequest := httptest.NewRequest(http.MethodPost, "/forms/recordings", bytes.NewReader(requestBody))
	pendingRequest.Header.Set("Content-Type", contentType)
	pendingRecorder := httptest.NewRecorder()
	pendingGinContext, _ := gin.CreateTestContext(pendingRecorder)
	pendingGinContext.Request = pendingRequest

	pendingProcessor := newVoiceRecordingProcessor(pendingRepo, transcriber, accounts, extractor, make(chan struct{}, 1))
	createVoiceRecordingHandler(pendingRepo, pendingProcessor)(pendingGinContext)

	if pendingRecorder.Code != http.StatusCreated {
		t.Fatalf("expected audio to be saved before Ollama processing with status %d, got %d: %s", http.StatusCreated, pendingRecorder.Code, pendingRecorder.Body.String())
	}
	pendingProcessor.processPending(context.Background())
	if pendingRepo.transcription != "trascrizione immediata" || len(pendingRepo.extraction) != 0 {
		t.Errorf("expected transcription to be saved and extraction to remain pending after Ollama failure, got transcription=%q extraction=%q", pendingRepo.transcription, pendingRepo.extraction)
	}

	transcriptionUnavailable = true
	unavailableRepo := &recordingRepositoryStub{}
	unavailableRequest := httptest.NewRequest(http.MethodPost, "/forms/recordings", bytes.NewReader(requestBody))
	unavailableRequest.Header.Set("Content-Type", contentType)
	unavailableRecorder := httptest.NewRecorder()
	unavailableGinContext, _ := gin.CreateTestContext(unavailableRecorder)
	unavailableGinContext.Request = unavailableRequest
	unavailableProcessor := newVoiceRecordingProcessor(unavailableRepo, transcriber, accounts, extractor, make(chan struct{}, 1))
	createVoiceRecordingHandler(unavailableRepo, unavailableProcessor)(unavailableGinContext)
	if unavailableRecorder.Code != http.StatusCreated {
		t.Fatalf("expected Whisper outage not to affect the saved recording response, got %d: %s", unavailableRecorder.Code, unavailableRecorder.Body.String())
	}
	unavailableProcessor.processPending(context.Background())
	if !bytes.Equal(unavailableRepo.audioData, []byte("recorded audio")) || unavailableRepo.transcription != "" {
		t.Errorf("expected audio to remain stored and pending when Whisper is unavailable, got audio=%q transcription=%q", unavailableRepo.audioData, unavailableRepo.transcription)
	}

	transcriptionUnavailable = false
	unavailableProcessor.processPending(context.Background())
	if unavailableRepo.transcription != "trascrizione immediata" {
		t.Errorf("expected pending audio to be transcribed after Whisper recovers, got %q", unavailableRepo.transcription)
	}
}

func TestExtractPendingVoiceRecordingsSerializesOllamaRequests(t *testing.T) {
	gin.SetMode(gin.TestMode)
	var activeRequests atomic.Int32
	var maxConcurrentRequests atomic.Int32
	extractionServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		active := activeRequests.Add(1)
		defer activeRequests.Add(-1)
		for maximum := maxConcurrentRequests.Load(); active > maximum; maximum = maxConcurrentRequests.Load() {
			if maxConcurrentRequests.CompareAndSwap(maximum, active) {
				break
			}
		}
		time.Sleep(25 * time.Millisecond)
		_, _ = io.WriteString(w, `{"done":true,"done_reason":"stop","message":{"role":"assistant","content":"{\"accountName\":\"Contanti\",\"accountTo\":null,\"categoryType\":\"EXPENSE\",\"categoryName\":\"Bar\",\"amount\":5,\"occurredAt\":\"2026-10-17\",\"description\":\"Caffè\"}"}}`)
	}))
	defer extractionServer.Close()
	extractor, err := service.NewTransactionExtractionClient(extractionServer.URL, "test-model")
	if err != nil {
		t.Fatal(err)
	}
	optionsRepo := extractionOptionsRepositoryStub{}
	accounts := service.NewAccountService(optionsRepo, optionsRepo)
	repo := &recordingRepositoryStub{
		pending: []repository.ReviewableVoiceRecording{
			{ID: 1, Transcription: "Prima trascrizione"},
			{ID: 2, Transcription: "Seconda trascrizione"},
		},
	}
	handler := newVoiceRecordingReviewHandler(repo, accounts, extractor, make(chan struct{}, 1))
	router := gin.New()
	router.POST("/extract-pending", handler.extractPending)

	responses := make([]*httptest.ResponseRecorder, 2)
	var requests sync.WaitGroup
	for i := range responses {
		responses[i] = httptest.NewRecorder()
		requests.Add(1)
		go func(response *httptest.ResponseRecorder) {
			defer requests.Done()
			router.ServeHTTP(response, httptest.NewRequest(http.MethodPost, "/extract-pending", nil))
		}(responses[i])
	}
	requests.Wait()

	if got := maxConcurrentRequests.Load(); got != 1 {
		t.Errorf("expected exactly one concurrent Ollama request, got %d", got)
	}
	if len(repo.extraction) == 0 {
		t.Error("expected extracted JSON to be saved")
	}
	for _, response := range responses {
		if response.Code != http.StatusOK {
			t.Errorf("expected status %d, got %d: %s", http.StatusOK, response.Code, response.Body.String())
		}
	}
	var firstResult struct {
		Processed int `json:"processed"`
		Succeeded int `json:"succeeded"`
	}
	var secondResult struct {
		Processed int `json:"processed"`
		Succeeded int `json:"succeeded"`
	}
	if err := json.Unmarshal(responses[0].Body.Bytes(), &firstResult); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(responses[1].Body.Bytes(), &secondResult); err != nil {
		t.Fatal(err)
	}
	if firstResult.Succeeded+secondResult.Succeeded != 2 {
		t.Errorf("expected two successful extractions across queued requests, got %+v and %+v", firstResult, secondResult)
	}
	if firstResult.Processed+secondResult.Processed != 2 {
		t.Errorf("expected two recordings to be processed exactly once, got %+v and %+v", firstResult, secondResult)
	}
}

func TestExtractPendingKeepsRecordingPendingWhenOllamaIsUnavailable(t *testing.T) {
	gin.SetMode(gin.TestMode)
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) {}))
	extractor, err := service.NewTransactionExtractionClient(server.URL, "test-model")
	if err != nil {
		t.Fatal(err)
	}
	server.Close()

	optionsRepo := extractionOptionsRepositoryStub{}
	accounts := service.NewAccountService(optionsRepo, optionsRepo)
	repo := &recordingRepositoryStub{
		pending: []repository.ReviewableVoiceRecording{{ID: 7, Transcription: "Spesa al bar"}},
	}
	handler := newVoiceRecordingReviewHandler(repo, accounts, extractor, make(chan struct{}, 1))
	response := httptest.NewRecorder()
	context, _ := gin.CreateTestContext(response)
	context.Request = httptest.NewRequest(http.MethodPost, "/extract-pending", nil)

	handler.extractPending(context)

	if response.Code != http.StatusOK {
		t.Fatalf("expected status %d, got %d: %s", http.StatusOK, response.Code, response.Body.String())
	}
	var result struct {
		Failed int `json:"failed"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &result); err != nil {
		t.Fatal(err)
	}
	if result.Failed != 1 {
		t.Errorf("expected one failed extraction, got %d", result.Failed)
	}
	if len(repo.pending[0].Extraction) != 0 {
		t.Error("expected failed extraction to remain pending for retry")
	}
}

func TestDeletePendingVoiceRecordingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	repo := &recordingRepositoryStub{}
	recorder := httptest.NewRecorder()
	context, _ := gin.CreateTestContext(recorder)
	context.Request = httptest.NewRequest(http.MethodDelete, "/forms/recordings/42", nil)
	context.Params = gin.Params{{Key: "id", Value: "42"}}

	newVoiceRecordingReviewHandler(repo, nil, nil, make(chan struct{}, 1)).delete(context)

	if context.Writer.Status() != http.StatusNoContent {
		t.Fatalf("expected status %d, got %d: %s", http.StatusNoContent, context.Writer.Status(), recorder.Body.String())
	}
	if repo.deletedID != 42 {
		t.Errorf("expected recording ID 42 to be deleted, got %d", repo.deletedID)
	}
}

func TestParseOccurredAtDefaultsToTodayWhenMissing(t *testing.T) {
	occurredAt, err := parseOccurredAt("")
	if err != nil {
		t.Fatalf("parse missing date: %v", err)
	}
	if got, want := occurredAt.Format("2006-01-02"), time.Now().Format("2006-01-02"); got != want {
		t.Errorf("expected today's date %q, got %q", want, got)
	}
}

func TestParseOccurredAtPreservesProvidedDate(t *testing.T) {
	occurredAt, err := parseOccurredAt("2025-03-14")
	if err != nil {
		t.Fatalf("parse provided date: %v", err)
	}
	if got, want := occurredAt.Format("2006-01-02"), "2025-03-14"; got != want {
		t.Errorf("expected date %q, got %q", want, got)
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

	for _, path := range []string{"/forms", "/forms/recordings", "/forms/accounts", "/forms/categories"} {
		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, path, nil))
		if response.Code != http.StatusOK {
			t.Errorf("GET %s: expected status %d, got %d", path, http.StatusOK, response.Code)
		}
	}

	for _, path := range []string{"/login", "/signup", "/logout", "/forms/transactions"} {
		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, path, nil))
		if response.Code != http.StatusNotFound {
			t.Errorf("GET %s: expected status %d, got %d", path, http.StatusNotFound, response.Code)
		}
	}

	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodPost, "/forms/recordings/extract-pending", nil))
	if response.Code != http.StatusOK {
		t.Errorf("POST /forms/recordings/extract-pending: expected status %d, got %d", http.StatusOK, response.Code)
	}

	response = httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodPost, "/api/v1/expenses", nil))
	if response.Code != http.StatusNotFound {
		t.Errorf("POST /api/v1/expenses: expected status %d, got %d", http.StatusNotFound, response.Code)
	}
}
