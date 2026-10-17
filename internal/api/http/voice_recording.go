package http

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"mime/multipart"
	"net/http"
	"strconv"
	"strings"
	"time"

	"koin/internal/model/dto"
	"koin/internal/repository"
	"koin/internal/service"

	"github.com/gin-gonic/gin"
)

const maxVoiceRecordingSize = 25 << 20

func createVoiceRecordingHandler(recordings repository.VoiceRecordingRepository, processor *voiceRecordingProcessor) gin.HandlerFunc {
	return func(c *gin.Context) {
		c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, maxVoiceRecordingSize+(1<<20))
		file, header, err := c.Request.FormFile("audio")
		if err != nil {
			var maxBytesError *http.MaxBytesError
			if errors.As(err, &maxBytesError) {
				c.JSON(http.StatusRequestEntityTooLarge, gin.H{"error": "registrazione troppo grande (massimo 25 MB)"})
				return
			}
			c.JSON(http.StatusBadRequest, gin.H{"error": "file audio mancante o non valido"})
			return
		}
		defer closeVoiceRecordingFile(file)
		if header.Size > maxVoiceRecordingSize {
			c.JSON(http.StatusRequestEntityTooLarge, gin.H{"error": "registrazione troppo grande (massimo 25 MB)"})
			return
		}

		mimeType := header.Header.Get("Content-Type")
		if !strings.HasPrefix(mimeType, "audio/") {
			c.JSON(http.StatusBadRequest, gin.H{"error": "il file caricato non è un audio valido"})
			return
		}

		durationMS, err := strconv.ParseInt(c.PostForm("durationMs"), 10, 64)
		if err != nil || durationMS < 0 {
			c.JSON(http.StatusBadRequest, gin.H{"error": "durata della registrazione non valida"})
			return
		}

		audioData, err := io.ReadAll(file)
		if err != nil {
			log.Printf("read uploaded voice recording: %v", err)
			c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile leggere la registrazione"})
			return
		}
		if len(audioData) == 0 {
			c.JSON(http.StatusBadRequest, gin.H{"error": "la registrazione è vuota"})
			return
		}

		id, err := recordings.CreateVoiceRecording(c.Request.Context(), audioData, mimeType, durationMS)
		if err != nil {
			log.Printf("save voice recording: %v", err)
			c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile salvare la registrazione"})
			return
		}

		log.Printf("voice recording audio saved | id=%d | audio_size=%d", id, len(audioData))
		c.JSON(http.StatusCreated, gin.H{"id": id})
		processor.Notify()
	}
}

type voiceRecordingProcessor struct {
	recordings      repository.VoiceRecordingRepository
	pending         repository.VoiceRecordingReviewRepository
	transcriber     *service.VoiceTranscriptionClient
	accounts        *service.AccountService
	extractor       *service.TransactionExtractionClient
	extractionQueue chan struct{}
	wake            chan struct{}
}

func newVoiceRecordingProcessor(
	recordings repository.VoiceRecordingReviewRepository,
	transcriber *service.VoiceTranscriptionClient,
	accounts *service.AccountService,
	extractor *service.TransactionExtractionClient,
	extractionQueue chan struct{},
) *voiceRecordingProcessor {
	return &voiceRecordingProcessor{
		recordings:      recordings,
		pending:         recordings,
		transcriber:     transcriber,
		accounts:        accounts,
		extractor:       extractor,
		extractionQueue: extractionQueue,
		wake:            make(chan struct{}, 1),
	}
}

func (processor *voiceRecordingProcessor) Notify() {
	select {
	case processor.wake <- struct{}{}:
	default:
	}
}

func (processor *voiceRecordingProcessor) Run(ctx context.Context) {
	ticker := time.NewTicker(time.Minute)
	defer ticker.Stop()
	for {
		processor.processPending(ctx)
		select {
		case <-ctx.Done():
			return
		case <-processor.wake:
		case <-ticker.C:
		}
	}
}

