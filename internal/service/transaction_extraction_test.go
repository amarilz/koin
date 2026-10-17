package service

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestTransactionExtractionClientUsesNativeOllamaJSONChat(t *testing.T) {
	var logs strings.Builder
	previousLogOutput := log.Writer()
	log.SetOutput(&logs)
	defer log.SetOutput(previousLogOutput)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/chat" {
			t.Errorf("unexpected endpoint %q", r.URL.Path)
		}
		var request struct {
			Model   string `json:"model"`
			Stream  bool   `json:"stream"`
			Think   bool   `json:"think"`
			Format  string `json:"format"`
			Options struct {
				Temperature float64 `json:"temperature"`
				NumPredict  int     `json:"num_predict"`
			} `json:"options"`
			Messages []struct {
				Role    string `json:"role"`
				Content string `json:"content"`
			} `json:"messages"`
		}
		if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
			t.Errorf("decode request: %v", err)
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}
		if request.Model != "test-model" {
			t.Errorf("expected test-model, got %q", request.Model)
		}
		if request.Format != "json" {
			t.Errorf("expected json format, got %q", request.Format)
		}
		if request.Stream || request.Think || request.Options.NumPredict != transactionExtractionMaxTokens || request.Options.Temperature != 0 {
			t.Errorf("unexpected generation settings: stream=%t think=%t num_predict=%d temperature=%g", request.Stream, request.Think, request.Options.NumPredict, request.Options.Temperature)
		}
		if len(request.Messages) != 2 || request.Messages[0].Role != "system" || request.Messages[1].Role != "user" {
			t.Errorf("unexpected chat messages: %#v", request.Messages)
		}
		if request.Messages[0].Content != transactionExtractionPrompt {
			t.Errorf("system prompt mismatch")
		}
		var input struct {
			Accounts      []string              `json:"accounts"`
			Categories    []TransactionCategory `json:"categories"`
			Transcription string                `json:"transcription"`
		}
		if err := json.Unmarshal([]byte(request.Messages[1].Content), &input); err != nil {
			t.Errorf("decode user data: %v", err)
		}
		if input.Transcription != "Ho speso 5 euro al bar" || len(input.Accounts) != 1 || input.Accounts[0] != "Contanti" {
			t.Errorf("unexpected user input: %#v", input)
		}
		if len(input.Categories) != 1 || input.Categories[0] != (TransactionCategory{Name: "Bar", Type: "EXPENSE"}) {
			t.Errorf("unexpected category list: %#v", input.Categories)
		}
		_, _ = w.Write([]byte(`{"done":true,"done_reason":"stop","message":{"role":"assistant","content":"{\"accountName\":\"Contanti\",\"accountTo\":null,\"categoryType\":\"EXPENSE\",\"categoryName\":\"Bar\",\"amount\":5,\"occurredAt\":\"2026-10-17\",\"description\":\"Caffè\"}"}}`))
	}))
	defer server.Close()

	client, err := NewTransactionExtractionClient(server.URL, "test-model")
	if err != nil {
		t.Fatal(err)
	}
	result, err := client.Extract(context.Background(), "Ho speso 5 euro al bar", []string{"Contanti"}, []TransactionCategory{{Name: "Bar", Type: "EXPENSE"}})
	if err != nil {
		t.Fatal(err)
	}
	if result.AccountName == nil || *result.AccountName != "Contanti" || result.Amount == nil || *result.Amount != 5 {
		t.Fatalf("unexpected extraction: %#v", result)
	}
	if strings.Contains(logs.String(), "Ollama request:") ||
		strings.Contains(logs.String(), "Ho speso 5 euro al bar") ||
		!strings.Contains(logs.String(), "Ollama response (status=200):") ||
		!strings.Contains(logs.String(), "Caffè") {
		t.Errorf("expected only the Ollama response in logs, got %q", logs.String())
	}
}

func TestNewTransactionExtractionClientRequiresHTTPURLAndModel(t *testing.T) {
	for _, tc := range []struct {
		url   string
		model string
	}{
		{url: "file:///tmp/ollama", model: "model"},
		{url: "http://localhost:11434", model: ""},
	} {
		if _, err := NewTransactionExtractionClient(tc.url, tc.model); err == nil {
			t.Errorf("expected error for URL %q and model %q", tc.url, tc.model)
		}
	}
}

func TestTransactionExtractionClientReportsTruncatedThinkingWithoutReturningIt(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"done":true,"done_reason":"length","message":{"role":"assistant","content":"","thinking":"private reasoning"}}`))
	}))
	defer server.Close()

	client, err := NewTransactionExtractionClient(server.URL, "test-model")
	if err != nil {
		t.Fatal(err)
	}
	_, err = client.Extract(context.Background(), "transcript", nil, nil)
	if err == nil {
		t.Fatal("expected an error when Ollama returns no final content")
	}
	if !strings.Contains(err.Error(), `done_reason="length"`) || !strings.Contains(err.Error(), "thinking_chars=") {
		t.Errorf("expected useful completion diagnostics, got %v", err)
	}
	if strings.Contains(err.Error(), "private reasoning") {
		t.Errorf("error unexpectedly exposes the model reasoning: %v", err)
	}
}
