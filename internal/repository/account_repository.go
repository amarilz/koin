package repository

import (
	"context"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
)

type AccountRepository interface {
	GetAccount(ctx context.Context, accountName string) (dbgen.Account, error)
	CreateAccount(ctx context.Context, createAccountDto dto.CreateAccountDto) (dbgen.Account, error)
	AddTransaction(ctx context.Context, account dbgen.Account, category dbgen.Category, addExpenseDto dto.AddTransactionDto) (int64, error)
	GetAccounts(ctx context.Context) ([]dbgen.Account, error)
	GetAccountBalance(ctx context.Context, accountID int64) (int64, error)
	GetRecentTransactions(ctx context.Context, limit int32) ([]dbgen.GetRecentTransactionEntriesRow, error)
	TransferBetweenAccounts(ctx context.Context, fromAccount dbgen.Account, toAccount dbgen.Account, transfer dto.TransferBetweenAccountsDto) (int64, error)
}
