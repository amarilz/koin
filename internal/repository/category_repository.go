package repository

import (
	"context"
	dbgen "koin/internal/db/generated"
	"koin/internal/model/dto"
)

type CategoryRepository interface {
	GetCategory(ctx context.Context, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error)
	CreateCategory(ctx context.Context, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error)
	GetCategories(ctx context.Context) ([]dbgen.Category, error)
}
