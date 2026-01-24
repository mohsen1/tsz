# Development Guide

This guide covers setting up and working with the Project Zang codebase.

## Getting Started

```bash
# Clone the repository
git clone https://github.com/mohsen1/tsz.git
cd tsz

# Build the project (also installs git hooks automatically)
cargo build
```

## Git Hooks

Pre-commit hooks are automatically installed on first build. They run:
- `cargo fmt` - Format code
- `cargo clippy --fix` - Lint and auto-fix issues
- Unit tests via `scripts/test.sh`

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

```bash
# Run all unit tests
cargo test --lib

# Run tests for a specific module
cargo test --lib scanner
cargo test --lib parser
cargo test --lib binder
cargo test --lib checker
```

### Conformance Tests

Conformance tests compare Zang's output against the official TypeScript compiler.

```bash
cd conformance

# Run quick test (500 tests)
npm run test

# Run all conformance tests
npm run test:native:all

# Run with WASM
npm run test:wasm:500
```

See [TESTING.md](./TESTING.md) for more details on the testing infrastructure.

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
├── conformance/           # Conformance test runner
├── TypeScript/            # TypeScript submodule (tests source)
├── docs/                  # Documentation
└── scripts/               # Build and utility scripts
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
# Edit conformance/typescript-versions.json with the new SHA
```

## Tips

- Run `cargo clippy` before committing to catch common issues
- Use `cargo fmt` to auto-format code
- The conformance test cache speeds up repeated runs - generate it with `npm run cache:generate`
