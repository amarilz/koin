package service

import (
	"context"
	"fmt"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
	repo "koin/internal/repository"
)

type AccountService struct {
	userRepo     repo.UserRepository
	accountRepo  repo.AccountRepository
	categoryRepo repo.CategoryRepository
}

func NewAccountService(
	userRepo repo.UserRepository,
	accountRepo repo.AccountRepository,
	categoryRepo repo.CategoryRepository,
) *AccountService {
	return &AccountService{
		userRepo:     userRepo,
		accountRepo:  accountRepo,
		categoryRepo: categoryRepo,
	}
}

func (accountService *AccountService) CreateAccount(ctx context.Context, createAccountDto dto.CreateAccountDto) (int64, error) {
	user, err2 := accountService.userRepo.GetUserByID(ctx, createAccountDto.UserID)
	if err2 != nil {
		return 0, err2
	}

	_, err := accountService.accountRepo.GetAccount(ctx, user, createAccountDto.Name)
	if err == nil {
		return 0, fmt.Errorf("account %q already exists", createAccountDto.Name)
	}

	account, err := accountService.accountRepo.CreateAccount(ctx, user, createAccountDto)
	if err != nil {
		return 0, err
	}

	return account.ID, nil
}

func (accountService *AccountService) AddTransaction(ctx context.Context, addExpenseDto dto.AddTransactionDto) (int64, error) {
	user, err2 := accountService.userRepo.GetUserByID(ctx, addExpenseDto.UserID)
	if err2 != nil {
		return 0, err2
	}
	account, err2 := accountService.accountRepo.GetAccount(ctx, user, addExpenseDto.AccountName)
	if err2 != nil {
		return 0, err2
	}

	// Tenta di ottenere la category, se non esiste la crea
	category, err2 := accountService.categoryRepo.GetCategory(ctx, user, addExpenseDto.CategoryName, addExpenseDto.CategoryType)
	if err2 != nil {
		category, err2 = accountService.categoryRepo.CreateCategory(ctx, user, addExpenseDto.CategoryName, addExpenseDto.CategoryType)
		if err2 != nil {
			return 0, err2
		}
	}

	transactionId, err2 := accountService.accountRepo.AddTransaction(ctx, user, account, category, addExpenseDto)
	if err2 != nil {
		return 0, err2
	}
	return transactionId, nil
}

func (accountService *AccountService) TransferBetweenAccounts(ctx context.Context, transfer dto.TransferBetweenAccountsDto) (int64, error) {
	user, err := accountService.userRepo.GetUserByID(ctx, transfer.UserID)
	if err != nil {
		return 0, err
	}

	fromAccount, err := accountService.accountRepo.GetAccount(ctx, user, transfer.AccountFrom)
	if err != nil {
		return 0, err
	}

	toAccount, err := accountService.accountRepo.GetAccount(ctx, user, transfer.AccountTo)
	if err != nil {
		return 0, err
	}

	return accountService.accountRepo.TransferBetweenAccounts(ctx, user, fromAccount, toAccount, transfer)
}

func (accountService *AccountService) GetCategories(ctx context.Context, user dbgen.User) ([]dbgen.Category, error) {
	return accountService.categoryRepo.GetCategories(ctx, user)
}

func (accountService *AccountService) GetAccounts(ctx context.Context, user dbgen.User) ([]dbgen.Account, error) {
	return accountService.accountRepo.GetAccounts(ctx, user)
}

func (accountService *AccountService) GetAccountBalance(ctx context.Context, accountID int64) (int64, error) {
	return accountService.accountRepo.GetAccountBalance(ctx, accountID)
}

func (accountService *AccountService) GetRecentTransactions(ctx context.Context, userID int64, limit int32) ([]dbgen.GetRecentTransactionEntriesByUserRow, error) {
	return accountService.accountRepo.GetRecentTransactions(ctx, userID, limit)
}

func (accountService *AccountService) CreateCategory(ctx context.Context, createCategoryDto dto.CreateCategoryDto) (dbgen.Category, error) {
	user, err := accountService.userRepo.GetUserByID(ctx, createCategoryDto.UserID)
	if err != nil {
		return dbgen.Category{}, err
	}

	// Verificare se la categoria esiste già
	_, err = accountService.categoryRepo.GetCategory(ctx, user, createCategoryDto.Name, createCategoryDto.CategoryType)
	if err == nil {
		return dbgen.Category{}, fmt.Errorf("category %q already exists", createCategoryDto.Name)
	}

	category, err := accountService.categoryRepo.CreateCategory(ctx, user, createCategoryDto.Name, createCategoryDto.CategoryType)
	if err != nil {
		return dbgen.Category{}, err
	}

	return category, nil
}
