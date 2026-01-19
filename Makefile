# Variabili del progetto
BINARY_NAME=koin
BUILD_DIR=bin
MAIN_PATH=./main.go
VERSION ?= $(shell cat VERSION 2>/dev/null || echo dev)

# Comandi
GO=go
GOCLEAN=$(GO) clean
GOTEST=$(GO) test
GOBUILD=$(GO) build

# Percorsi API
API_CONFIG=./internal/api/open-api-codegen.yaml
API_SPEC=./internal/api/api.yaml

.PHONY: all build clean test run generate generate-api generate-db help
.PHONY: compose-prod-up compose-prod-down-backup

# Task di default: genera tutto e compila
all: generate migrate build

migrate:
	migrate -source file://./internal/db/migrations -database postgres://myuser:mypassword@localhost:5432/mydb?sslmode=disable -verbose up

## build: Compila il binario del progetto
build: clean generate
	@echo "Costruzione del binario (VERSION=$(VERSION))..."
	@mkdir -p $(BUILD_DIR)
	$(GO) build -ldflags "-X koin/internal/version.Version=$(VERSION)" -o $(BUILD_DIR)/$(BINARY_NAME) $(MAIN_PATH)

## generate: Esegue tutti i generatori di codice (API e SQL)
generate: generate-api generate-db

## generate-api: Genera il codice dall'OpenAPI spec
generate-api:
	@echo "Generazione codice API (oapi-codegen)..."
	oapi-codegen -config $(API_CONFIG) $(API_SPEC)

## generate-db: Genera il codice Go dalle query SQL (sqlc)
generate-db:
	@echo "Generazione codice DB (sqlc)..."
	sqlc generate -f ./internal/db/sqlc.yaml

## run: Esegue il progetto (genera il codice prima di partire)
run: build
	$(GO) run $(MAIN_PATH)

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
	@awk 'BEGIN{FS=":"} /^##/{sub(/^##[ ]?/, "", $$0); desc=$$0; next} /^[a-zA-Z_-]+:/{if (desc != "") {printf "\033[36m%-15s\033[0m %s\n", $$1, desc; desc=""}}' $(MAKEFILE_LIST) | sort
