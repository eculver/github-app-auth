# Makefile for github-app-auth
#
# This mirrors the Go project's Makefile targets but uses Cargo conventions.
#
# Key targets:
#   make           - Build the binary (default)
#   make test      - Run tests with linting
#   make cover     - Run tests and open HTML coverage report in browser
#   make docker    - Build the container image
#   make install   - Install the binary to ~/.cargo/bin/
#   make clean     - Remove build artifacts

.DEFAULT_GOAL := build

# ---------------------------------------------------------------------------
# Lint
# ---------------------------------------------------------------------------

.PHONY: lint
lint:
	@cargo clippy -- -D warnings

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------

# Run linter + all tests. Equivalent to `go vet ./... && go test -race ./...`
.PHONY: test
test: lint
	@cargo test

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

# Run tests with coverage and open the HTML report in the browser.
# Requires: cargo install cargo-llvm-cov
#
# This is equivalent to the Go project's `make cover` which runs:
#   go test -cover -coverprofile=coverage.out && go tool cover -html=coverage.out
.PHONY: cover
cover:
	@cargo llvm-cov --html
	@open target/llvm-cov/html/index.html

# Generate an LCOV coverage report (useful for CI integrations).
.PHONY: cover-lcov
cover-lcov:
	@cargo llvm-cov --lcov --output-path lcov.info

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

.PHONY: build
build:
	@cargo build --release

# ---------------------------------------------------------------------------
# Container image
# ---------------------------------------------------------------------------

# Image name/tag — override on the command line, e.g.:
#   make docker IMAGE=ghcr.io/me/github-app-auth TAG=v1.2.3
IMAGE ?= github-app-auth
TAG ?= latest

# Build the container image. build.rs runs `git describe` inside the build (the
# .git directory is shipped in the build context) so the image's `--version`
# matches a host build — no build arg needed.
.PHONY: docker
docker:
	@docker build -t $(IMAGE):$(TAG) .

# ---------------------------------------------------------------------------
# Install / Uninstall
# ---------------------------------------------------------------------------

# Install the binary to ~/.cargo/bin/ (which should be in your $PATH).
# This is the Rust equivalent of `make install` copying to $GOPATH/bin/.
.PHONY: install
install:
	@cargo install --path .

.PHONY: uninstall
uninstall:
	@cargo uninstall github-app-auth

# ---------------------------------------------------------------------------
# Clean
# ---------------------------------------------------------------------------

.PHONY: clean
clean:
	@cargo clean
	@rm -f lcov.info
