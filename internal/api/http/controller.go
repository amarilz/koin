package http

import (
	"context"
	"errors"
	"net/http"

	apigen "koin/internal/api/generated"
	errs "koin/internal/errors"
	"koin/internal/model/dto"
	"koin/internal/service"

	"github.com/gin-contrib/sessions"
	"github.com/gin-gonic/gin"
	openapi_types "github.com/oapi-codegen/runtime/types"
)

type Controller struct {
	userService    *service.UserService
	accountService *service.AccountService
}

func NewController(userService *service.UserService, accountService *service.AccountService) apigen.ServerInterface {
	controller := &Controller{
		userService:    userService,
		accountService: accountService,
	}
	return apigen.NewStrictHandler(controller, nil)
}

func Health(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"status": "ok"})
}

func (ctrl *Controller) CreateUser(ctx context.Context, request apigen.CreateUserRequestObject) (apigen.CreateUserResponseObject, error) {
	// Validare che il body sia presente
	if request.Body == nil {
		return apigen.CreateUser400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_REQUEST",
				Message: "body richiesto",
			},
		}, nil
	}

	body := request.Body

	// Validare i dati richiesti
	if len(body.Email) == 0 || len(body.Password) == 0 {
		return apigen.CreateUser400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "name ed email sono obbligatori",
			},
		}, nil
	}

	// Mappare il body della request al DTO interno
	createUserDto := ToCreateUserDto(body)

	// Creare l'utente
	user, err := ctrl.userService.CreateUser(ctx, createUserDto)
	if err != nil {
		// Gestire i diversi tipi di errore
		if errors.Is(err, errs.ErrConflict) {
			return apigen.CreateUser409JSONResponse{
				ConflictJSONResponse: apigen.ConflictJSONResponse{
					Code:    "CONFLICT",
					Message: err.Error(),
				},
			}, nil
		}

		// Errore interno
		return apigen.CreateUser500JSONResponse{
			InternalErrorJSONResponse: apigen.InternalErrorJSONResponse{
				Code:    "INTERNAL_ERROR",
				Message: err.Error(),
			},
		}, nil
	}

	// Restituire success (201 Created) con i dati dell'utente creato
	return apigen.CreateUser201JSONResponse(apigen.CreateUserResponse{
		Email:     &user.Email,
		CreatedAt: &user.CreatedAt,
	}), nil
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
	if body.UserId == 0 || len(body.Name) == 0 || len(body.Currency) == 0 {
		return apigen.CreateAccount400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId, name, currency e initialBalance sono obbligatori",
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
		UserId:         &body.UserId,
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
	if body.UserId == 0 || len(body.Name) == 0 || len(body.CategoryType) == 0 {
		return apigen.CreateCategory400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId, name e categoryType sono obbligatori",
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
		UserId:       &body.UserId,
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
	if body.UserId == 0 || len(body.AccountName) == 0 || body.Amount == 0 {
		return apigen.AddTransaction400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId, accountName e amount sono obbligatori",
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
	if body.UserId == 0 || body.AccountFrom == "" || body.AccountTo == "" || body.Amount <= 0 {
		return apigen.TransferBetweenAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId, accountFrom, accountTo e amount sono obbligatori",
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
		UserID:      body.UserId,
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
		if errors.Is(err, errs.ErrAccountNotFound) || errors.Is(err, errs.ErrUserNotFound) || errors.Is(err, errs.ErrNotFound) {
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
	userID := request.Params.UserId
	if userID == 0 {
		return apigen.GetAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId è obbligatorio",
			},
		}, nil
	}

	user, err := ctrl.userService.GetUserByID(ctx, userID)
	if err != nil {
		return apigen.GetAccounts400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "NOT_FOUND",
				Message: "Utente non trovato",
			},
		}, nil
	}

	accounts, err := ctrl.accountService.GetAccounts(ctx, user)
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
	userID := request.Params.UserId
	if userID == 0 {
		return apigen.GetRecentTransactions400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId è obbligatorio",
			},
		}, nil
	}

	_, err := ctrl.userService.GetUserByID(ctx, userID)
	if err != nil {
		return apigen.GetRecentTransactions400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "NOT_FOUND",
				Message: "Utente non trovato",
			},
		}, nil
	}

	limit := int32(20)
	if request.Params.Limit != nil && *request.Params.Limit > 0 {
		limit = *request.Params.Limit
	}

	entries, err := ctrl.accountService.GetRecentTransactions(ctx, userID, limit)
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
	userID := request.Params.UserId
	if userID == 0 {
		return apigen.GetCategories400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "INVALID_DATA",
				Message: "userId è obbligatorio",
			},
		}, nil
	}

	user, err := ctrl.userService.GetUserByID(ctx, userID)
	if err != nil {
		return apigen.GetCategories400JSONResponse{
			BadRequestJSONResponse: apigen.BadRequestJSONResponse{
				Code:    "NOT_FOUND",
				Message: "Utente non trovato",
			},
		}, nil
	}

	categories, err := ctrl.accountService.GetCategories(ctx, user)
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

