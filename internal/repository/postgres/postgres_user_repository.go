package postgres

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"koin/internal/model/dto"

	dbgen "koin/internal/db/generated"
	apierr "koin/internal/errors"

	"golang.org/x/crypto/bcrypt"
)

type UserRepository struct {
	queries *dbgen.Queries
	db      *sql.DB // utile se vuoi transazioni
}

func NewUserRepository(db *sql.DB) *UserRepository {
	return &UserRepository{
		db:      db,
		queries: dbgen.New(db),
	}
}

func (userRepo *UserRepository) CreateUser(
	ctx context.Context,
	createUserDto dto.CreateUserDto,
) (dbgen.User, error) {
	// verifica esistenza utente
	_, err := userRepo.queries.GetUser(ctx, createUserDto.Email)
	if err == nil {
		return dbgen.User{}, fmt.Errorf("user %q already exists", createUserDto.Email)
	}
	if err != sql.ErrNoRows {
		return dbgen.User{}, fmt.Errorf("check user existence: %w", err)
	}

	// hash password
	hashedPassword, err := bcrypt.GenerateFromPassword(
		[]byte(createUserDto.Password),
		bcrypt.DefaultCost,
	)
	if err != nil {
		return dbgen.User{}, fmt.Errorf("hash password: %w", err)
	}

	// creo nuovo utente
	user, err := userRepo.queries.CreateUser(ctx, dbgen.CreateUserParams{
		Email:        createUserDto.Email,
		PasswordHash: string(hashedPassword),
	})
	if err != nil {
		return dbgen.User{}, fmt.Errorf("create user %q: %w", createUserDto.Email, err)
	}

	return user, nil
}

func (userRepo *UserRepository) GetUser(ctx context.Context, email string) (dbgen.User, error) {
	user, err := userRepo.queries.GetUser(ctx, email)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return dbgen.User{}, fmt.Errorf("%w: %s", apierr.ErrUserNotFound, email)
		}
		return dbgen.User{}, fmt.Errorf("get user by email %q: %w", email, err)
	}
	return user, nil
}

func (userRepo *UserRepository) GetUserByEmail(ctx context.Context, email string) (dbgen.User, error) {
	return userRepo.GetUser(ctx, email)
}

func (userRepo *UserRepository) GetUserByID(ctx context.Context, userID int64) (dbgen.User, error) {
	user, err := userRepo.queries.GetUserByID(ctx, userID)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return dbgen.User{}, fmt.Errorf("%w: user ID %d not found", apierr.ErrUserNotFound, userID)
		}
		return dbgen.User{}, fmt.Errorf("get user by ID %d: %w", userID, err)
	}
	return user, nil
}
