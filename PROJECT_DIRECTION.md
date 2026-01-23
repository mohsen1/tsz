# Project Zang

TypeScript compiler rewritten in Rust, compiled to WebAssembly. Goal: TSC compatibility with better performance.

**Current Status:** Solver needs tuning - weak type checking is too strict, causing 12,122 extra TS2322 errors. Focus is on reducing false positives in the solver's assignability logic.

**Conformance Baseline (2026-01-22):** 36.3% pass rate (4,423/12,197 tests)

---

## Top Priority: Reduce False Positives (Extra Errors)

**Problem:** TSZ emits far too many incorrect errors, causing 63.7% test failure rate. The checker is **too strict**, not too lenient.

**Key Insight:** Extra errors (false positives) outnumber missing errors significantly. Fixing false positives will have higher impact on pass rate than adding missing errors.

### Top Extra Errors (False Positives)

| Error Code | Extra Count | Description | Root Cause |
|------------|-------------|-------------|------------|
| TS2322 | 12,122x | Type not assignable | **Subtype checking too strict** |
| TS2304 | 3,798x | Cannot find name | **Symbol resolution false negatives** |
| TS2694 | 3,104x | Namespace has no exported member | **Export resolution broken** |
| TS1005 | 2,706x | Expected X | **Parser error recovery** |
| TS2552 | 1,825x | Cannot find name, did you mean? | **Suggestion logic triggering incorrectly** |
| TS2571 | 1,698x | Object is of type unknown | **Type inference defaulting to unknown** |
| TS2300 | 1,515x | Duplicate identifier | **Scope merging broken** |
| TS2339 | 1,498x | Property does not exist | **Member lookup incomplete** |

### Strategy: Fix Root Causes

**Phase 1: TS2322 False Positives (CRITICAL - 12,122 extra)**

**Root Cause Identified: Double Weak Type Checking**

The solver checks weak types **redundantly in two places**, compounding strictness:

| Layer | File | Lines | What It Does |
|-------|------|-------|--------------|
| CompatChecker | `src/solver/compat.rs` | 167-169, 289-481 | Checks `violates_weak_type()` and `violates_weak_union()` |
| SubtypeChecker | `src/solver/subtype.rs` | 375, 3704-3746 | Checks `violates_weak_type()` **again** |

Both have `enforce_weak_types = true` by default. If either layer finds a violation, assignment fails.

**What's a "Weak Type"?**
An object where all properties are optional: `{ a?: number }`. TypeScript requires that when assigning to a weak type, the source must share at least one property name with the target.

**The Problem:**
- `has_common_property()` logic may be too strict with union types
- `violates_weak_union()` has complex logic with likely false positives
- Redundant checking means borderline cases fail twice

**Fixes (in priority order):**

1. **Remove duplicate check** - Disable weak type check at `subtype.rs:375`, let only `compat.rs` handle it:
   ```rust
   // subtype.rs:375 - REMOVE or set enforce_weak_types = false
   if self.enforce_weak_types && self.violates_weak_type(source, target) {
       return SubtypeResult::False;
   }
   ```

2. **Review `violates_weak_union()`** (`compat.rs:315-357`) - Complex logic, likely source of false positives

3. **Review `has_common_property()`** - May reject valid assignments in union type cases

**Debug approach:**
```bash
# Find a test where TSZ emits TS2322 but TSC doesn't
./conformance/run-conformance.sh --native --max=50 --verbose 2>&1 | \
  grep -B5 "Extra: TS2322"

# Compare specific test
npx tsc --noEmit path/to/test.ts  # Should have NO errors
cargo run -- --check path/to/test.ts  # Incorrectly emits TS2322
```

**Key files:**
- `src/solver/compat.rs:289-481` - Weak type violation logic (PRIMARY)
- `src/solver/subtype.rs:375,3704-3746` - Redundant weak check (REMOVE)
- `src/checker/state.rs:12400-12430` - Where checker calls solver

**Phase 2: TS2304/TS2694 False Positives (6,902 combined)**

Symbol resolution is marking valid names as "not found":
- Exports being missed during binding
- Namespace members not properly resolved
- Declaration merging incomplete

**Focus files:**
- `src/binder/` - Symbol table construction
- `src/checker/state.rs` - Symbol lookup methods
- `src/checker/modules.rs` - Module/namespace resolution

**Phase 3: TS1005 Parser Errors (2,706 extra)**

Parser error recovery is emitting errors where TSC doesn't. May indicate:
- Overly strict parsing in optional syntax positions
- Error recovery generating false positives
- Missing syntax support

