.PHONY: build fmt check clippy test clean help

build: ## Собрать проект в debug-режиме
	cargo build
release: ## Собрать проект в release-режиме
	cargo build --release
fmt: ## Отформатировать весь Rust код
	cargo fmt --all
check: ## Быстрая проверка кода без сборки
	cargo check
clippy: ## Статический анализ кода с Clippy
	cargo clippy -- -D warnings
test: ## Запустить все тесты
	cargo test
clean: ## Очистить проект
	cargo clean

help: ## Показать доступные команды
	@echo "Доступные команды:"
	@echo "  make build       - сборка debug"
	@echo "  make release     - сборка release"
	@echo "  make fmt         - форматирование кода"
	@echo "  make check       - быстрая проверка кода"
	@echo "  make clippy      - статический анализ"
	@echo "  make test        - запуск тестов"
	@echo "  make clean       - очистка сборки"
