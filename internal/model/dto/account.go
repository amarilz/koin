package dto

import "time"

type CategoryType string

const (
	Income   CategoryType = "INCOME"
	Expense  CategoryType = "EXPENSE"
	Transfer CategoryType = "TRANSFER"
)

type CreateUserDto struct {
	Email    string
	Password string
}

type CreateAccountDto struct {
	UserID         int64
	Name           string
	Currency       string
	InitialBalance int64
}

type AddTransactionDto struct {
	UserID       int64
	AccountName  string
	CategoryName string
	CategoryType CategoryType
	OccurredAt   time.Time
	Amount       int64
	Description  *string
}

type CreateCategoryDto struct {
	UserID       int64
	Name         string
	CategoryType CategoryType
	Description  string
}

type TransferBetweenAccountsDto struct {
	UserID      int64
	AccountFrom string
	AccountTo   string
	Amount      int64
	OccurredAt  time.Time
	Description *string
}