func (processor *voiceRecordingProcessor) processPending(ctx context.Context) {
	pending, err := processor.pending.ListVoiceRecordingsPendingTranscription(ctx)
	if err != nil {
		log.Printf("list voice recordings pending transcription: %v", err)
		return
	}
	for _, recording := range pending {
		if ctx.Err() != nil {
			return
		}
		audioData, mimeType, err := processor.pending.GetPendingVoiceRecordingAudio(ctx, recording.ID)
		if err != nil {
			log.Printf("load audio for voice recording %d: %v", recording.ID, err)
			continue
		}
		if processor.transcriber == nil {
			log.Printf("transcribe voice recording %d: transcription service is not configured", recording.ID)
			continue
		}
		transcription, err := processor.transcriber.Transcribe(ctx, audioData, mimeType)
		if err != nil {
			log.Printf("transcribe voice recording %d: %v", recording.ID, err)
			continue
		}
		if err := processor.recordings.SaveVoiceRecordingTranscription(ctx, recording.ID, transcription); err != nil {
			log.Printf("save transcription for voice recording %d: %v", recording.ID, err)
			continue
		}
		if err := extractAndSaveTransaction(ctx, processor.recordings, processor.accounts, processor.extractor, processor.extractionQueue, recording.ID, transcription); err != nil {
			log.Printf("extract transaction from voice recording %d: %v", recording.ID, err)
		}
	}
}

func transactionExtractionOptions(ctx context.Context, accounts *service.AccountService) ([]string, []service.TransactionCategory, error) {
	if accounts == nil {
		return nil, nil, errors.New("transaction extraction accounts are not configured")
	}
	accountRecords, err := accounts.GetAccounts(ctx)
	if err != nil {
		return nil, nil, fmt.Errorf("load accounts for extraction: %w", err)
	}
	categoryRecords, err := accounts.GetCategories(ctx)
	if err != nil {
		return nil, nil, fmt.Errorf("load categories for extraction: %w", err)
	}
	accountNames := make([]string, 0, len(accountRecords))
	for _, account := range accountRecords {
		accountNames = append(accountNames, account.Name)
	}
	categoryOptions := make([]service.TransactionCategory, 0, len(categoryRecords))
	for _, category := range categoryRecords {
		categoryOptions = append(categoryOptions, service.TransactionCategory{Name: category.Name, Type: category.Type})
	}
	return accountNames, categoryOptions, nil
}

func extractAndSaveTransaction(
	ctx context.Context,
	recordings repository.VoiceRecordingRepository,
	accounts *service.AccountService,
	extractor *service.TransactionExtractionClient,
	extractionQueue chan struct{},
	id int64,
	transcription string,
) error {
	if extractor == nil {
		return errors.New("transaction extraction is not configured")
	}
	select {
	case extractionQueue <- struct{}{}:
		defer func() { <-extractionQueue }()
	case <-ctx.Done():
		return ctx.Err()
	}
	return extractAndSaveTransactionLocked(ctx, recordings, accounts, extractor, id, transcription)
}

func extractAndSaveTransactionLocked(
	ctx context.Context,
	recordings repository.VoiceRecordingRepository,
	accounts *service.AccountService,
	extractor *service.TransactionExtractionClient,
	id int64,
	transcription string,
) error {
	if extractor == nil {
		return errors.New("transaction extraction is not configured")
	}
	accountNames, categories, err := transactionExtractionOptions(ctx, accounts)
	if err != nil {
		return err
	}
	extraction, err := extractor.Extract(ctx, transcription, accountNames, categories)
	if err != nil {
		return fmt.Errorf("extract transaction: %w", err)
	}
	extractionJSON, err := json.Marshal(extraction)
	if err != nil {
		return fmt.Errorf("encode extraction: %w", err)
	}
	if err := recordings.SaveTransactionExtraction(ctx, id, extractionJSON); err != nil {
		return fmt.Errorf("save extraction: %w", err)
	}
	return nil
}

