package main

import (
	"context"
	"database/sql"
	"errors"
	"log"
	"os"
	"time"

	"koin/internal/api/http"
	"koin/internal/version"

	_ "github.com/jackc/pgx/v5/stdlib"

	"koin/internal/repository/postgres"
	"koin/internal/service"

	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
)

func main() {
	log.Printf("koin version: %s", version.Version)
	authToken := os.Getenv("API_TOKEN")
	if authToken == "" {
		authToken = "dev-token" // solo per sviluppo
	}

	dsn := os.Getenv("DATABASE_URL")
	if dsn == "" {
		dsn = "postgres://myuser:mypassword@localhost:5432/mydb?sslmode=disable"
	}

	// Apply migrations using the configured DATABASE_URL so compose env is respected
	err := migrateDatabase(dsn)
	if err != nil {
		log.Fatal(err)
	}

	db, err := sql.Open("pgx", dsn)
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := db.PingContext(ctx); err != nil {
		log.Fatal(err)
	}

	userRepo := postgres.NewUserRepository(db)
	accountRepo := postgres.NewAccountRepository(db)
	categoryRepo := postgres.NewCategoryRepository(db)
	userService := service.NewUserService(userRepo)
	accountService := service.NewAccountService(userRepo, accountRepo, categoryRepo)
	controller := http.NewController(userService, accountService)

	routerDeps := http.RouterDeps{
		AuthToken:   authToken,
		Controller:  controller,
		UserService: userService,
	}
	router := http.NewRouter(routerDeps)

	addr := ":8080"
	log.Printf("listening on %s", addr)
	if err := router.Run(addr); err != nil {
		log.Fatal(err)
	}
}

func migrateDatabase(dsn string) error {
	m, err := migrate.New(
		"file://./internal/db/migrations",
		dsn,
	)
	if err != nil {
		log.Fatal(err)
	}
	if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		log.Fatal(err)
	}
	log.Println("migrations applied")
	return err
}
