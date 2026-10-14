package service

import (
	"context"
	"fmt"

	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
	repo "koin/internal/repository"
)

type AccountService struct {
	accountRepo  repo.AccountRepository
	categoryRepo repo.CategoryRepository
}

func NewAccountService(accountRepo repo.AccountRepository, categoryRepo repo.CategoryRepository) *AccountService {
	return &AccountService{
		accountRepo:  accountRepo,
		categoryRepo: categoryRepo,
	}
}

func (accountService *AccountService) CreateAccount(ctx context.Context, createAccountDto dto.CreateAccountDto) (int64, error) {
	_, err := accountService.accountRepo.GetAccount(ctx, createAccountDto.Name)
	if err == nil {
		return 0, fmt.Errorf("account %q already exists", createAccountDto.Name)
	}

	account, err := accountService.accountRepo.CreateAccount(ctx, createAccountDto)
	if err != nil {
		return 0, err
	}
	return account.ID, nil
}

func (accountService *AccountService) AddTransaction(ctx context.Context, addExpenseDto dto.AddTransactionDto) (int64, error) {
	account, err := accountService.accountRepo.GetAccount(ctx, addExpenseDto.AccountName)
	if err != nil {
		return 0, err
	}

	category, err := accountService.categoryRepo.GetCategory(ctx, addExpenseDto.CategoryName, addExpenseDto.CategoryType)
	if err != nil {
		category, err = accountService.categoryRepo.CreateCategory(ctx, addExpenseDto.CategoryName, addExpenseDto.CategoryType)
		if err != nil {
			return 0, err
		}
	}

	return accountService.accountRepo.AddTransaction(ctx, account, category, addExpenseDto)
}

func (accountService *AccountService) TransferBetweenAccounts(ctx context.Context, transfer dto.TransferBetweenAccountsDto) (int64, error) {
	fromAccount, err := accountService.accountRepo.GetAccount(ctx, transfer.AccountFrom)
	if err != nil {
		return 0, err
	}

	toAccount, err := accountService.accountRepo.GetAccount(ctx, transfer.AccountTo)
	if err != nil {
		return 0, err
	}

	return accountService.accountRepo.TransferBetweenAccounts(ctx, fromAccount, toAccount, transfer)
}

func (accountService *AccountService) GetCategories(ctx context.Context) ([]dbgen.Category, error) {
	return accountService.categoryRepo.GetCategories(ctx)
}

func (accountService *AccountService) GetAccounts(ctx context.Context) ([]dbgen.Account, error) {
	return accountService.accountRepo.GetAccounts(ctx)
}

func (accountService *AccountService) GetAccountBalance(ctx context.Context, accountID int64) (int64, error) {
	return accountService.accountRepo.GetAccountBalance(ctx, accountID)
}

func (accountService *AccountService) GetRecentTransactions(ctx context.Context, limit int32) ([]dbgen.GetRecentTransactionEntriesRow, error) {
	return accountService.accountRepo.GetRecentTransactions(ctx, limit)
}

func (accountService *AccountService) CreateCategory(ctx context.Context, createCategoryDto dto.CreateCategoryDto) (dbgen.Category, error) {
	_, err := accountService.categoryRepo.GetCategory(ctx, createCategoryDto.Name, createCategoryDto.CategoryType)
	if err == nil {
		return dbgen.Category{}, fmt.Errorf("category %q already exists", createCategoryDto.Name)
	}

	return accountService.categoryRepo.CreateCategory(ctx, createCategoryDto.Name, createCategoryDto.CategoryType)
}