func closeVoiceRecordingFile(file multipart.File) {
	if err := file.Close(); err != nil {
		log.Printf("close uploaded voice recording: %v", err)
	}
}

type voiceRecordingReviewHandler struct {
	recordings      repository.VoiceRecordingReviewRepository
	accounts        *service.AccountService
	extractor       *service.TransactionExtractionClient
	extractionQueue chan struct{}
}

func newVoiceRecordingReviewHandler(recordings repository.VoiceRecordingReviewRepository, accounts *service.AccountService, extractor *service.TransactionExtractionClient, extractionQueue chan struct{}) *voiceRecordingReviewHandler {
	return &voiceRecordingReviewHandler{
		recordings:      recordings,
		accounts:        accounts,
		extractor:       extractor,
		extractionQueue: extractionQueue,
	}
}

func (handler *voiceRecordingReviewHandler) list(c *gin.Context) {
	recordings, err := handler.recordings.ListReviewableVoiceRecordings(c.Request.Context())
	if err != nil {
		log.Printf("list reviewable voice recordings: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile caricare le registrazioni"})
		return
	}
	c.JSON(http.StatusOK, recordings)
}

func (handler *voiceRecordingReviewHandler) extractPending(c *gin.Context) {
	select {
	case handler.extractionQueue <- struct{}{}:
		defer func() { <-handler.extractionQueue }()
	case <-c.Request.Context().Done():
		return
	}

	pending, err := handler.recordings.ListPendingTransactionExtractions(c.Request.Context())
	if err != nil {
		log.Printf("list voice recordings pending extraction: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile caricare le registrazioni in attesa"})
		return
	}
	if len(pending) == 0 {
		c.JSON(http.StatusOK, gin.H{"processed": 0, "succeeded": 0, "failed": 0})
		return
	}
	if handler.extractor == nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "servizio Ollama non configurato"})
		return
	}
	succeeded := 0
	failed := 0
	for _, recording := range pending {
		if c.Request.Context().Err() != nil {
			return
		}
		if err := extractAndSaveTransactionLocked(
			c.Request.Context(), handler.recordings, handler.accounts, handler.extractor, recording.ID, recording.Transcription,
		); err != nil {
			log.Printf("extract transaction from voice recording %d: %v", recording.ID, err)
			failed++
			continue
		}
		succeeded++
	}

	c.JSON(http.StatusOK, gin.H{"processed": len(pending), "succeeded": succeeded, "failed": failed})
}

func (handler *voiceRecordingReviewHandler) audio(c *gin.Context) {
	id, ok := voiceRecordingID(c)
	if !ok {
		return
	}
	audio, mimeType, err := handler.recordings.GetVoiceRecordingAudio(c.Request.Context(), id)
	if err != nil {
		if errors.Is(err, context.Canceled) {
			return
		}
		if errors.Is(err, sql.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "registrazione non trovata"})
			return
		}
		log.Printf("get audio for voice recording %d: %v", id, err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile leggere la registrazione"})
		return
	}
	c.Data(http.StatusOK, mimeType, audio)
}

type confirmVoiceTransactionRequest struct {
	AccountName  string   `json:"accountName"`
	AccountTo    string   `json:"accountTo"`
	CategoryType string   `json:"categoryType"`
	CategoryName string   `json:"categoryName"`
	Amount       *float64 `json:"amount"`
	OccurredAt   string   `json:"occurredAt"`
	Description  string   `json:"description"`
}