// LoginPage mostra il form di login
func LoginPage(c *gin.Context) {
	c.HTML(http.StatusOK, "login.html", gin.H{
		"Error": c.Query("error"),
	})
}

// SignupPage mostra il form di registrazione
func SignupPage(c *gin.Context) {
	c.HTML(http.StatusOK, "signup.html", gin.H{
		"Error": c.Query("error"),
	})
}

// HandleSignup gestisce il submit del form di registrazione
func HandleSignup(c *gin.Context, userService *service.UserService) {
	email := c.PostForm("email")
	password := c.PostForm("password")
	passwordConfirm := c.PostForm("password_confirm")

	// Validazione base
	if email == "" || password == "" || passwordConfirm == "" {
		c.HTML(http.StatusBadRequest, "signup.html", gin.H{
			"Error": "Email e password sono obbligatori",
			"Email": email,
		})
		return
	}

	// Validazione lunghezza password
	if len(password) < 8 {
		c.HTML(http.StatusBadRequest, "signup.html", gin.H{
			"Error": "La password deve contenere almeno 8 caratteri",
			"Email": email,
		})
		return
	}

	// Validazione maiuscola
	var hasUpper bool
	for _, r := range password {
		if r >= 'A' && r <= 'Z' {
			hasUpper = true
			break
		}
	}
	if !hasUpper {
		c.HTML(http.StatusBadRequest, "signup.html", gin.H{
			"Error": "La password deve contenere almeno una lettera maiuscola",
			"Email": email,
		})
		return
	}

	// Validazione numero
	var hasNumber bool
	for _, r := range password {
		if r >= '0' && r <= '9' {
			hasNumber = true
			break
		}
	}
	if !hasNumber {
		c.HTML(http.StatusBadRequest, "signup.html", gin.H{
			"Error": "La password deve contenere almeno un numero",
			"Email": email,
		})
		return
	}

	// Validazione conferma password
	if password != passwordConfirm {
		c.HTML(http.StatusBadRequest, "signup.html", gin.H{
			"Error": "Le password non corrispondono",
			"Email": email,
		})
		return
	}

	// Creare l'utente
	createUserDto := dto.CreateUserDto{
		Email:    email,
		Password: password,
	}

	user, err := userService.CreateUser(c, createUserDto)
	if err != nil {
		// Gestire errori specifici
		if errors.Is(err, errs.ErrConflict) {
			c.HTML(http.StatusConflict, "signup.html", gin.H{
				"Error": "Email già registrata",
				"Email": email,
			})
			return
		}

		c.HTML(http.StatusInternalServerError, "signup.html", gin.H{
			"Error": "Errore durante la registrazione: " + err.Error(),
			"Email": email,
		})
		return
	}

	// Salvare l'utente nella sessione
	session := sessions.Default(c)
	session.Set("userID", user.ID)
	session.Set("email", user.Email)
	err = session.Save()
	if err != nil {
		c.HTML(http.StatusInternalServerError, "signup.html", gin.H{
			"Error": "Errore durante il salvataggio della sessione",
			"Email": email,
		})
		return
	}

	// Reindirizzare alla home
	c.Redirect(http.StatusFound, "/forms")
}

// HandleLogin gestisce il submit del form di login
func HandleLogin(c *gin.Context, userService *service.UserService) {
	email := c.PostForm("email")
	password := c.PostForm("password")

	if email == "" || password == "" {
		c.HTML(http.StatusBadRequest, "login.html", gin.H{
			"Error": "Email e password sono obbligatori",
			"Email": email,
		})
		return
	}

	// Verificare le credenziali
	user, err := userService.Login(c, email, password)
	if err != nil {
		c.HTML(http.StatusUnauthorized, "login.html", gin.H{
			"Error": err.Error(),
			"Email": email,
		})
		return
	}

	// Salvare l'utente nella sessione
	session := sessions.Default(c)
	session.Set("userID", user.ID)
	session.Set("email", user.Email)
	err = session.Save()
	if err != nil {
		c.HTML(http.StatusInternalServerError, "login.html", gin.H{
			"Error": "Errore durante il salvataggio della sessione",
			"Email": email,
		})
		return
	}

	// Reindirizzare alla home
	c.Redirect(http.StatusFound, "/forms")
}

// Logout elimina la sessione
func Logout(c *gin.Context) {
	session := sessions.Default(c)
	session.Delete("userID")
	session.Delete("email")
	session.Save()
	c.Redirect(http.StatusFound, "/login")
}
