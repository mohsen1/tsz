# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (100 tests) | **53.5%** |
| Conformance (500 tests) | **40.8%** |
| Driver Tests | 113/113 passing |
| Crashes | 0 |

## Priority List (Current Focus)

### 1. Improve Conformance Test Pass Rate

**Target: 95%+**

#### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS1109 | 17 | Expression expected (parser) |
| TS2304 | 8 | Cannot find name |
| TS1359 | 8 | Identifier expected (parser) |
| TS2403 | 7 | Subsequent variable declarations must have same type |
| TS2345 | 6 | Argument type not assignable |
| TS2703 | 4 | Delete operand must be optional |

#### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2571 | 4 | Object is of type 'unknown' |
| TS2322 | 4 | Type not assignable |
| TS2349 | 2 | Cannot invoke expression |

#### Recent Fixes
- Fixed TS2454 to return ERROR type (prevents cascading TS2571)
- Implemented TS2435: nested ambient modules error
- Implemented TS1042: async getters/setters
- Implemented TS1202: import equals in ESM context
- Implemented TS2372: parameter cannot reference itself
- Implemented shorthand ambient modules (`declare module "x"` → `any`)
- Fixed TS2524 for 'await' in default parameter values
- Fixed Promise<T> extraction for await expressions (PROMISE_BASE)
- Fixed TS1040 for 'declare async function'
- Fixed await expressions to return original type when not Promise-like

### 2. Architecture Improvements

#### Flow Analysis (Critical)
`src/checker/flow_analyzer.rs`: `check_flow` is recursive. Deeply nested control flow will blow the stack. **Needs iterative worklist algorithm.**

#### Transform Pipeline
The transformation logic in `src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

#### Code Hygiene
- Remove `#![allow(dead_code)]` directives and fix/remove unused code
- Move all scripts to `scripts/` directory
- Consolidate Docker files to `scripts/docker/`
- Add proper tracing infrastructure (replace print statements)

### 3. Clean Up Clippy Ignores
Go through rules ignored in `clippy.toml` and fix underlying issues.

---

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/thin_checker.rs` | Type checker | 25,315 |
| `src/thin_parser.rs` | Parser | 10,704 |
| `src/thin_binder.rs` | Symbol binding | 4,452 |
| `src/solver/` | Type resolution | 37 files |
| `src/transforms/` | ES5 downlevel | ~10,000 |
| `conformance/` | Test infrastructure | - |

---

## Architecture Notes

### Memory Architecture (Resolved)
- **Shared source text**: `Arc<str>` used throughout (no full-file clones)
- **Single AST**: Fat AST deleted; `ThinNodeArena` is the only representation
- **Multi-file resolution**: ES6 imports correctly resolve across files

### Concurrency
- `TypeInterner` uses sharded `DashMap` + atomics (avoids `RwLock<Vec<_>>`)
- Remaining work: measure contention under parallel workloads

### Parser Recovery (Resolved)
- Removed error budget gaming
- Recovery relies on `resync_after_error()` + `last_error_pos` dedup

---

## Commands

```bash
# Build
cargo build

# Run all tests (local)
cargo test --lib

# Run specific test module
cargo test --lib solver::

# Build WASM
wasm-pack build --target nodejs --out-dir pkg

# Quick conformance test
cd conformance && npm run test:100

# Full conformance test
cd conformance && npm run test:1000
```

---

## Rules

### Never Break The Build
- All commits must pass unit tests
- No change should reduce conformance accuracy

### Keep Architecture Clean
- No shortcuts or test-aware code in source
- Fix root causes, not symptoms
- No whack-a-mole error suppression

### Anti-Patterns to Avoid

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |
| Add "Tier 0/1/2" patches | Fix root cause once |

---

## Merge Criteria

1. `cargo build` must pass with no errors
2. `cargo test` must pass with no failures
3. Tests must run fast (< 30 seconds for full suite)
4. Individual tests must complete in < 5 seconds

---

## Project Goals

**Target:** 95%+ exact match with TypeScript compiler on conformance tests.

**Non-Goals:**
- 100% compatibility (edge cases acceptable)
- Supporting deprecated features
