# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

---

## Current State (January 2025)

| Metric | Value |
|--------|-------|
| Lines of Rust | ~200,000 |
| Unit Tests | ~10,420 |
| Ignored Tests | 6 (infinite loops) |
| Test-Aware Patterns | **0 in thin_checker.rs** (GOAL ACHIEVED) |

**Build Status:** Passes
**Test Status:** Most pass, some failures, some hanging tests marked with `#[ignore]`

> **Note:** `src/lib.rs` has 5 `file_name.contains` patterns for TypeScript library file detection (lib.d.ts, lib.es*, lib.dom*, etc.). These are NOT test-aware code - they are legitimate runtime configuration for loading standard library type definitions.

---

## Priority List (Current Focus)

### 1. Fix Hanging Tests

Several tests have infinite loops and hang forever. These must be identified and marked with `#[ignore]` before any other work.

**Currently Ignored (infinite loops):**
- `test_class_es5_commonjs_class_exports` (transforms/class_es5_tests.rs)
- `test_source_map_decorator_combined_advanced` (source_map_tests.rs)
- `test_source_map_decorator_composition_es5_comprehensive` (source_map_tests.rs)
- `test_source_map_decorator_composition_es5_method_params` (source_map_tests.rs)
- `test_source_map_decorator_metadata_es5_parameter_decorators` (source_map_tests.rs)

**Action:** Run tests with timeouts to find any remaining hanging tests.

### 2. Remove Test-Aware Code from Checker

~~The checker has **24 places** that check file names to suppress errors for tests.~~ **COMPLETED**

> **Status: RESOLVED**
> All `file_name.contains` patterns have been removed from `src/thin_checker.rs`.
> - Active code: Replaced with AST-based detection using parent traversal
> - Dead code: Removed entirely by cleaning up disabled `check_unused_declarations()`
>
> Verify: `grep -c 'file_name\.contains' src/thin_checker.rs` (should return 0)

**The rule:** Source code must not know about tests. If a test fails, fix the underlying logic.

### 3. Fix Test Infrastructure

Before fixing more checker/parser bugs, the test infrastructure needs to:
- Parse `@` directives from test files (like `@strict`, `@noImplicitAny`)
- Configure the checker environment based on those directives
- Match how TypeScript's own test runner works

### 4. Clean Up Warnings

Run `cargo clippy` and fix warnings. Many unused imports and dead code exist.

### 5. Run Conformance Tests

After cleanup, run the conformance test suite:
```bash
./differential-test/run-conformance.sh --max=200 --workers=4
```

Analyze results to identify root causes of failures.

---

## Merge Criteria

**Before merging any branch:**
1. `cargo build` must pass with no errors
2. `cargo test` must pass with no failures
3. Tests must run fast (< 30 seconds for full test suite)
4. Individual tests must complete in < 5 seconds (mark slow tests with `#[ignore]`)

---

## Rules

### Never Break The Build
- All commits must pass unit tests
- No change should reduce conformance accuracy

### Keep Architecture Clean
- No shortcuts
- No test-aware code in source
- Fix root causes, not symptoms
- No whack-a-mole error suppression

### Anti-Patterns to Avoid

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |
| Add "Tier 0/1/2" patches | Fix root cause once |
| Add filtering for test patterns | Make checker correct for all code |
| Create infinite loops in transforms | Add recursion limits |

---

## Commands

```bash
# Build
cargo build

# Run all tests
cargo test --lib

# Run specific test module
cargo test --lib solver::

# Build WASM
wasm-pack build --target web --out-dir pkg

# Quick conformance test
./differential-test/run-conformance.sh --max=200 --workers=4

# Full conformance test
./differential-test/run-conformance.sh --all --workers=14
```

---

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/thin_checker.rs` | Type checker (needs cleanup) | 24,564 |
| `src/thin_parser.rs` | Parser | 11,068 |
| `src/binder.rs` | Symbol binding | 2,108 |
| `src/solver/` | Type resolution (39 files) | ~15,000 |
| `src/transforms/` | ES5 downlevel transforms | ~10,000 |
| `differential-test/` | Conformance test infrastructure | - |
| `AGENTS.md` | Architecture rules for AI agents | - |

---

## Test Results

| Metric | Meaning |
|--------|---------|
| Exact Match | Identical errors to TSC |
| Missing Errors | TSC emits, we don't |
| Extra Errors | We emit, TSC doesn't |

---

## Project Goals

**Target:** 95%+ exact match with TypeScript compiler on conformance tests, with clean architecture and maintainable codebase.

**Non-Goals:**
- 100% compatibility (edge cases acceptable)
- Supporting deprecated features
- Matching TSC performance exactly (we aim for better)
