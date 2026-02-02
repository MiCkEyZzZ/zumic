# The Zumic Makefile
#
# A set of convenient commands for Zumic development.
# Main features:
#  - Build debug/release:    make build / make build-release
#  - Run:                    make run / make run-release
#  - Tests и property tests: make test / make proptest / make stress-test
#  - Formating/lints:        make fmt / make clippy / make check-all
#  - Fuzz и benchmark:       make fuzz / make bench
#  - Gir/release managment:  make git-tag / make git-release
#
# Variables:
#  BUILD_TARGET  - reading from .cargo/config.toml (target triple)
#  TARGET_ARG    - automatically generated for cargo (--target ...)
#  TARGET_DIR    - path in target/ for the selected target'а
#  VERSION       - automatically taken from Cargo.toml (used in release-auto)
#  ZUMIC_BANNER  - control the banner when running

BUILD_TARGET := $(shell test -f .cargo/config.toml && grep -E '^\s*target\s*=' .cargo/config.toml | head -1 | cut -d'"' -f2)
TARGET_ARG   := $(if $(BUILD_TARGET),--target $(BUILD_TARGET),)
TARGET_DIR   := target/$(if $(BUILD_TARGET),$(BUILD_TARGET)/,)
VERSION      := v$(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)

##@ Build
.PHONY: build build-release build-all-platforms

build: ## Build debug version
	cargo build $(TARGET_ARG)

build-release: ## Build release version
	cargo build --release $(TARGET_ARG)

build-all-platforms: ## Build for all platforms (as in CI)
	@echo "Building for multiple platforms..."
	cargo build --release --target x86_64-unknown-linux-gnu
	cargo build --release --target aarch64-unknown-linux-gnu
	cargo build --release --target x86_64-apple-darwin
	cargo build --release --target x86_64-pc-windows-msvc

##@ Test
.PHONY: check clippy clippy-ci nextest test miri miri-test test-ci

check: ## Cargo check
	cargo check

clippy: ## Clippy (treat warning as errors)
	cargo clippy -- -D warnings

clippy-ci: ## Clippy as in CI: all targets and all features, warnings -> error
	cargo clippy --all-targets --all-features -- -D warnings

nextest: ## Nextest
	cargo nextest run

test: ## Cargo test (regular tests)
	cargo test

test-ci: ## Full test suite as in CI
	cargo fmt -- --check
	$(MAKE) clippy-ci
	$(MAKE) test-all

miri: ## Run all tests in Miri
	cargo miri test

miri-test: ## Run a specific test in Miri. Usage: make miri-test TEST="module::test_name"
	cargo miri test $(TEST)

##@ Format & Lints
.PHONY: fmt fmt-toml fmt-all check-toml check-all

fmt: ## Rust fmt
	cargo fmt --all

fmt-toml: ## TOML fmt
	taplo format

fmt-all: ## Formating everithing
	$(MAKE) fmt
	$(MAKE) fmt-toml

check-toml: ## Check TOML format
	taplo format --check

check-all: ## Full check (check + clippy + format + toml)
	$(MAKE) check
	$(MAKE) clippy
	$(MAKE) fmt
	$(MAKE) check-toml

##@ Bench & Fuzz
.PHONY: bench fuzz fuzz-target fuzz-long fuzz-quick fuzz-decode

bench: ## Benchmarks
	cargo bench

fuzz: ## Fuzz tests (2 minutes, decode_value)
	./scripts/run_fuzz.sh decode_value 2

fuzz-target: ## Run specific fuzz target. Usage: make fuzz-target TARGET=decode_value MINUTES=10
ifndef TARGET
	$(error TARGET is not set. Use make fuzz-target TARGET=decode_value MINUTES=10)
endif
	./scripts/run_fuzz.sh $(TARGET) $(MINUTES)

fuzz-long: ## Long fuzzing (60 minutes)
	./scripts/run_fuzz.sh decode_value 60

fuzz-quick: ## Quick fuzzing (1 minute)
	./scripts/run_fuzz.sh decode_value 1

