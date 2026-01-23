# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

**Current Status:** False positives have been fixed (no extra errors in top list). Now focusing on adding missing error detection.

**Conformance Baseline (2026-01-23):** 41.5% pass rate (5,056/12,197 tests) - up from 36.3%

---

## Top Priority: Add Missing Error Detection

**Problem:** TSZ is missing error detection for many cases. The checker is no longer too strict (no extra errors in top list).

**Key Insight:** False positives have been fixed. Now focusing on adding missing error detection to improve coverage.

### Completed: False Positives (Extra Errors)

The following issues have been fixed:
- TS2322 false positives - Removed duplicate weak type checking
- TS2304 false positives - Fixed symbol resolution
- Extra errors no longer appear in conformance top list

### Top Missing Errors (Current Focus)

| Error Code | Missing Count | Description | Priority |
|------------|---------------|-------------|----------|
| TS2304 | 4,636x | Cannot find name | HIGH |
| TS2318 | 3,492x | Cannot find global type | HIGH |
| TS2307 | 2,331x | Cannot find module | HIGH |
| TS2583 | 1,913x | Change target library? | MEDIUM |
| TS2322 | 1,875x | Type not assignable (legitimate) | MEDIUM |
| TS2488 | 1,780x | Type must have Symbol.iterator | MEDIUM |
| TS2339 | 950x | Property does not exist | MEDIUM |
| TS2362 | 901x | Left-hand side arithmetic | LOW |

### Strategy

1. **TS2304/TS2318:** Symbol resolution and global type lookup gaps
2. **TS2307:** Module resolution improvements
3. **TS2488:** Iterator protocol checking
4. **TS2362/TS2363:** Arithmetic operand type checking
5. **TS2583:** ES version feature detection

**Key Files for Missing Errors:**
| File | Purpose |
|------|---------|
| `src/module_resolver.rs` | TS2307 module not found |
| `src/checker/state.rs` | TS2318 global type lookup, TS2304 symbol resolution |
| `src/solver/lower.rs` | Type reference resolution |
| `src/binder/` | Symbol table construction |
| `src/checker/modules.rs` | Module/namespace resolution |

---

## Secondary Priority: Stability Issues

### OOM Tests (4 tests)

These tests cause out-of-memory:
- `compiler/genericDefaultsErrors.ts`
- `conformance/types/typeRelationships/recursiveTypes/infiniteExpansionThroughInstantiation2.ts`
- `compiler/thislessFunctionsNotContextSensitive3.ts`
- `compiler/recursiveTypes1.ts`

**Root cause:** Infinite type expansion or unbounded recursion in solver.

**Fix:** Add recursion depth limits in type instantiation.

### Timeout Tests (54 tests)

Tests hanging without completion. Sample:
- `conformance/salsa/moduleExportAssignment7.ts`
- `compiler/typeNamedUndefined2.ts`
- `compiler/constructorWithIncompleteTypeAnnotation.ts`

**Root cause:** Likely infinite loops in type resolution or checker.

**Fix:** Add iteration limits and cycle detection.

### Worker Crashes (112 crashed, 113 respawned)

Workers are crashing during test execution. This is separate from the 15 crashed tests.

**Likely causes:**
- Panic in Rust code propagating to WASM
- Stack overflow on deep recursion

---

## Transform Pipeline Migration

**Status:** Lower priority now. Focus on diagnostic accuracy first.

**Problem:** `src/transforms/` has ~7,500 lines mixing AST manipulation with string emission.

**Transforms needing migration:**
| Transform | Lines | Complexity |
|-----------|-------|------------|
| `class_es5.rs` | 4,849 | CRITICAL |
| `async_es5.rs` | 1,491 | HIGH |
| `namespace_es5.rs` | 1,169 | MEDIUM |

**Reference implementations:** `enum_es5.rs`, `destructuring_es5.rs`, `ir.rs`

---

## Test Infrastructure Status

**Current state:** Stable and functional.

| Category | Files | Pass Rate |
|----------|-------|-----------|
| conformance | 5,655 | 39.0% (2,206) |
| compiler | 6,398 | 43.4% (2,777) |
| projects | 144 | 50.7% (73) |
| **Total** | **12,197** | **41.5% (5,056)** |

**Performance:** 103 tests/sec with 8 workers

---

## Commands Reference

```bash
# Build
cargo build                              # Native build
wasm-pack build --target nodejs          # WASM build

# Test
cargo test --lib                         # All unit tests
cargo test --lib solver::subtype_tests   # Specific module

# Conformance
./conformance/run-conformance.sh --all --workers=8    # Full suite
./conformance/run-conformance.sh --max=100            # Quick check
./conformance/run-conformance.sh --native             # Use native (faster)
./conformance/run-conformance.sh --filter=path/test   # Single test
```

---

## Rules

| Don't | Do Instead |
|-------|------------|
| Chase pass percentages | Fix root causes systematically |
| Add test-specific workarounds | Fix underlying logic |
| Suppress errors to pass tests | Understand why error is wrong |

---

**Total Codebase:** ~500,000 lines of Rust code
