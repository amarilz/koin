CARGO ?= cargo
PYTHON ?= python3

.PHONY: all build run test clean help

all: build

## build: Compila il binario Rust in bin/koin
build:
	$(CARGO) build --locked --release
	install -d bin
	install -m 755 target/release/koin bin/koin

## run: Avvia Koin
run:
	$(CARGO) run --locked

## test: Esegue i test Rust e i test di importazione PostgreSQL
test:
	$(CARGO) test --locked
	PYTHONDONTWRITEBYTECODE=1 $(PYTHON) -m unittest discover -s tests -p test_import_postgres_backup.py

## clean: Rimuove i file di build
clean:
	$(CARGO) clean
	rm -f bin/koin

## help: Mostra questa guida
help:
	@echo "Uso: make [target]"
	@awk '/^## / { print substr($$0, 4) }' $(MAKEFILE_LIST)
