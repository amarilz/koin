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

	accountRepo := postgres.NewAccountRepository(db)
	categoryRepo := postgres.NewCategoryRepository(db)
	recordingRepo := postgres.NewVoiceRecordingRepository(db)
	accountService := service.NewAccountService(accountRepo, categoryRepo)
	controller := http.NewController(accountService)
	transcriptionURL := os.Getenv("TRANSCRIPTION_SERVICE_URL")
	if transcriptionURL == "" {
		transcriptionURL = "http://whisper:8000/transcribe"
	}
	transcriptionClient, err := service.NewVoiceTranscriptionClient(transcriptionURL)
	if err != nil {
		log.Fatal(err)
	}
	transcriptionService := service.NewVoiceTranscriptionService(recordingRepo, transcriptionClient)
	go transcriptionService.Run(context.Background())

	routerDeps := http.RouterDeps{
		Controller: controller,
		Recordings: recordingRepo,
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
		return err
	}
	m.Log = migrationLogger{}
	defer m.Close()

	if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		return err
	} else if errors.Is(err, migrate.ErrNoChange) {
		log.Println("no migrations to apply")
		return nil
	}
	log.Println("migrations applied")
	return nil
}

type migrationLogger struct{}

func (migrationLogger) Printf(format string, v ...interface{}) {
	log.Printf(format, v...)
}

func (migrationLogger) Verbose() bool {
	return false
}
