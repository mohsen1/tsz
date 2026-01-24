# Justfile for tsz - fast development workflows
#
# Install: cargo install just
# Run: just <command>

# Run tests with cargo-nextest (fast, smart test runner)
test:
    cargo nextest run

# Watch mode with bacon - automatically reruns tests on file changes
watch:
    bacon

# Quick compilation check without running tests
check:
    cargo check --tests

# Run clippy for lints
lint:
    cargo clippy --all-targets --all-features

# Format code
fmt:
    cargo fmt

# Run full test suite with benchmarks
test-all:
    cargo nextest run --all-targets
    cargo test --benches

# Run only library unit tests (skip integration tests)
test-lib:
    cargo nextest run --lib

# Run only integration tests
test-integration:
    cargo nextest run --test-threads=1

# Development build (optimized for fast iteration)
build-dev:
    cargo build --profile orchestrator

# Clean build artifacts
clean:
    cargo clean

# Update dependencies
update:
    cargo update

# Run tests for changed files since last commit (similar to pre-commit hook)
test-changed:
    #!/usr/bin/env bash
    CHANGED_FILES=$(git diff --cached --name-only --diff-filter=ACM | grep '\.rs$' || true)
    if [ -n "$CHANGED_FILES" ]; then
        echo "Testing changed files:"
        echo "$CHANGED_FILES" | sed 's/^/  - /'
        echo ""
        cargo nextest run
    else
        echo "No Rust files staged, running all tests..."
        cargo nextest run
    fi

# Install development tools
install-tools:
    cargo install cargo-nextest
    cargo install bacon
    cargo install just
