package errors

import "errors"

var (
	ErrNotFound            = errors.New("not found")
	ErrUserNotFound        = errors.New("user not found")
	ErrAccountNotFound     = errors.New("account not found")
	ErrConflict            = errors.New("conflict")
	ErrInvalidData         = errors.New("invalid data")
	ErrInsufficientBalance = errors.New("insufficient balance")
)
