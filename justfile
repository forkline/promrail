# Promrail Justfile
# Run `just --list` to see available commands

CARGO_TARGET_DIR := "target"
CARGO_TARGET := "x86_64-unknown-linux-gnu"
PROJECT_VERSION := `sed -n 's/^version = "\(.*\)"/\1/p' ./Cargo.toml | head -n1`
PKG_BASE_NAME := "promrail-" + PROJECT_VERSION + "-" + CARGO_TARGET

# Show available commands
default:
    just --list

# Compile promrail in release mode
build:
    cargo build --release

# Install pre-commit hooks
pre-commit-install:
    pre-commit install

# Run pre-commit on all files
pre-commit:
    pre-commit run --all-files

# Format Rust code
fmt:
    cargo fmt

# Check Rust code formatting
fmt-check:
    cargo fmt -- --check

# Run clippy linter
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run clippy with automatic fixes
clippy-fix:
    cargo clippy --all-targets --all-features --fix --allow-dirty -- -D warnings

# Run all linting checks (fmt + clippy)
lint: fmt-check clippy

# Run all linting with automatic fixes
lint-fix: fmt clippy-fix

# Run unit tests
test-unit:
    cargo test --lib

# Run integration tests
test-integration:
    cargo test --test '*'

# Run all tests (lint + unit + integration)
test: lint test-unit test-integration

# Run end-to-end tests
test-e2e:
    cargo test --test e2e -- --test-threads=1

# Run all tests including e2e
test-all: test test-e2e

# Automatically update changelog based on commits
update-changelog:
    git cliff -t v{{PROJECT_VERSION}} -u -p CHANGELOG.md

# Generate release artifacts
release:
    cargo build --release --all-features --target {{CARGO_TARGET}}
    tar -czf {{PKG_BASE_NAME}}.tar.gz -C {{CARGO_TARGET_DIR}}/{{CARGO_TARGET}}/release promrail
    @echo "Released in {{CARGO_TARGET_DIR}}/{{CARGO_TARGET}}/release/promrail"

# Clean build artifacts
clean:
    cargo clean
    rm -f *.tar.gz

# Show project info
info:
    @echo "Project: promrail"
    @echo "Version: {{PROJECT_VERSION}}"
    @echo "Target: {{CARGO_TARGET}}"
