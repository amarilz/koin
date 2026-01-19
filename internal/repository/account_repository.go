package repository

import (
	"context"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
)

type AccountRepository interface {
	GetAccount(ctx context.Context, user dbgen.User, accountName string) (dbgen.Account, error)
	CreateAccount(ctx context.Context, user dbgen.User, createAccountDto dto.CreateAccountDto) (dbgen.Account, error)
	AddTransaction(ctx context.Context, user dbgen.User, account dbgen.Account, category dbgen.Category, addExpenseDto dto.AddTransactionDto) (int64, error)
	GetAccounts(ctx context.Context, user dbgen.User) ([]dbgen.Account, error)
	GetAccountBalance(ctx context.Context, accountID int64) (int64, error)
	GetRecentTransactions(ctx context.Context, userID int64, limit int32) ([]dbgen.GetRecentTransactionEntriesByUserRow, error)
	TransferBetweenAccounts(ctx context.Context, user dbgen.User, fromAccount dbgen.Account, toAccount dbgen.Account, transfer dto.TransferBetweenAccountsDto) (int64, error)
}
