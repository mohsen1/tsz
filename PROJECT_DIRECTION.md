# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (2,000 tests) | **29.4%** (588/2,000) |
| Driver Tests | 113/113 passing |
| Test Speed | **73 tests/sec** |
| Crashes | 3 | OOM: 0 | Timeout: 2 |

---

## Priority List

### 1. Improve Conformance Test Pass Rate

**Target: 95%+**

#### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2318 | 1,974x | Cannot find global type (expected with @noLib) |
| TS2583 | 536x | Cannot find name (need ES2015+ lib) |
| TS2711 | 232x | Cannot assign to 'exports' (CommonJS) |
| TS2304 | 228x | Cannot find name |
| TS2792 | 226x | Cannot find module |
| TS2488 | 178x | Type must have Symbol.iterator |

#### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2571 | 870x | Object is of type 'unknown' |
| TS2300 | 646x | Duplicate identifier |
| TS2322 | 282x | Type not assignable |
| TS2304 | 278x | Cannot find name |
| TS1005 | 219x | Expected token (parser) |

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
