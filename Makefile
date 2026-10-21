.DEFAULT_GOAL := help

.PHONY: help
help: ## Show this help message
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

.PHONY: fmt
fmt: ## Format Markdown with prettier
	npx --yes prettier@3.9.6 --write "**/*.md"

.PHONY: fmt-check
fmt-check: ## Check Markdown formatting
	npx --yes prettier@3.9.6 --check "**/*.md"

.PHONY: lint
lint: ## Run cargo fmt and clippy
	cargo fmt --check
	cargo clippy --all-targets --locked -- -D warnings

.PHONY: test
test: ## Run the test suite
	sh tests/release-version-check.sh
	cargo test --locked

.PHONY: release-check
release-check: ## Verify TAG matches Cargo.toml and herdr-plugin.toml before tagging
	@test -n "$(TAG)" || { echo "usage: make release-check TAG=v<version>"; exit 2; }
	sh scripts/check-release-version.sh "$(TAG)"

.PHONY: build
build: ## Build the release binary
	cargo build --release --locked

.PHONY: check
check: lint fmt-check test build ## Run all checks (lint, fmt-check, test, build)
