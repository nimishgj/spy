.PHONY: ci build test fmt fmt-check clippy clean help

# Default target prints help.
help:
	@echo "Available targets:"
	@echo "  make ci         - full CI suite (build + test + fmt-check + clippy)"
	@echo "  make build      - cargo build --workspace --all-targets"
	@echo "  make test       - cargo test --workspace"
	@echo "  make fmt        - cargo fmt --all (rewrite)"
	@echo "  make fmt-check  - cargo fmt --all -- --check"
	@echo "  make clippy     - cargo clippy --workspace --all-targets -- -D warnings"
	@echo "  make clean      - cargo clean"

ci: build test fmt-check clippy
	@echo "CI passed"

build:
	cargo build --workspace --all-targets --locked

test:
	cargo test --workspace --locked

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --locked -- -D warnings

clean:
	cargo clean
