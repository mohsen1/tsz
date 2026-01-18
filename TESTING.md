# Testing Guide for Project Zang

This guide covers the testing infrastructure for the Rust/WASM TypeScript compiler implementation.

## Quick Start

```bash
# 1. Run Rust unit tests
./wasm/test.sh

# 2. Run conformance tests (compare against TypeScript)
./wasm/conformance/run-conformance.sh --max=1000

# 3. Test a specific file
node wasm/scripts/run-single-test.mjs tests/cases/compiler/2dArrays.ts
```

## Test Types

### 🦀 Rust Unit Tests
**Location**: `./wasm/test.sh`  
**Purpose**: Test core compiler logic (parsing, binding, type checking)  
**Speed**: Fast (~10s)

```bash
./wasm/test.sh                    # All tests
./wasm/test.sh parser_tests       # Specific module
./wasm/test.sh --bench            # Performance benchmarks
```

### 📊 Conformance Tests  
**Location**: `./wasm/conformance/`  
**Purpose**: Compare WASM output against TypeScript compiler  
**Speed**: Medium-Slow (1-15 mins depending on scope)

```bash
# Quick iteration (1000 tests, ~1 min)
./wasm/conformance/run-conformance.sh --max=1000

# Full suite (45K tests, ~15 mins) 
./wasm/conformance/run-conformance.sh --all

# Specific categories
./wasm/conformance/run-conformance.sh --category=compiler
./wasm/conformance/run-conformance.sh --category=conformance
```

### 🔍 Individual Test Scripts
**Location**: `./wasm/scripts/`  
**Purpose**: Debug specific issues and compare detailed output

```bash
# Test single file with verbose output
node wasm/scripts/run-single-test.mjs tests/cases/compiler/arrayLiterals.ts --verbose

# Compare baselines for first N tests
node wasm/scripts/compare-baselines.mjs 50 compiler

# Validate WASM module
node wasm/scripts/validate-wasm.mjs
```

## Workflow for Developers

### 🚀 When Starting Work
```bash
# 1. Make sure everything builds
./wasm/test.sh

# 2. Get baseline conformance
./wasm/conformance/run-conformance.sh --max=1000
```

### 🔧 During Development  
```bash
# Test specific areas you're working on
node wasm/scripts/run-single-test.mjs tests/cases/compiler/yourTest.ts --verbose

# Quick conformance check
./wasm/conformance/run-conformance.sh --max=500
```

### ✅ Before Committing
```bash
# 1. All Rust tests pass
./wasm/test.sh

# 2. Conformance hasn't regressed
./wasm/conformance/run-conformance.sh --max=2000

# 3. If working on parser/checker, run full suite
./wasm/conformance/run-conformance.sh --all
```

## Understanding Conformance Metrics

The conformance test outputs several key metrics:

- **Exact Match**: % of tests with identical output (target: 50%+)
- **Missing Errors**: % where WASM accepts but TypeScript rejects (target: <30%)  
- **Extra Errors**: % where WASM rejects but TypeScript accepts (target: <20%)
- **Parse Errors**: Absolute count of parse failures (target: <100)

### Current Priority Areas

Focus testing on these high-impact areas:

1. **TS2454 (Used before assigned)**: 573 missing errors
   ```bash
   node wasm/conformance/find-ts2454.mjs
   ```

2. **TS2322 (Type not assignable)**: 310 missing errors  
   ```bash
   node wasm/conformance/find-ts2322.mjs
   ```

3. **TS2339 (Property doesn't exist)**: 292 extra errors
   ```bash  
   node wasm/conformance/find-ts2339.mjs
   ```

## Performance Testing

```bash
# Benchmark parsing speed
./wasm/test.sh --bench

# Profile specific test
node --prof wasm/scripts/run-single-test.mjs tests/cases/compiler/largeFile.ts
```

## Debugging Failed Tests

1. **Individual test fails**:
   ```bash
   node wasm/scripts/run-single-test.mjs path/to/test.ts --verbose
   ```

2. **Conformance regression**:
   ```bash  
   # Find new failures
   ./wasm/conformance/run-conformance.sh --max=1000 | grep "FAIL"
   
   # Debug specific error type
   node wasm/conformance/find-ts2322.mjs
   ```

3. **Parser issues**:
   ```bash
   # Check if it's a parsing problem
   node wasm/scripts/run-single-test.mjs path/to/test.ts --thin
   ```

## Directory Structure

```
wasm/
├── test.sh                     # Main Rust test runner
├── scripts/                    # Individual test tools
│   ├── run-single-test.mjs     # Test one file
│   ├── compare-baselines.mjs   # Compare against baselines  
│   ├── run-batch-tests.mjs     # Run multiple tests
│   └── validate-wasm.mjs       # WASM module validation
├── conformance/          # Conformance test suite
│   ├── run-conformance.sh      # Main conformance runner
│   ├── find-ts2454.mjs         # Find specific error types
│   └── ...                     # Error-specific analyzers
└── dev-tests/                  # Development test files
    ├── test_debug.js           # General debugging
    └── ...                     # Specific test cases
```

## Tips

- Use `--max=1000` for quick feedback during development
- Use `--all` for comprehensive testing before major commits  
- Focus on the error types that have highest counts in conformance report
- Test both `--thin` and `--legacy` parser modes for completeness
- Run benchmarks periodically to catch performance regressions