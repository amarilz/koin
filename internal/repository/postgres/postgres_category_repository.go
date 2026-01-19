package postgres

import (
	"context"
	"database/sql"
	"fmt"
	"koin/internal/model/dto"

	dbgen "koin/internal/db/generated"
)

type CategoryRepository struct {
	queries *dbgen.Queries
	db      *sql.DB
}

func NewCategoryRepository(db *sql.DB) *CategoryRepository {
	return &CategoryRepository{
		db:      db,
		queries: dbgen.New(db),
	}
}

func (repo *CategoryRepository) GetCategory(ctx context.Context, user dbgen.User, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error) {
	category, err := repo.queries.GetCategory(ctx, dbgen.GetCategoryParams{
		UserID: user.ID,
		Name:   categoryName,
		Type:   string(categoryType),
	})
	if err != nil {
		return dbgen.Category{}, err
	}
	return category, nil
}

func (repo *CategoryRepository) CreateCategory(ctx context.Context, user dbgen.User, categoryName string, categoryType dto.CategoryType) (dbgen.Category, error) {
	category, err := repo.queries.CreateCategory(ctx, dbgen.CreateCategoryParams{
		UserID: user.ID,
		Name:   categoryName,
		Type:   string(categoryType),
	})
	if err != nil {
		return dbgen.Category{}, fmt.Errorf("create category %q: %w", categoryName, err)
	}
	return category, nil
}

func (repo *CategoryRepository) GetCategories(ctx context.Context, user dbgen.User) ([]dbgen.Category, error) {
	categories, err := repo.queries.GetCategoriesByUser(ctx, user.ID)
	if err != nil {
		return nil, fmt.Errorf("get categories by user: %w", err)
	}
	return categories, nil
}
