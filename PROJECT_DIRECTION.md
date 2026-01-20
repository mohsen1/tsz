# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

## Current Status

| Metric | Value |
|--------|-------|
| Conformance (500 tests) | **30.0%** (150/500) |
| Driver Tests | 113/113 passing |
| Test Speed | **68 tests/sec** |
| Crashes | 0 | OOM: 0 | Timeout: 0 |

### Recent Improvements
- TS2571 "Object is of type 'unknown'" reduced 64% (use ANY for unresolved symbols)
- TS2300 "Duplicate identifier" reduced 14% (static vs instance member fix)
- Crashes eliminated: 865 → 0 (comprehensive compiler options support)

---

## Priority List

### 1. Improve Conformance Test Pass Rate

**Target: 50%+ (currently 30%)**

#### Top Missing Errors (we should emit but don't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2318 | 696x | Cannot find global type (expected with @noLib tests) |
| TS2583 | 298x | Cannot find name (need ES2015+ lib) |
| TS2304 | 59x | Cannot find name |
| TS2307 | 53x | Cannot find module |
| TS7006 | 27x | Parameter implicitly has 'any' type |

#### Top Extra Errors (we emit but shouldn't)
| Error | Count | Description |
|-------|-------|-------------|
| TS2300 | 60x | Duplicate identifier |
| TS1005 | 58x | Expected token (parser) |
| TS2339 | 34x | Property does not exist |
| TS1202 | 28x | Import assignment in ESM |
| TS2454 | 26x | Variable used before assigned |

### 2. Make Flow Analysis Iterative

`src/checker/flow_analyzer.rs`: `check_flow` is recursive. Deeply nested control flow will blow the stack. **Needs iterative worklist algorithm.**

### 3. Fix Transform Pipeline

`src/transforms/` mixes AST manipulation with string emission. Transforms should produce a lowered AST, then the printer should emit strings.

### 4. Code Hygiene

- Remove `#![allow(dead_code)]` and fix unused code
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
./conformance/run-conformance.sh --max=500   # Run 500 conformance tests
./conformance/run-conformance.sh --all       # Run all conformance tests
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