func (handler *voiceRecordingReviewHandler) confirm(c *gin.Context) {
	id, ok := voiceRecordingID(c)
	if !ok {
		return
	}
	if _, err := handler.recordings.GetReviewableVoiceRecording(c.Request.Context(), id); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "registrazione non trovata o già confermata"})
			return
		}
		log.Printf("get voice recording %d before confirmation: %v", id, err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile caricare la registrazione"})
		return
	}
	var request confirmVoiceTransactionRequest
	if err := c.ShouldBindJSON(&request); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "dati della transazione non validi"})
		return
	}
	if strings.TrimSpace(request.AccountName) == "" || request.Amount == nil || *request.Amount <= 0 || *request.Amount > 1e12 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "account e importo positivo sono obbligatori"})
		return
	}
	occurredAt, err := parseOccurredAt(request.OccurredAt)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "la data deve essere nel formato YYYY-MM-DD"})
		return
	}
	amount := int64(*request.Amount*100 + 0.5)
	if amount == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "l'importo deve essere almeno un centesimo"})
		return
	}
	var transactionID int64
	switch dto.CategoryType(request.CategoryType) {
	case dto.Income, dto.Expense:
		if strings.TrimSpace(request.CategoryName) == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "la categoria è obbligatoria"})
			return
		}
		if dto.CategoryType(request.CategoryType) == dto.Expense {
			amount = -amount
		}
		transactionID, err = handler.accounts.AddTransaction(c.Request.Context(), dto.AddTransactionDto{
			AccountName:  strings.TrimSpace(request.AccountName),
			CategoryName: strings.TrimSpace(request.CategoryName),
			CategoryType: dto.CategoryType(request.CategoryType),
			OccurredAt:   occurredAt,
			Amount:       amount,
			Description:  optionalDescription(request.Description),
		})
	case dto.Transfer:
		if strings.TrimSpace(request.AccountTo) == "" || strings.TrimSpace(request.AccountName) == strings.TrimSpace(request.AccountTo) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "gli account di origine e destinazione devono essere distinti"})
			return
		}
		description := strings.TrimSpace(request.Description)
		if description == "" {
			description = "Trasferimento tra account"
		}
		transactionID, err = handler.accounts.TransferBetweenAccounts(c.Request.Context(), dto.TransferBetweenAccountsDto{
			AccountFrom: strings.TrimSpace(request.AccountName),
			AccountTo:   strings.TrimSpace(request.AccountTo),
			Amount:      amount,
			OccurredAt:  occurredAt,
			Description: &description,
		})
	default:
		c.JSON(http.StatusBadRequest, gin.H{"error": "tipo operazione non valido"})
		return
	}
	if err != nil {
		log.Printf("create transaction from voice recording %d: %v", id, err)
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}
	if err := handler.recordings.DeletePendingVoiceRecording(c.Request.Context(), id); err != nil {
		log.Printf("delete voice recording %d after transaction %d: %v", id, transactionID, err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "transazione creata ma impossibile eliminare la registrazione"})
		return
	}
	c.JSON(http.StatusCreated, gin.H{"transactionId": transactionID})
}

func (handler *voiceRecordingReviewHandler) delete(c *gin.Context) {
	id, ok := voiceRecordingID(c)
	if !ok {
		return
	}
	if err := handler.recordings.DeletePendingVoiceRecording(c.Request.Context(), id); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			c.JSON(http.StatusNotFound, gin.H{"error": "trascrizione non trovata o già confermata"})
			return
		}
		log.Printf("delete pending voice recording %d: %v", id, err)
		c.JSON(http.StatusInternalServerError, gin.H{"error": "impossibile eliminare la trascrizione"})
		return
	}
	c.Status(http.StatusNoContent)
}

func voiceRecordingID(c *gin.Context) (int64, bool) {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id <= 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "ID registrazione non valido"})
		return 0, false
	}
	return id, true
}

func optionalDescription(value string) *string {
	value = strings.TrimSpace(value)
	if value == "" {
		return nil
	}
	return &value
}

func parseOccurredAt(value string) (time.Time, error) {
	if strings.TrimSpace(value) == "" {
		value = time.Now().Format("2006-01-02")
	}
	return time.Parse("2006-01-02", value)
}
