package postgres

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"koin/internal/model/dto"

	dbgen "koin/internal/db/generated"
	apierr "koin/internal/errors"
)

type AccountRepository struct {
	queries *dbgen.Queries
	db      *sql.DB // utile se vuoi transazioni
}

func NewAccountRepository(db *sql.DB) *AccountRepository {
	return &AccountRepository{
		db:      db,
		queries: dbgen.New(db),
	}
}

func (repo *AccountRepository) CreateAccount(ctx context.Context, user dbgen.User, createAccountDto dto.CreateAccountDto) (dbgen.Account, error) {
	account, err := repo.queries.CreateAccount(ctx, dbgen.CreateAccountParams{
		UserID:         user.ID,
		Name:           createAccountDto.Name,
		Currency:       createAccountDto.Currency,
		InitialBalance: createAccountDto.InitialBalance,
	})
	if err != nil {
		return dbgen.Account{}, fmt.Errorf("create account %q: %w", account.Name, err)
	}
	return account, nil
}

func (repo *AccountRepository) GetAccount(ctx context.Context, user dbgen.User, accountName string) (dbgen.Account, error) {
	account, err := repo.queries.GetAccount(ctx, dbgen.GetAccountParams{
		UserID: user.ID,
		Name:   accountName,
	})
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return dbgen.Account{}, fmt.Errorf("%w: %s", apierr.ErrAccountNotFound, accountName)
		}
		return dbgen.Account{}, fmt.Errorf("get account by user and name %q-%q: %w", user, accountName, err)
	}
	return account, nil
}

func (repo *AccountRepository) AddTransaction(ctx context.Context, user dbgen.User, account dbgen.Account, category dbgen.Category, addExpenseDto dto.AddTransactionDto) (int64, error) {
	transactionId, err := repo.queries.AddTransaction(ctx, dbgen.AddTransactionParams{
		UserID:     user.ID,
		OccurredAt: addExpenseDto.OccurredAt,
	})
	if err != nil {
		return 0, err
	}

	err = repo.queries.AddTransactionEntry(ctx, dbgen.AddTransactionEntryParams{
		TransactionID: transactionId,
		AccountID:     account.ID,
		CategoryID: sql.NullInt64{
			Int64: category.ID,
			Valid: true,
		},
		Amount: addExpenseDto.Amount,
		Description: sql.NullString{
			String: func() string {
				if addExpenseDto.Description == nil {
					return ""
				}
				return *addExpenseDto.Description
			}(),
			Valid: addExpenseDto.Description != nil,
		},
	})
	if err != nil {
		return 0, err
	}
	return transactionId, nil
}

func (repo *AccountRepository) GetAccounts(ctx context.Context, user dbgen.User) ([]dbgen.Account, error) {
	accounts, err := repo.queries.GetAccountsByUser(ctx, user.ID)
	if err != nil {
		return nil, fmt.Errorf("get accounts by user: %w", err)
	}
	return accounts, nil
}

func (repo *AccountRepository) GetAccountBalance(ctx context.Context, accountID int64) (int64, error) {
	balance, err := repo.queries.GetAccountBalance(ctx, accountID)
	if err != nil {
		return 0, err
	}
	return balance, nil
}

func (repo *AccountRepository) GetRecentTransactions(ctx context.Context, userID int64, limit int32) ([]dbgen.GetRecentTransactionEntriesByUserRow, error) {
	entries, err := repo.queries.GetRecentTransactionEntriesByUser(ctx, dbgen.GetRecentTransactionEntriesByUserParams{
		UserID: userID,
		Limit:  limit,
	})
	if err != nil {
		return nil, fmt.Errorf("get recent transactions: %w", err)
	}
	return entries, nil
}

func (repo *AccountRepository) TransferBetweenAccounts(ctx context.Context, user dbgen.User, fromAccount dbgen.Account, toAccount dbgen.Account, transfer dto.TransferBetweenAccountsDto) (int64, error) {
	if fromAccount.ID == toAccount.ID {
		return 0, fmt.Errorf("accounts must be different")
	}
	if transfer.Amount <= 0 {
		return 0, fmt.Errorf("amount must be greater than zero")
	}

	tx, err := repo.db.BeginTx(ctx, nil)
	if err != nil {
		return 0, err
	}

	queries := repo.queries.WithTx(tx)

	balance, err := queries.GetAccountBalance(ctx, fromAccount.ID)
	if err != nil {
		_ = tx.Rollback()
		return 0, err
	}
	currentBalance := fromAccount.InitialBalance + balance
	if currentBalance < transfer.Amount {
		_ = tx.Rollback()
		return 0, fmt.Errorf("%w", apierr.ErrInsufficientBalance)
	}

	transactionID, err := queries.AddTransaction(ctx, dbgen.AddTransactionParams{
		UserID:     user.ID,
		OccurredAt: transfer.OccurredAt,
	})
	if err != nil {
		_ = tx.Rollback()
		return 0, err
	}

	var descStr string
	if transfer.Description == nil || *transfer.Description == "" {
		descStr = "Trasferimento tra account"
	} else {
		descStr = *transfer.Description
	}

	err = queries.AddTransactionEntry(ctx, dbgen.AddTransactionEntryParams{
		TransactionID: transactionID,
		AccountID:     fromAccount.ID,
		CategoryID:    sql.NullInt64{},
		Amount:        -transfer.Amount,
		Description: sql.NullString{
			String: descStr,
			Valid:  true,
		},
	})
	if err != nil {
		_ = tx.Rollback()
		return 0, err
	}

	err = queries.AddTransactionEntry(ctx, dbgen.AddTransactionEntryParams{
		TransactionID: transactionID,
		AccountID:     toAccount.ID,
		CategoryID:    sql.NullInt64{},
		Amount:        transfer.Amount,
		Description: sql.NullString{
			String: descStr,
			Valid:  true,
		},
	})
	if err != nil {
		_ = tx.Rollback()
		return 0, err
	}

	if err := tx.Commit(); err != nil {
		return 0, err
	}

	return transactionID, nil
}
