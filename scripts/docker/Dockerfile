# syntax=docker/dockerfile:1.4
FROM rust:latest

WORKDIR /app

# Install cargo-nextest for faster test execution (40-60% faster than cargo test)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-nextest --locked

# Copy source code
COPY . .

# Build with cached dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --tests && \
    cp -r /app/target/debug/deps /app/deps-cache || true

# Default command to run tests with nextest
CMD ["cargo", "nextest", "run"]
