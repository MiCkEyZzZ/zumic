##@ Build
.PHONY: build release
build: ## Сборка debug
	cargo build
release: ## Сборка release
	cargo build --release

##@ Test
.PHONY: check clippy nextest test
check: ## Cargo check
	cargo check
clippy: ## Clippy
	cargo clippy -- -D warnings
nextest: ## Nextest
	cargo nextest run
test: ## Cargo test
	cargo test

##@ Format & Lints
.PHONY: fmt fmt-toml fmt-all check-toml check-all
fmt: ## Rust fmt
	cargo fmt --all
fmt-toml: ## TOML fmt
	taplo format
fmt-all: ## Все форматы
	$(MAKE) fmt && $(MAKE) fmt-toml
check-toml: ## TOML check
	taplo format --check
check-all: ## Полная проверка
	$(MAKE) check && $(MAKE) clippy && $(MAKE) fmt-check && $(MAKE) check-toml

##@ Bench & Fuzz
.PHONY: bench fuzz
bench: ## Бенчмарки
	cargo bench
fuzz: ## Fuzz tests
	cargo fuzz run

##@ Misc
.PHONY: clean help
clean: ## Очистка
	cargo clean
help: ##@ Display this help
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} \
	  /^[a-zA-Z0-9_-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } \
	  /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)
