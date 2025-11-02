# The Zumic Makefile
#
# Набор удобных команд для разработки Zumic.
# Основные возможности:
#  - Сборка debug/release:   make build / make build-release
#  - Запуск:                 make run / make run-release
#  - Тесты и property тесты: make test / make proptest / make stress-test
#  - Форматирование/линты:   make fmt / make clippy / make check-all
#  - Fuzz и benchmark:       make fuzz / make bench
#  - Управление git/релизом: make git-tag / make git-release
#
# Переменные:
#  BUILD_TARGET  - читается из .cargo/config.toml (target triple)
#  TARGET_ARG    - автоматически формируется для cargo (--target ...)
#  TARGET_DIR    - путь в target/ для выбранного target'а
#  VERSION       - автоматически берётся из Cargo.toml (используется в release-auto)
#  ZUMIC_BANNER  - контролирует баннер при запуске

BUILD_TARGET := $(shell test -f .cargo/config.toml && grep -E '^\s*target\s*=' .cargo/config.toml | head -1 | cut -d'"' -f2)
TARGET_ARG   := $(if $(BUILD_TARGET),--target $(BUILD_TARGET),)
TARGET_DIR   := target/$(if $(BUILD_TARGET),$(BUILD_TARGET)/,)
VERSION      := v$(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)

##@ Build
.PHONY: build build-release build-all-platforms

build: ## Сборка debug
	cargo build $(TARGET_ARG)

build-release: ## Сборка релизной версии
	cargo build --release $(TARGET_ARG)

build-all-platforms: ## Сборка для всех платформ (как в CI)
	@echo "Building for multiple platforms..."
	cargo build --release --target x86_64-unknown-linux-gnu
	cargo build --release --target aarch64-unknown-linux-gnu
	cargo build --release --target x86_64-apple-darwin
	cargo build --release --target x86_64-pc-windows-msvc

##@ Test
.PHONY: check clippy clippy-ci nextest test miri miri-test test-ci

check: ## Cargo проверка
	cargo check

clippy: ## Clippy (рассматривать предупреждения как ошибки)
	cargo clippy -- -D warnings

clippy-ci: ## Clippy как в CI: все таргеты и все фичи, warnings -> error
	cargo clippy --all-targets --all-features -- -D warnings

nextest: ## Nextest
	cargo nextest run

test: ## Cargo test (обычные тесты)
	cargo test

test-ci: ## Полный набор тестов как в CI
	cargo fmt -- --check
	$(MAKE) clippy-ci
	$(MAKE) test-all

miri: ## Запустите все тесты в Miri
	cargo miri test

miri-test: ## Запустите определенный тест в Miri. Использование: make miri-test TEST="модуль::имя_теста"
	cargo miri test $(TEST)

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
.PHONY: bench fuzz fuzz-target fuzz-long fuzz-quick fuzz-decode

bench: ## Бенчмарки
	cargo bench

fuzz: ## Fuzz tests (2 минуты, decode_value)
	./scripts/run_fuzz.sh decode_value 2

fuzz-target: ## Запуск конкретного fuzz target. Использование: make fuzz-target TARGET=decode_value MINUTES=10
ifndef TARGET
	$(error TARGET is not set. Use make fuzz-target TARGET=decode_value MINUTES=10)
endif
	./scripts/run_fuzz.sh $(TARGET) $(MINUTES)

fuzz-long: ## Длительный фаззинг (60 минут)
	./scripts/run_fuzz.sh decode_value 60

fuzz-quick: ## Быстрый фаззинг (1 минута)
	./scripts/run_fuzz.sh decode_value 1

fuzz-decode: ## Фаззинг decode_value (10 минут)
	./scripts/run_fuzz.sh decode_value 10

fuzz-build: ## Сборка fuzz target без запуска
	cd fuzz && cargo +nightly fuzz build decode_value

