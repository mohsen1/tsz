# Development Guide

This guide covers setting up and working with the Project Zang codebase.

## Getting Started

```bash
# Clone the repository
git clone https://github.com/mohsen1/tsz.git
cd tsz

# Initialize the TypeScript submodule (required for conformance tests)
git submodule update --init TypeScript

# Build the project (also installs git hooks automatically)
cargo build
```

## Git Hooks

Pre-commit hooks are automatically installed on first build. They run:
- `cargo fmt` - Format code
- `cargo clippy --fix` - Lint and auto-fix issues
- Unit tests via `cargo nextest run`

To manually install hooks:
```bash
./scripts/install-hooks.sh
```

To skip hooks for a single commit (not recommended):
```bash
git commit --no-verify
```

## Running Tests

### Unit Tests

We use [cargo-nextest](https://nexte.st/) for all test runs. It provides timeout protection,
per-test isolation, and better output than `cargo test`.

```bash
# Install nextest (one time)
cargo install cargo-nextest

# Run all unit tests
cargo nextest run

# Run tests for a specific module
cargo nextest run -E 'test(/scanner/)'
cargo nextest run -E 'test(/parser/)'
cargo nextest run -E 'test(/binder/)'
cargo nextest run -E 'test(/checker/)'

# Quick mode (fail-fast, 10s timeout)
cargo nextest run --profile quick

# Run ignored tests
cargo nextest run --run-ignored all
```

Nextest profiles are configured in `.config/nextest.toml` with protection against:
- **Hanging tests**: Auto-terminate after timeout periods
- **Leaked threads**: Detect tests that don't terminate cleanly
- **Slow tests**: Warn and kill tests exceeding time limits

### Conformance Tests

Conformance tests compare Zang's output against the official TypeScript compiler.

```bash
# Run conformance tests (server mode, fast)
./scripts/conformance/run.sh --server --max=500

# Run all conformance tests
./scripts/conformance/run.sh --all

# Run with WASM
./scripts/conformance/run.sh --wasm --max=500
```

## Building

### Native Binary

```bash
# Debug build
cargo build

# Release build
cargo build --release

# The binary is at target/release/tsz
```

### WASM Build

```bash
# Install wasm-pack if needed
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build WASM package
wasm-pack build --target web --out-dir pkg
```

## Project Structure

```
tsz/
├── src/                    # Rust source code
│   ├── scanner/           # Lexer/tokenizer
│   ├── parser/            # Parser
│   ├── binder/            # Name binding
│   ├── checker/           # Type checker
│   ├── solver/            # Type constraint solver
│   └── ...
├── TypeScript/            # TypeScript submodule (tests source)
├── docs/                  # Documentation
└── scripts/               # Build, utility scripts, conformance & fourslash test runners
```

## Updating TypeScript Version

The TypeScript submodule is used for conformance tests:

```bash
# Update to latest TypeScript
cd TypeScript
git fetch origin main
git checkout origin/main
cd ..
git add TypeScript

# Update the version mapping
# Edit scripts/conformance/typescript-versions.json with the new SHA
```

## Tips

- Run `cargo clippy` before committing to catch common issues
- Use `cargo fmt` to auto-format code
- The conformance test cache speeds up repeated runs - generate it with `npm run cache:generate`
