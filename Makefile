# Определяем кастомный target, если он задан в .cargo/config.toml
BUILD_TARGET := $(shell test -f .cargo/config.toml && grep -E '^\s*target\s*=' .cargo/config.toml | head -1 | cut -d'"' -f2)
TARGET_ARG   := $(if $(BUILD_TARGET),--target $(BUILD_TARGET),)
TARGET_DIR   := target/$(if $(BUILD_TARGET),$(BUILD_TARGET)/,)

##@ Build
.PHONY: build build-release
build: ## Сборка debug
	cargo build $(TARGET_ARG)

build-release: ## Сборка релизной версии
	cargo build --release $(TARGET_ARG)

##@ Test
.PHONY: check clippy clippy-ci nextest test miri miri-test
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

##@ Property testing команды
.PHONY: proptest proptest-quick proptest-long proptest-verbose proptest-coverage proptest-continuous proptest-timing \
        stress-test stress-test-quick endurance-test test-all find-bugs-fast

# Быстрые property tests (100 случаев на тест)
proptest-quick: ## Быстрые property tests (100 случаев)
	PROPTEST_CASES=100 cargo test --test property_tests

# Обычные property tests (по умолчанию 1000 случаев)
proptest: ## Обычные property tests (по умолчанию)
	cargo test --test property_tests

# Длительное тестирование (10000 случаев)
proptest-long: ## Длительное property testing
	PROPTEST_CASES=10000 cargo test --test property_tests

# Подробный вывод для отладки
proptest-verbose: ## Подробный вывод для property tests
	PROPTEST_CASES=1000 RUST_LOG=debug cargo test --test property_tests -- --nocapture

# Запуск property tests с генерацией отчета о покрытии
proptest-coverage: ## Генерация покрытия для property tests (tarpaulin, HTML)
	cargo tarpaulin --tests --out Html --output-dir coverage/ --test property_tests

# Continuous property testing - запускать в фоне
proptest-continuous: ## Бесконечный цикл property tests (оставлять с осторожностью)
	while true; do \
		echo "Running property tests iteration $$(date)"; \
		PROPTEST_CASES=1000 cargo test --test property_tests || break; \
		sleep 60; \
	done

# Проверить что property tests проходят быстро (не более 30 сек как в Success Criteria)
proptest-timing: ## Измерение времени выполнения property tests
	time PROPTEST_CASES=1000 cargo test --test property_tests

# Запуск стресс-тестов (медленные, с большим количеством итераций)
stress-test: ## Запуск стресс-тестов (медленные, много итераций)
	PROPTEST_CASES=10000 cargo test --test stress_tests

# Быстрые стресс-тесты для CI
stress-test-quick: ## Быстрые стресс-тесты (короткие, для CI)
	PROPTEST_CASES=1000 cargo test --test stress_tests

# Эндуранс тест - найти memory leaks (очень медленный, только локально)
endurance-test: ## Эндуранс тест для поиска утечек памяти (медленный)
	cargo test --test stress_tests test_endurance_many_iterations --release -- --ignored --nocapture

# Полный набор тестов - property + stress + unit
test-all: ## Полный набор тестов (unit + property + stress)
	cargo test
	$(MAKE) proptest
	$(MAKE) stress-test-quick

# Найти баги быстро - краткий набор тестов с разными типами
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

##@ Help
help: ## Показать это сообщение
	@echo
	@echo "Zumic Makefile (version $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml))"
	@echo "Usage: make [target]"
	@echo
	@awk 'BEGIN {FS = ":.*##"; \
	  printf "%-20s %s\n", "Target", " Description"; \
	  printf "--------------------  -----------------------------\n"} \
	/^[a-zA-Z0-9_-]+:.*?##/ { printf " \033[36m%-20s\033[0m %s\n", $$1, $$2 } \
	/^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
