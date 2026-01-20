# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (12,053 tests) | **24.7%** (2,983/12,053) |
| Driver Tests | 113/113 passing |
| Test Speed | **106 tests/sec** |
| Crashes | 865 | OOM: 37 | Timeout: 57 |

---

## Priority List

### 1. Improve Conformance Test Pass Rate

**Target: 95%+**

#### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2304 | 4,764x | Cannot find name |
| TS7053 | 2,458x | Element implicitly has 'any' type |
| TS2792 | 2,377x | Cannot find module |
| TS2339 | 2,147x | Property does not exist |
| TS2583 | 1,882x | Cannot find name (need ES2015+ lib) |
| TS2488 | 1,571x | Type must have Symbol.iterator |

#### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2304 | 393,322x | Cannot find name (symbol resolution bug!) |
| TS2322 | 11,939x | Type not assignable |
| TS1005 | 3,473x | Expected token (parser) |
| TS2571 | 3,137x | Object is of type 'unknown' |
| TS2694 | 3,105x | Namespace has no exported member |

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
