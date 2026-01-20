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

* TS2571 "Object is of type 'unknown'" reduced 64% (use ANY for unresolved symbols)
* TS2300 "Duplicate identifier" reduced 14% (static vs instance member fix)
* Crashes eliminated: 865 → 0 (comprehensive compiler options support)

## Gaps / Risks

* Conformance vs goals: PROJECT\_DIRECTION targets 50%+; latest recorded runs show either 30% pass or total WASM init crashes (0% in `specs/TEST_RESULTS.md`). Need to reconcile current harness state—blocker for measuring solver accuracy.

```24:44:PROJECT_DIRECTION.md
Current 30% (150/500); target 50%+; missing/extra error buckets listed.
```

```23:45:specs/TEST_RESULTS.md
All 190 tests crashing during WASM initialization; conformance unmeasurable.
```

* Error coverage gaps remain (TS2318/TS2583/TS2304/TS2307/TS7006 missing; TS2300/TS1005/TS2339/TS1202/TS2454 extra). No evidence these are resolved yet; solver/checker work needed.

* Flow analysis still recursive and transform pipeline still mixing lowering/printing per PROJECT\_DIRECTION—architectural debt not yet addressed.

* Compat layer completeness is uncertain: we expose `compat` module, but need an audit against `TS_UNSOUNDNESS_CATALOG.md` to ensure every rule (e.g., weak type detection, template literal limits, rest bivariance, exact optional property types) is wired and option-driven.

* Architecture cleanup is critical-priority; while inspected files show no path heuristics, a full sweep (checker/binder) should confirm all ~40 instances cited as removed are truly gone.

## Suggested Next Steps

* Unblock testing: fix WASM initialization crash path, rerun conformance to get real signal; reconcile 30% vs 0% discrepancy.

* Target top missing/extra errors with solver/checker fixes (library loading for TS2318/2583, name/module resolution, duplicate identifier handling) to move toward 50% goal.

* Implement flow analyzer worklist and transform/print separation per PROJECT\_DIRECTION.

* Perform a compat-layer audit against `TS_UNSOUNDNESS_CATALOG.md`; ensure each rule is option-driven and exercised via tests (freshness, exactOptionalPropertyTypes, rest bivariance, template literal limits, weak types).

* Continue test-awareness sweeps in checker/binder to guarantee alignment with `ARCHITECTURE_CLEANUP.md` and AGENTS rules.

***

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

* Remove `#![allow(dead_code)]` and fix unused code
* Add proper tracing infrastructure (replace print statements)
* Clean up Clippy ignores in `clippy.toml`

***

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/thin_checker.rs` | Type checker | 25,965 |
| `src/thin_parser.rs` | Parser | 10,720 |
| `src/thin_binder.rs` | Symbol binding | 4,537 |
| `src/solver/` | Type resolution | 37 files |

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