fuzz-decode: ## Fuzzing decode_value (10 minutes)
	./scripts/run_fuzz.sh decode_value 10

fuzz-build: ## Build fuzz target without running
	cd fuzz && cargo +nightly fuzz build decode_value

fuzz-clean: ## Clean fuzz artifacts
	rm -rf fuzz/artifacts/*
	rm -rf fuzz/corpus/*
	rm -rf results/*

##@ Misc
.PHONY: clean clean-all

clean: ## Clean build artifacts
	cargo clean

clean-all: ## Full clean (including fuzz)
	cargo clean
	$(MAKE) fuzz-clean

##@ Git
.PHONY: git-add git-commit git-push git-status

git-add: ## Add all changes to index
	git add .

git-commit: ## Commit changes. Usage: make git-commit MSG="Your message"
ifndef MSG
	$(error MSG is not set. Use make git-commit MSG="your message")
endif
	git commit -m "$(MSG)"

git-push: ## Push commits to remote repository
	git push

git-status: ## Show repository status
	git status

##@ Git Release
.PHONY: git-tag git-push-tag git-release release-auto bump-version release-all prepare-release

git-tag: ## Create a git tag. Example: make git-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-tag VERSION=v0.2.0)
endif
	git tag -a $(VERSION) -m "Release $(VERSION)"

git-push-tag: ## Push tag to origin. Example: make git-push-tag VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-push-tag VERSION=v0.2.0)
endif
	git push origin $(VERSION)

prepare-release: ## Prepare release (update version, changelog). Usage: make prepare-release VERSION=v0.5.0
ifndef VERSION
	$(error VERSION is not set. Use make prepare-release VERSION=v0.5.0)
endif
	@if [ -f scripts/prepare-release.sh ]; then \
		./scripts/prepare-release.sh $(VERSION); \
	else \
		echo "Error: scripts/prepare-release.sh not found"; \
		exit 1; \
	fi

git-release: ## Full release: tag + push. Example: make git-release VERSION=v0.2.0
ifndef VERSION
	$(error VERSION is not set. Use make git-release VERSION=v0.2.0)
endif
	$(MAKE) git-tag VERSION=$(VERSION)
	$(MAKE) git-push-tag VERSION=$(VERSION)
	@echo "✅ Tag $(VERSION) pushed. GitHub Actions will create the release automatically."
	@echo "Monitor: https://github.com/MiCkEyZzZ/zumic/actions"

release-auto: ## Automatic release based on Cargo.toml version
	$(MAKE) git-release VERSION=$(VERSION)

bump-version: ## Bump patch version in Cargo.toml (cargo-edit)
	cargo set-version --bump patch
	git add Cargo.toml Cargo.lock
	git commit -m "chore: bump version to $(shell awk -F\" '/^version/ {print $$2}' Cargo.toml)"

release-all: ## Full release cycle: prepare-release + tests + push
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

##@ Property testing commands
.PHONY: proptest proptest-quick proptest-long proptest-verbose proptest-coverage proptest-continuous proptest-timing \
        proptest-zdb proptest-hll proptest-all \
        stress-test stress-test-quick endurance-test test-all find-bugs-fast

proptest-quick: ## Quick property tests (100 cases)
	PROPTEST_CASES=100 cargo test --test property_tests
	PROPTEST_CASES=100 cargo test --test hll_property_tests

proptest: ## Regular property tests (default)
	cargo test --test property_tests
	cargo test --test hll_property_tests

proptest-long: ## Long property testing
	PROPTEST_CASES=10000 cargo test --test property_tests
	PROPTEST_CASES=10000 cargo test --test hll_property_tests

proptest-verbose: ## Verbose output for property tests
	PROPTEST_CASES=1000 RUST_LOG=debug cargo test --test property_tests -- --nocapture
	PROPTEST_CASES=1000 RUST_LOG=debug cargo test --test hll_property_tests -- --nocapture

proptest-zdb: ## ZDB-only property tests
	cargo test --test property_tests

proptest-hll: ## HLL-only property tests
	cargo test --test hll_property_tests

proptest-hll-quick: ## Quick HLL property tests (100 cases)
	PROPTEST_CASES=100 cargo test --test hll_property_tests

proptest-hll-long: ## Long HLL property tests (10000 cases)
	PROPTEST_CASES=10000 cargo test --test hll_property_tests

proptest-hll-verbose: ## Verbose HLL property tests
	PROPTEST_CASES=1000 RUST_LOG=debug cargo test --test hll_property_tests -- --nocapture

proptest-all: ## All property tests (ZDB + HLL + others)
	$(MAKE) proptest-zdb
	$(MAKE) proptest-hll

proptest-coverage: ## Property test coverage generation (tarpaulin, HTML)
	cargo tarpaulin --tests --out Html --output-dir coverage/ --test property_tests --test hll_property_tests

proptest-continuous: ## Continuous property testing loop (use with caution)
	while true; do \
		echo "Running property tests iteration $$(date)"; \
		PROPTEST_CASES=1000 cargo test --test property_tests || break; \
		PROPTEST_CASES=1000 cargo test --test hll_property_tests || break; \
		sleep 60; \
	done

proptest-timing: ## Measure execution time of property tests
	@echo "==> ZDB property tests timing:"
	time PROPTEST_CASES=1000 cargo test --test property_tests
	@echo "==> HLL property tests timing:"
	time PROPTEST_CASES=1000 cargo test --test hll_property_tests

stress-test: ## Run stress tests (slow, many iterations)
	PROPTEST_CASES=10000 cargo test --test stress_tests

stress-test-quick: ## Quick stress tests (short, for CI)
	PROPTEST_CASES=1000 cargo test --test stress_tests

endurance-test: ## Endurance test for memory leaks (slow)
	cargo test --test stress_tests test_endurance_many_iterations --release -- --ignored --nocapture

test-all: ## Complete test suite (unit + integration + property + stress)
	@echo "==> Running unit tests..."
	cargo test --lib
	@echo "==> Running integration tests..."
	cargo test --test hll_integration_tests
	@echo "==> Running property tests (ZDB)..."
	$(MAKE) proptest-zdb
	@echo "==> Running property tests (HLL)..."
	$(MAKE) proptest-hll
	@echo "==> Running stress tests (quick)..."
	$(MAKE) stress-test-quick
	@echo "✅ All tests passed!"

find-bugs-fast: ## Minimal test suite to quickly find bugs
	PROPTEST_CASES=500 cargo test --test property_tests roundtrip_all_values
	PROPTEST_CASES=500 cargo test --test property_tests numeric_edge_cases
	PROPTEST_CASES=500 cargo test --test hll_property_tests add_idempotence
	PROPTEST_CASES=500 cargo test --test hll_property_tests merge_commutativity
	cargo test --test stress_tests test_compression_pathological_cases

##@ Run
.PHONY: run run-full run-compact run-release

run: ## Run Zumic in default mode (debug → full)
	cargo run

run-full: ## Run Zumic with full banner (force)
	ZUMIC_BANNER=full cargo run

run-compact: ## Run Zumic with compact banner (force)
	ZUMIC_BANNER=compact cargo run

run-release: ## Run Zumic release build
	cargo build --release $(TARGET_ARG) && ./$(TARGET_DIR)release/zumic

run-m: ## Run Zumic in memory mode
	RUST_ENV=memory cargo run --bin zumic

run-p: ## Run Zumic in persistent mode
	RUST_ENV=persistent cargo run --bin zumic

run-c: ## Run Zumic in cluster mode
	RUST_ENV=cluster cargo run --bin zumic

##@ CI/CD
.PHONY: ci-local simulate-ci

ci-local: ## Run CI checks locally
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

simulate-ci: ## Simulate full CI pipeline (slow)
	@echo "==> Simulating full CI pipeline..."
	$(MAKE) ci-local
	@echo "==> Building release"
	$(MAKE) build-release
	@echo "==> Fuzz test (quick)"
	$(MAKE) fuzz-quick
	@echo "✅ CI simulation complete!"

##@ Help
help: ## Show this help message
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
