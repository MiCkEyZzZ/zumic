##@ Build
.PHONY: build build-release
build: ## Сборка debug
	cargo build

build-release: ## Сборка релизной сборки
	cargo build --release

##@ Test
.PHONY: check clippy nextest test
check: ## Cargo check
	cargo check

clippy: ## Clippy (treat warnings as errors)
	cargo clippy -- -D warnings

nextest: ## Nextest
	cargo nextest run

test: ## Cargo test (обычные тесты)
	cargo test

##@ Format & Lints
.PHONY: fmt fmt-toml fmt-all check-toml check-all
fmt: ## Rust fmt
	cargo fmt --all

fmt-toml: ## TOML fmt
	taplo format

fmt-all: ## Форматирование всего
	$(MAKE) fmt
	$(MAKE) fmt-toml

check-toml: ## Проверка TOML-формата
	taplo format --check

check-all: ## Полная проверка (check + clippy + формат + toml)
	$(MAKE) check
	$(MAKE) clippy
	$(MAKE) fmt
	$(MAKE) check-toml

##@ Bench & Fuzz
.PHONY: bench fuzz
bench: ## Бенчмарки
	cargo bench

fuzz: ## Fuzz tests
	cargo fuzz run

##@ Misc
.PHONY: clean
clean: ## Очистка артефактов
	cargo clean

##@ Git
.PHONY: git-add git-commit git-push git-status
git-add: ## Добавить все изменения в индекс
	git add .

git-commit: ## Закоммитить изменения. Использование: make git-commit MSG="Your message"
ifndef MSG
	$(error MSG is not set. Use make git-commit MSG="your message")
endif
	git commit -m "$(MSG)"

git-push: ## Отправить коммиты на удалённый репозиторий
	git push

git-status: ## Показать статус репозитория
	git status

##@ Git Release
.PHONY: git-tag git-push-tag git-release release-auto bump-version release-all
git-tag: ## Создание git-тега. Пример: make git-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-tag VERSION=v0.2.0)
endif
	git tag $(VERSION)

git-push-tag: ## Отправить тег в origin. Пример: make git-push-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-push-tag VERSION=v0.2.0)
endif
	git push origin $(VERSION)

git-release: ## Полный релиз: tag + push + GitHub Release. Пример: make git-release VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-release VERSION=v0.2.0)
endif
	@# Проверка наличия gh
	@if ! command -v gh >/dev/null 2>&1; then \
	  echo "Error: GitHub CLI (gh) not found. Please install and authenticate."; \
	  exit 1; \
	fi
	$(MAKE) git-tag VERSION=$(VERSION)
	$(MAKE) git-push-tag VERSION=$(VERSION)
	gh release create $(VERSION) --generate-notes --allow-dirty

# Автоматический релиз по версии из Cargo.toml
VERSION := v$(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)
release-auto: ## Автоматический релиз (tag + push) по версии из Cargo.toml
	$(MAKE) git-release VERSION=$(VERSION)

bump-version: ## Бампит патч-версию в Cargo.toml (cargo-edit)
	cargo set-version --bump patch
	git add Cargo.toml
	git commit -m "chore: bump version to $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)"

release-all: ## Полный цикл релиза: bump-version + release-auto
	$(MAKE) bump-version
	$(MAKE) release-auto

##@ Help
help: ## Показать это сообщение
	@echo
	@echo "Zumic Makefile (version $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml))"
	@echo "Usage: make [target]"
	@echo
	@awk 'BEGIN {FS = ":.*##"; \
	  printf " %-20s %s\n", "Target", "Description"; \
	  printf " -------------------- ------------------------------\n"} \
	  /^[a-zA-Z0-9_-]+:.*?##/ { printf " \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	  /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
	@echo