**Focus files:**
- `src/parser/` - Parser implementation
- `src/thin_parser/` - Thin parser if used

### Success Criteria

- **Week 1:** TS2322 extra count < 5,000 (from 12,122)
- **Week 2:** TS2304/TS2694 extra count < 3,000 (from 6,902)
- **Target:** Pass rate 36.3% → 50%+

---

## High Priority: Missing Errors Coverage

**Problem:** Some legitimate errors are not being emitted.

### Top Missing Errors

| Error Code | Missing Count | Description |
|------------|---------------|-------------|
| TS2318 | 3,372x | Cannot find global type '{0}' |
| TS2307 | 2,304x | Cannot find module '{0}' |
| TS2304 | 2,072x | Cannot find name '{0}' |
| TS2488 | 1,770x | Type must have Symbol.iterator |
| TS2583 | 1,184x | Cannot find name. Do you need to change target? |
| TS2362 | 873x | Left-hand side of arithmetic must be number/bigint |
| TS2322 | 847x | Type not assignable (legitimate) |
| TS2363 | 753x | Right-hand side of arithmetic must be number/bigint |

### Strategy

**Note:** Fix false positives FIRST. Many "missing" errors may appear once false positives stop masking them.

**After Phase 1-3 above:**
1. TS2318/TS2307: Module resolution gaps
2. TS2488: Iterator protocol checking
3. TS2362/TS2363: Arithmetic operand type checking
4. TS2583: ES version feature detection

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
| conformance | 5,626 | 35.5% (1,997) |
| compiler | 6,369 | 36.9% (2,353) |
| projects | 144 | 50.7% (73) |
| **Total** | **12,197** | **36.3% (4,423)** |

**Performance:** 92 tests/sec with 8 workers

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
| Add more error checks before fixing false positives | Fix false positives first |
| Chase pass percentages | Fix root causes systematically |
| Add test-specific workarounds | Fix underlying logic |
| Suppress errors to pass tests | Understand why error is wrong |

---

## Key Files

### For TS2322 False Positives (Solver - TOP PRIORITY)
| File | Lines | Purpose |
|------|-------|---------|
| `src/solver/compat.rs` | 131-181 | `is_assignable()` entry point |
| `src/solver/compat.rs` | 289-481 | Weak type violation logic |
| `src/solver/compat.rs` | 315-357 | `violates_weak_union()` - likely culprit |
| `src/solver/subtype.rs` | 375 | **REMOVE** - redundant weak check |
| `src/solver/subtype.rs` | 3704-3746 | `violates_weak_type()` implementation |
| `src/checker/state.rs` | 12400-12430 | Where checker calls `is_assignable_to()` |

### For TS2304/TS2694 False Positives (Symbol Resolution)
| File | Purpose |
|------|---------|
| `src/binder/` | Symbol table construction |
| `src/checker/state.rs` | Symbol lookup methods |
| `src/checker/modules.rs` | Module/namespace resolution |

### For Missing Error Fixes
| File | Purpose |
|------|---------|
| `src/module_resolver.rs` | TS2307 module not found |
| `src/checker/state.rs` | TS2318 global type lookup |
| `src/solver/lower.rs` | Type reference resolution |

---

## Immediate Action Plan

### Day 1: Remove Redundant Weak Type Check

**Quick win:** Remove duplicate weak type enforcement in `src/solver/subtype.rs:375`:

```rust
// BEFORE (too strict - checks twice)
if self.enforce_weak_types && self.violates_weak_type(source, target) {
    return SubtypeResult::False;
}

// AFTER (let compat.rs handle it)
// Remove or disable this check
```

Run conformance to measure impact.

### Day 2: Debug Remaining TS2322 False Positives

```bash
# Find tests with extra TS2322
./conformance/run-conformance.sh --max=50 --verbose 2>&1 | \
  grep -A2 "Extra: TS2322" | head -30

# Pick ONE test, compare TSC vs TSZ
npx tsc --noEmit conformance/path/to/test.ts
cargo run -- --check conformance/path/to/test.ts
```

Focus on `violates_weak_union()` in `compat.rs:315-357`.

### Day 3-4: Fix `violates_weak_union()` Logic

Review the complex union handling - likely generating false positives.

### Day 5: Fix Symbol Resolution False Positives

Debug TS2304/TS2694 extra errors.

**Goal:** TS2322 extra count < 5,000 (from 12,122), pass rate 36.3% → 45%+

---

**Total Codebase:** ~500,000 lines of Rust code
