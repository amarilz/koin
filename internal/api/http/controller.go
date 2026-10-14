package http

import (
	"context"
	"errors"
	"net/http"

	apigen "koin/internal/api/generated"
	errs "koin/internal/errors"
	"koin/internal/model/dto"
	"koin/internal/service"

	"github.com/gin-gonic/gin"
	openapi_types "github.com/oapi-codegen/runtime/types"
)

type Controller struct {
	accountService *service.AccountService
}

func NewController(accountService *service.AccountService) apigen.ServerInterface {
	controller := &Controller{
		accountService: accountService,
	}
	return apigen.NewStrictHandler(controller, nil)
}

func Health(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"status": "ok"})
}

func (ctrl *Controller) CreateAccount(ctx context.Context, request apigen.CreateAccountRequestObject) (apigen.CreateAccountResponseObject, error) {
	// Validare che il body sia presente
	if request.Body == nil {
		return apigen.CreateAccount400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_REQUEST",
				Message: "body richiesto",
			},
		}, nil
	}

	body := request.Body

	// Validare i dati richiesti
	if len(body.Name) == 0 || len(body.Currency) == 0 {
		return apigen.CreateAccount400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "name, currency e initialBalance sono obbligatori",
			},
		}, nil
	}

	// Mappare il body della request al DTO interno
	createAccountDto := ToCreateAccountDto(body)

	// Eseguire la transazione
	accountId, err := ctrl.accountService.CreateAccount(ctx, createAccountDto)
	if err != nil {
		// Gestire i diversi tipi di errore
		if errors.Is(err, errs.ErrConflict) {
			return apigen.CreateAccount409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "CONFLICT",
					Message: err.Error(),
				},
			}, nil
		}

		if errors.Is(err, errs.ErrNotFound) {
			return apigen.CreateAccount400JSONResponse{
				BadRequestJSONResponse: apigen.BadRequestJSONResponse{
					Code:    "NOT_FOUND",
					Message: err.Error(),
				},
			}, nil
		}

		// Errore interno
		return apigen.CreateAccount500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	// Restituire success (201 Created) con i dati della spesa creata
	return apigen.CreateAccount201JSONResponse(apigen.CreateAccount201JSONResponse{
		Id:             &accountId,
		Name:           &body.Name,
		Currency:       &body.Currency,
		InitialBalance: &body.InitialBalance,
	}), nil
}

func (ctrl *Controller) CreateCategory(ctx context.Context, request apigen.CreateCategoryRequestObject) (apigen.CreateCategoryResponseObject, error) {
	// Validare che il body sia presente
	if request.Body == nil {
		return apigen.CreateCategory400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_REQUEST",
				Message: "body richiesto",
			},
		}, nil
	}

	body := request.Body

	// Validare i dati richiesti
	if len(body.Name) == 0 || len(body.CategoryType) == 0 {
		return apigen.CreateCategory400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "name e categoryType sono obbligatori",
			},
		}, nil
	}

	// Mappare il body della request al DTO interno
	createCategoryDto := ToCreateCategoryDto(body)

	// Creare la categoria
	category, err := ctrl.accountService.CreateCategory(ctx, createCategoryDto)
	if err != nil {
		// Gestire i diversi tipi di errore
		if errors.Is(err, errs.ErrConflict) {
			return apigen.CreateCategory409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "CONFLICT",
					Message: err.Error(),
				},
			}, nil
		}

		// Errore interno
		return apigen.CreateCategory500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	// Restituire success (201 Created) con i dati della categoria creata
	return apigen.CreateCategory201JSONResponse(apigen.CreateCategoryResponse{
		Id:           &category.ID,
		Name:         &category.Name,
		CategoryType: &category.Type,
	}), nil
}

func (ctrl *Controller) AddTransaction(ctx context.Context, request apigen.AddTransactionRequestObject) (apigen.AddTransactionResponseObject, error) {
	// Validare che il body sia presente
	if request.Body == nil {
		return apigen.AddTransaction400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_REQUEST",
				Message: "body richiesto",
			},
		}, nil
	}

	body := request.Body

	// Validare i dati richiesti
	if len(body.AccountName) == 0 || body.Amount == 0 {
		return apigen.AddTransaction400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "accountName e amount sono obbligatori",
			},
		}, nil
	}

	// Mappare il body della request al DTO interno
	addExpenseDto := ToAddExpenseDto(body)

	// Eseguire la transazione
	transactionId, err := ctrl.accountService.AddTransaction(ctx, addExpenseDto)
	if err != nil {
		// Gestire i diversi tipi di errore
		if errors.Is(err, errs.ErrConflict) {
			return apigen.AddTransaction409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "CONFLICT",
					Message: err.Error(),
				},
			}, nil
		}

		if errors.Is(err, errs.ErrNotFound) {
			return apigen.AddTransaction400JSONResponse{
				BadRequestJSONResponse: apigen.BadRequestJSONResponse{
					Code:    "NOT_FOUND",
					Message: err.Error(),
				},
			}, nil
		}

		// Errore interno
		return apigen.AddTransaction500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	// Restituire success (201 Created) con i dati della spesa creata
	return apigen.AddTransaction201JSONResponse(apigen.AddTransactionResponse{
		Amount:        &body.Amount,
		TransactionId: &transactionId,
	}), nil
}

