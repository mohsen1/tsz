# syntax=docker/dockerfile:1.4
FROM rust:latest

WORKDIR /app

# Install cargo-nextest for faster test execution (40-60% faster than cargo test)
RUN cargo install cargo-nextest --locked

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies (cached layer)
RUN mkdir -p src && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    cargo build --tests 2>/dev/null || true && \
    rm -rf src

# Copy actual source code
COPY . .

# Default command to run tests with nextest
CMD ["cargo", "nextest", "run"]
