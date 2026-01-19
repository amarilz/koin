package http

import (
	//"errors"
	"net/http"

	apperr "koin/internal/errors"

	"github.com/gin-gonic/gin"
)

type APIError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func writeError(c *gin.Context, status int, code, message string) {
	c.JSON(status, gin.H{"error": APIError{Code: code, Message: message}})
}

func mapStoreError(c *gin.Context, err error) bool {
	switch err {
	case nil:
		return false
	case apperr.ErrNotFound:
		writeError(c, http.StatusNotFound, "NOT_FOUND", "resource not found")
		return true
	case apperr.ErrConflict:
		writeError(c, http.StatusConflict, "CONFLICT", "resource conflict")
		return true
	case apperr.ErrInvalidData:
		writeError(c, http.StatusBadRequest, "BAD_REQUEST", "invalid data")
		return true
	default:
		writeError(c, http.StatusInternalServerError, "INTERNAL", "internal server error")
		return true
	}
}
