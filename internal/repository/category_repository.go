package repository

import (
	"context"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
)

type CategoryRepository interface {
	GetCategory(ctx context.Context, user dbgen.User, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error)
	CreateCategory(ctx context.Context, user dbgen.User, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error)
	GetCategories(ctx context.Context, user dbgen.User) ([]dbgen.Category, error)
}
