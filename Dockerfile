# syntax=docker/dockerfile:1

# ---- Build stage ----
# Pin to a specific Rust version for reproducible builds. bookworm matches the
# runtime image below so the dynamically-linked OpenSSL/glibc versions line up.
FROM rust:1-bookworm AS builder

WORKDIR /usr/src/github-app-auth

# Pre-fetch dependencies in a cached layer. Copying the manifests first means
# `cargo fetch` is only re-run when Cargo.toml / Cargo.lock change, not on every
# source edit.
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch

# Now copy the rest of the source. The .git directory is included so build.rs
# can run `git describe` and bake the real version into the binary. Copying it
# last keeps the cargo fetch layer above cache-stable across commits.
COPY build.rs ./
COPY src ./src
COPY .git ./.git

# Mark the repo as a safe directory: the copied .git is root-owned and git
# refuses to operate on a repo it considers owned by another user otherwise.
RUN git config --global --add safe.directory /usr/src/github-app-auth \
    && cargo build --release --locked --bin github-app \
    && strip target/release/github-app

# ---- Runtime stage ----
# Slim Debian base. reqwest's default TLS backend links against the system
# OpenSSL, so we install libssl3 and ca-certificates for HTTPS to api.github.com.
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Run as a non-root user.
RUN useradd --system --create-home --uid 10001 appuser
USER appuser

COPY --from=builder /usr/src/github-app-auth/target/release/github-app /usr/local/bin/github-app

ENTRYPOINT ["github-app"]
