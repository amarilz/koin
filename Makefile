# Variabili del progetto
BINARY_NAME=koin
BUILD_DIR=bin
MAIN_PATH=./main.go
VERSION ?= $(shell cat VERSION 2>/dev/null || echo dev)

# Comandi
GO ?= go
GOCLEAN = $(GO) clean
GOTEST = $(GO) test
GOBUILD = $(GO) build

# Configurare DATABASE_URL per eseguire le migrazioni.
DATABASE_URL ?=

# Percorsi API
API_CONFIG=./internal/api/open-api-codegen.yaml
API_SPEC=./internal/api/api.yaml

.PHONY: all build clean test run generate generate-api generate-db migrate docker-build help

# Task di default: genera il codice e compila senza richiedere un database attivo
all: build

## migrate: Applica le migrazioni PostgreSQL (richiede DATABASE_URL)
migrate:
	@if [ -z "$(DATABASE_URL)" ]; then \
		echo "DATABASE_URL non impostata; specificare l'URL PostgreSQL per eseguire le migrazioni." >&2; \
		exit 1; \
	fi
	$(GO) tool migrate -source file://./internal/db/migrations -database "$(DATABASE_URL)" -verbose up

## build: Compila il binario del progetto
build: generate
	@echo "Costruzione del binario (VERSION=$(VERSION))..."
	@mkdir -p $(BUILD_DIR)
	$(GOBUILD) -ldflags "-X koin/internal/version.Version=$(VERSION)" -o $(BUILD_DIR)/$(BINARY_NAME) $(MAIN_PATH)

## docker-build: Costruisce l'immagine Docker con tag versione e latest
docker-build:
	docker build --build-arg VERSION=$(VERSION) -t $(BINARY_NAME):$(VERSION) -t $(BINARY_NAME):latest .

## generate: Esegue tutti i generatori di codice (API e SQL)
generate: generate-api generate-db

## generate-api: Genera il codice dall'OpenAPI spec
generate-api:
	@echo "Generazione codice API (oapi-codegen)..."
	$(GO) tool oapi-codegen -config $(API_CONFIG) $(API_SPEC)

## generate-db: Genera il codice Go dalle query SQL (sqlc)
generate-db:
	@echo "Generazione codice DB (sqlc)..."
	$(GO) tool sqlc generate -f ./internal/db/sqlc.yaml

## run: Compila ed esegue il progetto
run: build
	./$(BUILD_DIR)/$(BINARY_NAME)

## test: Esegue i test
test:
	$(GOTEST) -v ./...

## clean: Pulisce file temporanei e binari
clean:
	@echo "Pulizia in corso..."
	$(GOCLEAN)
	rm -rf $(BUILD_DIR)

## help: Mostra questa guida
help:
	@echo "Uso: make [target]"
	@echo ""
	@echo "Target disponibili:"
	@awk 'BEGIN{FS=":"} /^##/{desc=$$0; sub(/^## [^:]+:[ ]*/, "", desc); next} /^[a-zA-Z_-]+:/{target=$$1; if (desc != "") {printf "\033[36m%-15s\033[0m %s\n", target, desc; desc=""}}' $(MAKEFILE_LIST) | sort
