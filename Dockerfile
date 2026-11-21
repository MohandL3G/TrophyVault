# syntax=docker/dockerfile:1

# ------------------------------------------------------------------------------
# Stage 1: Build trophyvault-server release binary
# ------------------------------------------------------------------------------
FROM rust:bookworm AS builder

WORKDIR /usr/src/trophyvault

# Copy workspace definition and member crates
COPY Cargo.toml Cargo.lock ./
COPY crates/core ./crates/core
COPY crates/server ./crates/server

# Exclude desktop crate from workspace members so server builds cleanly without GUI dependencies
RUN sed -i '/crates\/desktop/d' Cargo.toml && sed -i '/default-members/d' Cargo.toml

# Compile release binary for trophyvault-server
RUN cargo build --release -p trophyvault-server

# ------------------------------------------------------------------------------
# Stage 2: Minimal runtime environment
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runner

# Install CA certificates for outgoing HTTPS requests
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary from builder stage
COPY --from=builder /usr/src/trophyvault/target/release/trophyvault-server /usr/local/bin/trophyvault-server

# Ensure data and static directories exist
RUN mkdir -p /data /app/static

# Copy static web assets
COPY crates/server/static /app/static

# Expose web server port
EXPOSE 5050

# Default environment variables
ENV PORT=5050 \
    DATABASE_PATH=/data/trophyvault.db

ENTRYPOINT ["/usr/local/bin/trophyvault-server"]
