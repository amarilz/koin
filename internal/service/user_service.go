package service

import (
	"context"
	"fmt"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
	repo "koin/internal/repository"

	"golang.org/x/crypto/bcrypt"
)

type UserService struct {
	userRepo repo.UserRepository
}

func NewUserService(userRepo repo.UserRepository) *UserService {
	return &UserService{
		userRepo: userRepo,
	}
}

func (userService *UserService) CreateUser(ctx context.Context, createUserDto dto.CreateUserDto) (dbgen.User, error) {
	user, err := userService.userRepo.CreateUser(ctx, createUserDto)
	if err != nil {
		return dbgen.User{}, err
	}
	return user, nil
}

func (userService *UserService) GetUserByEmail(ctx context.Context, email string) (dbgen.User, error) {
	return userService.userRepo.GetUser(ctx, email)
}

func (userService *UserService) GetUserByID(ctx context.Context, userID int64) (dbgen.User, error) {
	return userService.userRepo.GetUserByID(ctx, userID)
}

// Login verifica le credenziali e restituisce l'utente se valide
func (userService *UserService) Login(ctx context.Context, email, password string) (dbgen.User, error) {
	// Recuperare l'utente dal database per email
	user, err := userService.userRepo.GetUser(ctx, email)
	if err != nil {
		return dbgen.User{}, fmt.Errorf("credenziali non valide")
	}

	// Verificare la password usando bcrypt
	err = bcrypt.CompareHashAndPassword([]byte(user.PasswordHash), []byte(password))
	if err != nil {
		return dbgen.User{}, fmt.Errorf("credenziali non valide")
	}

	return user, nil
}
