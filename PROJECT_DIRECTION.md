# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

---

## Priority List (Current Focus)

**Stop all feature work until these are addressed:**

### 1. Remove Test-Aware Code from Source

The checker has ~40 places that check file names to suppress errors for tests. This is architectural debt that must be removed.

**What to remove from `src/thin_checker.rs`:**
```rust
// BAD - This pattern appears 40+ times and must be removed:
let is_test_file = self.ctx.file_name.contains("conformance")
    || self.ctx.file_name.contains("test")
    || self.ctx.file_name.contains("cases");

if is_test_file && self.ctx.file_name.contains("Symbol") {
    return; // Suppressing errors for tests
}
```

**The rule:** Source code must not know about tests. If a test fails, fix the underlying logic, not by adding special cases for test file names.

### 2. Fix Testing Infrastructure

Before fixing more checker/parser bugs, the test infrastructure needs to:
- Parse `@` directives from test files (like `@strict`, `@noImplicitAny`)
- Configure the checker environment based on those directives
- Match how TypeScript's own test runner works

### 3. Architecture Review

Review and clean up before adding features:
- Remove all `file_name.contains()` checks in checker
- Ensure parser and checker are test-agnostic
- Document actual architectural decisions in specs/

### 4. Use ts-tests Properly

The `ts-tests/` directory should work like TypeScript's test suite:
- Test infrastructure reads directives from test files
- Configures compiler options accordingly
- Compares output to baselines

### 5. Study Results, Then Plan

After cleanup, analyze test results to identify:
- Root causes of failures (not symptoms)
- Patterns in missing/extra errors
- Priority order for fixes

### 6. Update agents.md

Enforce these rules in agent instructions to prevent regression.

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

---

## Commands

```bash
# Build WASM
wasm-pack build --target web --out-dir pkg

# Quick conformance test
./differential-test/run-conformance.sh --max=200 --workers=4

# Full conformance test
./differential-test/run-conformance.sh --all --workers=14
```

---

## Key Files

| File | Purpose |
|------|---------|
| `src/thin_parser.rs` | Parser |
| `src/thin_checker.rs` | Type checker (needs cleanup) |
| `src/binder/` | Symbol binding |
| `src/solver/` | Type resolution |
| `src/checker/types/diagnostics.rs` | Error codes |
| `differential-test/` | Test infrastructure |
| `specs/` | Architecture docs |

---

## Test Results

| Metric | Meaning |
|--------|---------|
| Exact Match | Identical errors to TSC |
| Missing Errors | TSC emits, we don't |
| Extra Errors | We emit, TSC doesn't |


## Project Goals

**Target:** 95%+ exact match with TypeScript compiler on conformance tests, with clean architecture and maintainable codebase.