func (ctrl *Controller) TransferBetweenAccounts(ctx context.Context, request apigen.TransferBetweenAccountsRequestObject) (apigen.TransferBetweenAccountsResponseObject, error) {
	if request.Body == nil {
		return apigen.TransferBetweenAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_REQUEST",
				Message: "body richiesto",
			},
		}, nil
	}

	body := request.Body
	if body.AccountFrom == "" || body.AccountTo == "" || body.Amount <= 0 {
		return apigen.TransferBetweenAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "accountFrom, accountTo e amount sono obbligatori",
			},
		}, nil
	}
	if body.AccountFrom == body.AccountTo {
		return apigen.TransferBetweenAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "accountFrom e accountTo devono essere diversi",
			},
		}, nil
	}

	transferDto := dto.TransferBetweenAccountsDto{
		AccountFrom: body.AccountFrom,
		AccountTo:   body.AccountTo,
		Amount:      body.Amount,
		OccurredAt:  body.OccurredAt.Time,
	}
	if body.Description != nil {
		transferDto.Description = body.Description
	}

	transactionID, err := ctrl.accountService.TransferBetweenAccounts(ctx, transferDto)
	if err != nil {
		if errors.Is(err, errs.ErrConflict) {
			return apigen.TransferBetweenAccounts409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "CONFLICT",
					Message: err.Error(),
				},
			}, nil
		}
		if errors.Is(err, errs.ErrAccountNotFound) || errors.Is(err, errs.ErrNotFound) {
			return apigen.TransferBetweenAccounts400JSONResponse{
				BadRequestJSONResponse: apigen.BadRequestJSONResponse{
					Code:    "NOT_FOUND",
					Message: err.Error(),
				},
			}, nil
		}
		if errors.Is(err, errs.ErrInsufficientBalance) {
			return apigen.TransferBetweenAccounts409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "INSUFFICIENT_BALANCE",
					Message: err.Error(),
				},
			}, nil
		}
		return apigen.TransferBetweenAccounts500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	return apigen.TransferBetweenAccounts201JSONResponse(apigen.TransferBetweenAccountsResponse{
		TransactionId: &transactionID,
	}), nil
}

func (ctrl *Controller) GetAccounts(ctx context.Context, request apigen.GetAccountsRequestObject) (apigen.GetAccountsResponseObject, error) {
	accounts, err := ctrl.accountService.GetAccounts(ctx)
	if err != nil {
		return apigen.GetAccounts500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	response := make([]apigen.AccountItem, len(accounts))
	for i, account := range accounts {
		balance, err := ctrl.accountService.GetAccountBalance(ctx, account.ID)
		if err != nil {
			return apigen.GetAccounts500JSONResponse{
				InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
					Code:    "INTERNAL_ERROR",
					Message: err.Error(),
				},
			}, nil
		}
		currentBalance := account.InitialBalance + balance

		response[i] = apigen.AccountItem{
			Id:             &account.ID,
			Name:           &account.Name,
			Currency:       &account.Currency,
			InitialBalance: &account.InitialBalance,
			CurrentBalance: &currentBalance,
		}
	}

	return apigen.GetAccounts200JSONResponse(response), nil
}

func (ctrl *Controller) GetRecentTransactions(ctx context.Context, request apigen.GetRecentTransactionsRequestObject) (apigen.GetRecentTransactionsResponseObject, error) {
	limit := int32(20)
	if request.Params.Limit != nil && *request.Params.Limit > 0 {
		limit = *request.Params.Limit
	}

	entries, err := ctrl.accountService.GetRecentTransactions(ctx, limit)
	if err != nil {
		return apigen.GetRecentTransactions500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	response := make([]apigen.TransactionItem, len(entries))
	for i, entry := range entries {
		var categoryName *string
		if entry.CategoryName.Valid {
			categoryName = &entry.CategoryName.String
		}
		var categoryType *string
		if entry.CategoryType.Valid {
			categoryType = &entry.CategoryType.String
		}
		occurredAt := openapi_types.Date{Time: entry.OccurredAt}

		var desc *string
		if entry.Description.Valid {
			desc = &entry.Description.String
		}
		response[i] = apigen.TransactionItem{
			TransactionId: &entry.TransactionID,
			OccurredAt:    &occurredAt,
			AccountName:   &entry.AccountName,
			CategoryName:  categoryName,
			CategoryType:  categoryType,
			Amount:        &entry.Amount,
			Description:   desc,
		}
	}

	return apigen.GetRecentTransactions200JSONResponse(response), nil
}

func (ctrl *Controller) GetCategories(ctx context.Context, request apigen.GetCategoriesRequestObject) (apigen.GetCategoriesResponseObject, error) {
	categories, err := ctrl.accountService.GetCategories(ctx)
	if err != nil {
		return apigen.GetCategories500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	// Mappare le categorie al formato di risposta
	response := make([]apigen.CategoryItem, len(categories))
	for i, category := range categories {
		response[i] = apigen.CategoryItem{
			Id:           &category.ID,
			Name:         &category.Name,
			CategoryType: (*string)(&category.Type),
		}
	}

	return apigen.GetCategories200JSONResponse(response), nil
}
