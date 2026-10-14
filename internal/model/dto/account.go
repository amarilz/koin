package dto

import "time"

type CategoryType string

const (
	Income   CategoryType = "INCOME"
	Expense  CategoryType = "EXPENSE"
	Transfer CategoryType = "TRANSFER"
)

type CreateAccountDto struct {
	Name           string
	Currency       string
	InitialBalance int64
}

type AddTransactionDto struct {
	AccountName  string
	CategoryName string
	CategoryType CategoryType
	OccurredAt   time.Time
	Amount       int64
	Description  *string
}

type CreateCategoryDto struct {
	Name         string
	CategoryType CategoryType
	Description  string
}

type TransferBetweenAccountsDto struct {
	AccountFrom string
	AccountTo   string
	Amount      int64
	OccurredAt  time.Time
	Description *string
}
