package http

import (
	"bytes"
	"log"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestRequestLoggerSkipsGetRequests(t *testing.T) {
	gin.SetMode(gin.TestMode)
	var logs bytes.Buffer
	previousOutput := log.Writer()
	log.SetOutput(&logs)
	defer log.SetOutput(previousOutput)

	router := gin.New()
	router.Use(RequestLogger())
	router.Any("/", func(c *gin.Context) {
		c.Status(http.StatusOK)
	})

	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/", nil))
	if logs.Len() != 0 {
		t.Errorf("expected GET request not to be logged, got %q", logs.String())
	}

	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodPost, "/", nil))
	if !strings.Contains(logs.String(), "method=POST") {
		t.Errorf("expected non-GET request to be logged, got %q", logs.String())
	}
}
