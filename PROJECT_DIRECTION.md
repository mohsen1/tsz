# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (100 tests) | **53.5%** |
| Conformance (500 tests) | **40.8%** |
| Driver Tests | 113/113 passing |
| Crashes | 0 |

---

## Priority List

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

### 2. Make Flow Analysis Iterative

`src/checker/flow_analyzer.rs`: `check_flow` is recursive. Deeply nested control flow will blow the stack. **Needs iterative worklist algorithm.**

### 3. Fix Transform Pipeline

`src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

### 4. Code Hygiene

- Remove `#![allow(dead_code)]` and fix unused code
- Move scripts to `scripts/`, Docker files to `scripts/docker/`
- Add proper tracing infrastructure (replace print statements)
- Clean up Clippy ignores in `clippy.toml`

---

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/thin_checker.rs` | Type checker | 25,315 |
| `src/thin_parser.rs` | Parser | 10,704 |
| `src/thin_binder.rs` | Symbol binding | 4,452 |
| `src/solver/` | Type resolution | 37 files |

---

## Commands

```bash
cargo build                              # Build
cargo test --lib                         # Run all tests
cargo test --lib solver::                # Run specific module
wasm-pack build --target nodejs          # Build WASM
cd conformance && npm run test:100       # Quick conformance
cd conformance && npm run test:1000      # Full conformance
```

---

## Rules

- All commits must pass unit tests
- No test-aware code in source
- Fix root causes, not symptoms

| Don't | Do Instead |
|-------|------------|
| Check file names in checker | Fix the underlying logic |
| Suppress errors for specific tests | Implement correct behavior |

---

## Merge Criteria

1. `cargo build` passes
2. `cargo test` passes
3. Tests run in < 30 seconds
