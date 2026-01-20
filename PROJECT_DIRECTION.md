# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (500 tests) | **23.2%** (116/500) |
| Driver Tests | 113/113 passing |
| Test Speed | **87 tests/sec** |
| Crashes | 27 (handled gracefully) |

---

## Priority List

### 1. Improve Conformance Test Pass Rate

**Target: 95%+**

#### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2583 | 296x | Cannot find name (need ES2015+ lib) |
| TS2304 | 95x | Cannot find name |
| TS2792 | 75x | Cannot find module |
| TS2339 | 62x | Property does not exist |
| TS7006 | 52x | Parameter implicitly has 'any' type |
| TS1202 | 48x | Import assignment in ESM |

#### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2300 | 70x | Duplicate identifier |
| TS2571 | 66x | Object is of type 'unknown' |
| TS2322 | 63x | Type not assignable |
| TS1005 | 58x | Expected token (parser) |
| TS2339 | 27x | Property does not exist |

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
