package http

import (
	"errors"
	"io"
	"log"
	"mime/multipart"
	"net/http"
	"strconv"
	"strings"

	"koin/internal/repository"

	"github.com/gin-gonic/gin"
)

const maxVoiceRecordingSize = 25 << 20

func createVoiceRecordingHandler(recordings repository.VoiceRecordingRepository) gin.HandlerFunc {
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
		c.JSON(http.StatusCreated, gin.H{"id": id})
	}
}

func closeVoiceRecordingFile(file multipart.File) {
	if err := file.Close(); err != nil {
		log.Printf("close uploaded voice recording: %v", err)
	}
}
