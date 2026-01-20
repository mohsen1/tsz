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
**Speed**: Fast (~10s)

```bash
./scripts/test.sh                    # All tests
cargo test --lib parser::            # Specific module
cargo test --lib solver::            # Solver tests
```

### 📊 Conformance Tests  
**Location**: `./conformance/`  
**Purpose**: Compare WASM output against TypeScript compiler  
**Speed**: ~70 tests/sec

⚠️ **Always run in Docker** - Tests can cause infinite loops or OOM.

```bash
# Quick iteration (500 tests, ~7s)
./conformance/run-conformance.sh --max=500

# Medium suite (2000 tests, ~30s)
./conformance/run-conformance.sh --max=2000

# Full suite (12K+ tests, ~3 mins)
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

## Workflow for Developers

### 🚀 When Starting Work
```bash
# 1. Make sure everything builds
cargo build --release

# 2. Run unit tests
./scripts/test.sh

# 3. Get baseline conformance
./conformance/run-conformance.sh --max=500
```

### 🔧 During Development  
```bash
# Test specific file you're working on
node scripts/run-single-test.mjs TypeScript/tests/cases/compiler/yourTest.ts

# Quick conformance check
./conformance/run-conformance.sh --max=500
```

### ✅ Before Committing
```bash
# 1. All Rust tests pass
./scripts/test.sh

# 2. Conformance hasn't regressed
./conformance/run-conformance.sh --max=2000

# 3. If working on parser/checker, run full suite
./conformance/run-conformance.sh --all
```

## Understanding Conformance Metrics

The conformance test runner outputs:

```
Pass Rate: 30.0% (150/500)
Time: 7.4s (68 tests/sec)

Summary:
  ✓ Passed:   150
  ✗ Failed:   350
  💥 Crashed:  0
  💾 OOM:      0
  ⏱ Timeout:  0
```

### Key Metrics
- **Pass Rate**: % of tests with identical error codes (target: 50%+)
- **Crashes**: Tests that caused WASM exceptions (target: 0)
- **OOM**: Tests that ran out of memory (target: 0)
- **Timeout**: Tests that took >10s (target: 0)

### Error Analysis
The runner shows top missing and extra errors:

```
Top Missing Errors (we should emit but don't):
  TS2318: 696x  - Cannot find global type (@noLib tests)
  TS2583: 298x  - Cannot find name (ES2015+ lib)
  TS2304: 59x   - Cannot find name

Top Extra Errors (we emit but shouldn't):
  TS2300: 60x   - Duplicate identifier
  TS1005: 58x   - Expected token (parser)
  TS2339: 34x   - Property does not exist
```

## Directory Structure

```
scripts/
├── test.sh                     # Main Rust test runner
├── bench.sh                    # Benchmark runner
├── build-wasm.sh               # WASM build script
├── docker/                     # Docker configuration
│   ├── Dockerfile              # Main Dockerfile
│   └── Dockerfile.bench        # Benchmark Dockerfile
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

- Use `--max=500` for quick feedback during development (~7s)
- Use `--max=2000` for thorough testing before commits (~30s)
- Use `--all` for comprehensive testing before major changes (~3 mins)
- Always run conformance tests in Docker to prevent system hangs
- Focus on reducing the top extra errors (we emit but shouldn't)
- Run `./scripts/test.sh` before every commit
