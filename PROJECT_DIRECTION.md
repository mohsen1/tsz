# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Test Status

**Note**: Conformance tests use **WASM build** (`pkg/`), not native binary. Build first:
```bash
wasm-pack build --target nodejs --out-dir pkg
./conformance/run-conformance.sh --all
```

## Gaps / Risks

* **Conformance gap**: Target is 50%+. Run tests to get current metrics. Latest full run (12,053 tests): **30.7% pass rate**.
* **WASM-specific issues**: 121 worker crashes during test run - may indicate memory/stability issues in WASM build.

* **Transform pipeline debt**: `src/transforms/` mixes AST manipulation with string emission. Should produce lowered AST, then printer emits strings.

* **Compat layer completeness**: `compat` module needs audit against `TS_UNSOUNDNESS_CATALOG.md` to ensure all rules (weak types, template literal limits, rest bivariance, exactOptionalPropertyTypes) are wired and option-driven.

***

## Priority List

### 1. Unblock Testing

Fix WASM initialization crash path to enable conformance runs. Unit tests require Docker (`./scripts/test.sh` - enforced at compile time).

### 2. Improve Conformance Test Pass Rate

**Target: 50%+**

Run tests to get current metrics and identify top issues:
```bash
./conformance/run-conformance.sh --all
```

Focus areas based on latest run:
- Library loading (TS2318: ~8K missing, TS2583: ~1.4K missing)
- Type assignability (TS2322: ~12K extra - we're too strict)
- Name/module resolution (TS2304, TS2307)
- Duplicate identifier handling (TS2300: ~1.5K extra)

### 3. Fix Transform Pipeline

`src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

### 4. Compat Layer Audit

Audit `compat` module against `TS_UNSOUNDNESS_CATALOG.md` to ensure all rules are wired and option-driven (weak types, template literal limits, rest bivariance, exactOptionalPropertyTypes).

### 5. Code Hygiene

* Remove `#![allow(dead_code)]` and fix unused code
* Add proper tracing infrastructure (replace print statements)
* Clean up Clippy ignores in `clippy.toml`
* Test-awareness cleanup: sweep checker/binder for path heuristics or test-specific workarounds (per AGENTS rules)

***

## Key Files

| File/Directory | Purpose | Lines |
|----------------|---------|-------|
| `src/checker/` | Type checker | ~44,000 total |
| `src/parser/state.rs` | Parser implementation | 10,770 |
| `src/parser/node.rs` | AST node definitions | ~5,500 |
| `src/binder.rs` | Symbol binding | 587 |
| `src/solver/` | Type resolution | 37 files |
| `src/transforms/` | JavaScript transforms | ~850K total (inc. tests) |

***

## Commands

```bash
cargo build                              # Build
cargo test --lib                         # Run all tests
cargo test --lib solver::                # Run specific module
wasm-pack build --target nodejs          # Build WASM
./conformance/run-conformance.sh --max=500   # Run 500 conformance tests
./conformance/run-conformance.sh --all       # Run all conformance tests
```

***

## Rules

* All commits must pass unit tests
* No test-aware code in source
* Fix root causes, not symptoms

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |

***

## Merge Criteria

1. `cargo build` passes
2. `cargo test` passes
3. Tests run in < 30 seconds
