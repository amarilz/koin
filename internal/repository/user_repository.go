package repository

import (
	"context"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
)

type UserRepository interface {
	CreateUser(ctx context.Context, createUserDto dto.CreateUserDto) (dbgen.User, error)
	GetUser(ctx context.Context, email string) (dbgen.User, error)
	GetUserByEmail(ctx context.Context, email string) (dbgen.User, error)
	GetUserByID(ctx context.Context, userID int64) (dbgen.User, error)
}