fuzz-clean: ## Очистка fuzz артефактов
	rm -rf fuzz/artifacts/*
	rm -rf fuzz/corpus/*
	rm -rf results/*

##@ Misc
.PHONY: clean clean-all

clean: ## Очистка артефактов
	cargo clean

clean-all: ## Полная очистка (включая fuzz)
	cargo clean
	$(MAKE) fuzz-clean

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
.PHONY: git-tag git-push-tag git-release release-auto bump-version release-all prepare-release

git-tag: ## Создание git-тега. Пример: make git-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-tag VERSION=v0.2.0)
endif
	git tag -a $(VERSION) -m "Release $(VERSION)"

git-push-tag: ## Отправить тег в origin. Пример: make git-push-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-push-tag VERSION=v0.2.0)
endif
	git push origin $(VERSION)

prepare-release: ## Подготовка релиза (обновить версию, changelog). Использование: make prepare-release VERSION=v0.5.0
ifndef VERSION
	$(error VERSION is not set. Use make prepare-release VERSION=v0.5.0)
endif
	@if [ -f scripts/prepare-release.sh ]; then \
		./scripts/prepare-release.sh $(VERSION); \
	else \
		echo "Error: scripts/prepare-release.sh not found"; \
		exit 1; \
	fi

git-release: ## Полный релиз: tag + push. Пример: make git-release VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-release VERSION=v0.2.0)
endif
	$(MAKE) git-tag VERSION=$(VERSION)
	$(MAKE) git-push-tag VERSION=$(VERSION)
	@echo "✅ Tag $(VERSION) pushed. GitHub Actions will create the release automatically."
	@echo "Monitor: https://github.com/MiCkEyZzZ/zumic/actions"

release-auto: ## Автоматический релиз по версии из Cargo.toml
	$(MAKE) git-release VERSION=$(VERSION)

bump-version: ## Бампит патч-версию в Cargo.toml (cargo-edit)
	cargo set-version --bump patch
	git add Cargo.toml Cargo.lock
	git commit -m "chore: bump version to $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)"

release-all: ## Полный цикл релиза: prepare-release + tests + push
	@read -p "Version (e.g., v0.5.0): " ver; \
	$(MAKE) prepare-release VERSION=$$ver && \
	$(MAKE) test-ci && \
	echo "Tests passed. Ready to commit and push." && \
	read -p "Continue with git commit and tag? [y/N] " confirm; \
	if [ "$$confirm" = "y" ]; then \
		git add -A && \
		git commit -m "chore: prepare release $$ver" && \
		$(MAKE) git-release VERSION=$$ver; \
	fi

##@ Property testing команды
.PHONY: proptest proptest-quick proptest-long proptest-verbose proptest-coverage proptest-continuous proptest-timing \
        stress-test stress-test-quick endurance-test test-all find-bugs-fast

proptest-quick: ## Быстрые property tests (100 случаев)
	PROPTEST_CASES=100 cargo test --test property_tests

proptest: ## Обычные property tests (по умолчанию)
	cargo test --test property_tests

proptest-long: ## Длительное property testing
	PROPTEST_CASES=10000 cargo test --test property_tests

proptest-verbose: ## Подробный вывод для property tests
	PROPTEST_CASES=1000 RUST_LOG=debug cargo test --test property_tests -- --nocapture

proptest-coverage: ## Генерация покрытия для property tests (tarpaulin, HTML)
	cargo tarpaulin --tests --out Html --output-dir coverage/ --test property_tests

proptest-continuous: ## Бесконечный цикл property tests (оставлять с осторожностью)
	while true; do \
		echo "Running property tests iteration $$(date)"; \
		PROPTEST_CASES=1000 cargo test --test property_tests || break; \
		sleep 60; \
	done

proptest-timing: ## Измерение времени выполнения property tests
	time PROPTEST_CASES=1000 cargo test --test property_tests

stress-test: ## Запуск стресс-тестов (медленные, много итераций)
	PROPTEST_CASES=10000 cargo test --test stress_tests

stress-test-quick: ## Быстрые стресс-тесты (короткие, для CI)
	PROPTEST_CASES=1000 cargo test --test stress_tests

endurance-test: ## Эндуранс тест для поиска утечек памяти (медленный)
	cargo test --test stress_tests test_endurance_many_iterations --release -- --ignored --nocapture

test-all: ## Полный набор тестов (unit + property + stress)
	cargo test
	$(MAKE) proptest
	$(MAKE) stress-test-quick

find-bugs-fast: ## Минимальный набор тестов, чтобы быстро найти баги
	PROPTEST_CASES=500 cargo test --test property_tests roundtrip_all_values
	PROPTEST_CASES=500 cargo test --test property_tests numeric_edge_cases
	cargo test --test stress_tests test_compression_pathological_cases

##@ Run
.PHONY: run run-full run-compact run-release

run: ## Запуск Зумик в режиме по умолчанию (debug → full)
	cargo run

run-full: ## Запуск Зумик с полным баннером (force)
	ZUMIC_BANNER=full cargo run

run-compact: ## Запуск Зумик с коротким баннером (force)
	ZUMIC_BANNER=compact cargo run

run-release: ## Запуск Зумик в релизной версии
	cargo build --release $(TARGET_ARG) && ./$(TARGET_DIR)release/zumic

##@ CI/CD
.PHONY: ci-local simulate-ci

ci-local: ## Запустить проверки как в CI локально
	@echo "==> Running CI checks locally..."
	@echo "==> 1. Format check"
	cargo fmt -- --check
	@echo "==> 2. Clippy"
	$(MAKE) clippy-ci
	@echo "==> 3. Tests"
	cargo test
	@echo "==> 4. Property tests (quick)"
	$(MAKE) proptest-quick
	@echo "✅ All CI checks passed!"

simulate-ci: ## Симуляция полного CI pipeline (медленно)
	@echo "==> Simulating full CI pipeline..."
	$(MAKE) ci-local
	@echo "==> Building release"
	$(MAKE) build-release
	@echo "==> Fuzz test (quick)"
	$(MAKE) fuzz-quick
	@echo "✅ CI simulation complete!"

##@ Help
help: ## Показать это сообщение
	@echo
	@echo "Zumic Makefile (version $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml))"
	@echo "Usage: make [target]"
	@echo
	@awk 'BEGIN {FS = ":.*##"; \
	  printf "%-25s %s\n", "Target", " Description"; \
	  printf "-------------------------  -----------------------------\n"} \
	/^[a-zA-Z0-9_-]+:.*?##/ { printf " \033[36m%-25s\033[0m %s\n", $$1, $$2 } \
	/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)

.DEFAULT_GOAL := help
