# Testing Guide for Project Zang

This guide covers the testing infrastructure for the Rust/WASM TypeScript compiler implementation.

## Quick Start

```bash
# 1. Run Rust unit tests
./scripts/test.sh

# 2. Run conformance tests (compare against TypeScript)
./conformance/run-conformance.sh --max=500

# 3. Test a specific file
node scripts/run-single-test.mjs TypeScript/tests/cases/compiler/2dArrays.ts
```

## Test Types

### 🦀 Rust Unit Tests
**Location**: `./scripts/test.sh`  
**Purpose**: Test core compiler logic (parsing, binding, type checking)

```bash
./scripts/test.sh                    # All tests
cargo test --lib parser::            # Specific module
cargo test --lib solver::            # Solver tests
```

#### Using nextest (Recommended)

For better timeout handling and parallel execution, use [cargo-nextest](https://nexte.st/):

```bash
# Install nextest
cargo install cargo-nextest

# Run all tests with timeouts (30s default)
cargo nextest run

# Quick profile (10s timeout, fail-fast)
cargo nextest run --profile quick

# CI profile (60s timeout)
cargo nextest run --profile ci

# Run specific tests
cargo nextest run solver::
cargo nextest run test_conditional_infer
```

Configuration is in `.cargo/nextest.toml`.

### 📊 Conformance Tests  
**Location**: `./conformance/`  
**Purpose**: Compare WASM output against TypeScript compiler

⚠️ **Always run in Docker** - Tests can cause infinite loops or OOM.

```bash
# Quick iteration (500 tests)
./conformance/run-conformance.sh --max=500

# Medium suite (2000 tests)
./conformance/run-conformance.sh --max=2000

# Full suite (12K+ tests)
./conformance/run-conformance.sh --all

# Verbose output (shows individual failures)
./conformance/run-conformance.sh --max=100 --verbose

# Specific category
./conformance/run-conformance.sh --category=compiler
./conformance/run-conformance.sh --category=conformance
```

### 🔍 Single Test Script
**Location**: `./scripts/run-single-test.mjs`  
**Purpose**: Debug specific test files

```bash
# Test single file
node scripts/run-single-test.mjs TypeScript/tests/cases/compiler/arrayLiterals.ts

# Validate WASM module
node scripts/validate-wasm.mjs
```

## Workflow

### 🚀 When Starting Work
```bash
cargo build --release
./scripts/test.sh
./conformance/run-conformance.sh --max=500
```

### 🔧 During Development  
```bash
node scripts/run-single-test.mjs TypeScript/tests/cases/compiler/yourTest.ts
./conformance/run-conformance.sh --max=500
```

### ✅ Before Committing
```bash
./scripts/test.sh
./conformance/run-conformance.sh --max=2000
```

## Directory Structure

```
scripts/
├── test.sh                     # Main Rust test runner
├── bench.sh                    # Benchmark runner
├── build-wasm.sh               # WASM build script
├── docker/                     # Docker configuration
├── run-single-test.mjs         # Test one file
├── validate-wasm.mjs           # WASM module validation
└── help.mjs                    # Help/usage info

conformance/                    # Conformance test suite
├── run-conformance.sh          # Main runner (Docker-based)
├── src/
│   ├── runner.ts               # Test orchestrator
│   ├── worker.ts               # Parallel worker
│   └── baseline.ts             # Baseline comparison
└── package.json                # Node dependencies
```

## Tips

- Use `--max=500` for quick feedback during development
- Use `--max=2000` for thorough testing before commits
- Use `--all` for comprehensive testing before major changes
- Always run conformance tests in Docker to prevent system hangs
- Run `./scripts/test.sh` before every commit
