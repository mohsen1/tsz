# How to Improve Conformance

## Quick Start

```bash
# Run conformance tests (default: actionable summary)
./scripts/conformance/run.sh

# Verbose mode (full category breakdown)
./scripts/conformance/run.sh --verbose

# Investigate specific failures
./scripts/conformance/run.sh --filter=StrictMode --print-test
```

# Conformance Improvement Action Plan

## Executive Summary

**Current Pass Rate: 48.4% (5,995/12,378)**

After deep analysis with ask-gemini and code inspection, I've identified **5 fundamental architectural issues** that are responsible for the majority of conformance failures. Fixing these in order will yield the biggest improvements.

---

## Root Cause Analysis

### The Error Pattern Tells the Story

| Error | Missing | Extra | Root Cause |
|-------|---------|-------|------------|
| **TS2304** | 1,412 | 829 | Symbol resolution returns ANY instead of erroring (Any Poisoning) |
| **TS2318** | 1,185 | - | Global types not resolved from lib contexts |
| **TS2322** | 670 | 1,358 | Both directions: ANY suppresses real errors + false assignability |
| **TS2339** | - | 1,288 | Property lookup falls back to incomplete hardcoded lists |
| **TS18050** | 679 | - | Null checks don't trigger because type is ANY |
| **TS1005** | - | 1,131 | Parser ASI edge cases producing false syntax errors |

### The Fundamental Problem: "Any Poisoning"

The checker has a **cascading failure mode** where unresolved symbols return `TypeId::ANY` instead of emitting errors. This causes:

1. **Suppressed TS2304** - Symbol not found → return ANY → no error
2. **Suppressed TS2318** - Global type not found → return ANY → no error  
3. **Suppressed TS18050** - Type is ANY (not null) → no null check
4. **Suppressed TS2322** - ANY is assignable to everything → no type error
5. **False TS2339** - When real types appear, they fail property lookups

**Evidence from code:**

```rust
// src/checker/type_computation_complex.rs
if self.ctx.has_lib_loaded() {
    // Lib files loaded but global not found - use ANY for graceful degradation
    return TypeId::ANY;  // <-- SUPPRESSES ERROR
}
```

---

## Targeted Action Plan (Priority Order)

### Phase 1: Stop the Bleeding - Fix Any Poisoning (~2000+ tests)

**Problem:** The checker returns `TypeId::ANY` for unresolved symbols to "prevent cascading errors." This is backwards - the cascading errors ARE the real bugs.

**Actions:**

1. **Change default behavior of `report_unresolved_imports` to `true`**
   - File: `src/checker/context.rs`
   - Currently defaults to `false`, which suppresses TS2304/TS2307

2. **Remove "graceful degradation" that returns ANY for missing globals**
   - File: `src/checker/type_computation_complex.rs` (~line 1576)
   - Pattern: `if first_char.is_uppercase() || self.is_known_global_value_name(name) { return TypeId::ANY; }`
   - Should emit TS2304/TS2318 instead

3. **Return TypeId::ERROR instead of TypeId::ANY for failed resolutions**
   - TypeId::ERROR will still prevent cascading errors but won't silently pass assignability

**Expected Impact:** ~1,500 missing errors become correct, but may introduce some extra errors initially

---

### Phase 2: Fix Global Type Resolution (~1,200 tests)

**Problem:** Utility types like `Partial`, `Pick`, `Record`, `Promise` aren't being resolved from lib contexts even when lib files are loaded.

**Actions:**

1. **Audit lib context integration in `resolve_type_reference`**
   - File: `src/checker/state_type_resolution.rs`
   - Verify `ctx.lib_contexts` are being searched for type symbols

2. **Fix symbol lookup to search lib contexts for type names**
   - File: `src/checker/symbol_resolver.rs`
   - `resolve_identifier_symbol_in_type_position` must check lib binders

3. **Ensure merged lib binder has all symbols**
   - File: `src/cli/driver.rs` - `load_lib_files_for_contexts()`
   - Verify symbols aren't lost during merging

**Expected Impact:** ~1,200 TS2318 errors resolved

---

### Phase 3: Fix Property Resolution on Primitives (~1,300 tests)

**Problem:** The hardcoded `apparent.rs` fallback is incomplete, causing false "property doesn't exist" errors.

**Actions:**

1. **Ensure lib.d.ts types are used for primitive boxed types**
   - File: `src/solver/operations_property.rs`
   - `resolve_primitive_property` should resolve from `String`, `Number` interfaces in lib

2. **Expand hardcoded fallback lists as backup**
   - File: `src/solver/apparent.rs`
   - Add missing ES2020+ methods: `at`, `replaceAll`, `isWellFormed`, etc.

3. **Add proper TypeResolver integration**
   - The solver should query a TypeResolver for primitive methods, not hardcode them

**Expected Impact:** ~1,000 extra TS2339 errors eliminated

---

### Phase 4: Fix Parser ASI Edge Cases (~1,100 tests)

**Problem:** Parser generates false TS1005 "Expected X" errors in restricted productions.

**Actions:**

1. **Audit `can_parse_semicolon_for_restricted_production()`**
   - File: `src/parser/state.rs`
   - Compare behavior to TSC for `return`, `throw`, `break`, `continue`

2. **Fix expression parsing after restricted keywords**
   - When ASI should insert, don't try to parse expression that will fail

3. **Add test cases for common ASI patterns**
   - `return` followed by newline
   - `throw` followed by object literal on same line

**Expected Impact:** ~1,100 extra TS1005 errors eliminated

---

### Phase 5: Implement Missing Checks (~700 tests)

**Problem:** strictNullChecks and excess property checking aren't fully implemented.

**Actions:**

1. **Fix TS18050 (possibly null/undefined) detection**
   - File: `src/checker/state_type_analysis.rs`
   - `split_nullish_type` needs to work even when type isn't pure null/undefined

2. **Implement object literal freshness tracking**
   - Checker must track when object literals are "fresh" (just created)
   - Pass freshness flag to `is_assignable_to` for excess property check

3. **Call `check_excess_properties` for fresh object literals**
   - File: `src/solver/sound.rs` has the logic
   - Checker needs to invoke it at assignment sites

**Expected Impact:** ~700 tests for null checks, ~300 for excess properties

---

## Implementation Order

```
Week 1: Phase 1 (Any Poisoning)
         ├── Change report_unresolved_imports default
         ├── Remove ANY fallback for globals  
         └── Return ERROR instead of ANY for failures

Week 2: Phase 2 (Global Types)
         ├── Audit lib context symbol lookup
         ├── Fix resolve_identifier_symbol_in_type_position
         └── Verify lib merging

Week 3: Phase 3 (Property Resolution)  
         ├── Use lib types for primitive methods
         └── Expand apparent.rs fallback

Week 4: Phase 4 (Parser)
         ├── Fix ASI handling
         └── Add restricted production tests

Week 5: Phase 5 (Missing Checks)
         ├── Fix strictNullChecks
         └── Add excess property checking
```

---

## Quick Wins to Validate Approach

Before diving into full phases, test the theory with minimal changes:

1. **Change `report_unresolved_imports` default to `true`** - Single line change, measures impact
2. **Log when returning ANY for unresolved symbols** - Quantify the problem
3. **Pick one TS2318 failing test and trace the resolution** - Verify lib context issue

---

## Key Files to Modify

| Phase | Files |
|-------|-------|
| 1 | `src/checker/context.rs`, `src/checker/type_computation_complex.rs` |
| 2 | `src/checker/state_type_resolution.rs`, `src/checker/symbol_resolver.rs` |
| 3 | `src/solver/operations_property.rs`, `src/solver/apparent.rs` |
| 4 | `src/parser/state.rs` |
| 5 | `src/checker/state_type_analysis.rs`, `src/checker/assignability_checker.rs` |

---

## Expected Outcome

| Phase | Est. Tests Fixed | Cumulative Pass Rate |
|-------|------------------|---------------------|
| Current | - | 48.4% |
| Phase 1 | +1,500 | ~60% |
| Phase 2 | +1,200 | ~70% |
| Phase 3 | +1,000 | ~78% |
| Phase 4 | +1,100 | ~87% |
| Phase 5 | +700 | ~92% |

---

**Bottom Line:** The fundamental issue is that the checker was designed for "graceful degradation" which silently accepts broken code. TypeScript expects errors to be emitted, not suppressed. Phase 1 (stopping any poisoning) is the single most impactful change.

---

## Progress Tracking

### Phase 1: Fix Any Poisoning

**Status:** BLOCKED (waiting for Phase 2)

**Root Cause Identified:**
- CLI driver (`src/cli/driver.rs:1409`) sets `report_unresolved_imports = true`
- tsz-server (`src/bin/tsz_server/main.rs`) does NOT set this flag
- Conformance tests run through tsz-server, so all unresolved symbols return `TypeId::ANY`

**Experiment Results (Feb 1, 2026):**

Tried enabling `report_unresolved_imports = true` in tsz-server:

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Pass Rate | 48.4% | 45.2% | **-3.2%** |
| Missing TS2304 | 1,412 | 676 | -736 (good!) |
| Extra TS2304 | 829 | 1,946 | +1,117 (bad!) |
| Extra TS2307 | 604 | 1,677 | +1,073 (bad!) |

**Analysis:** Enabling error reporting fixed 736 missing TS2304 errors but created 1,117 *additional* extra errors. The net effect is negative because **our resolution is broken** - we emit errors for symbols that TSC successfully resolves.

**Conclusion:** Must fix global type and module resolution (Phase 2) BEFORE enabling error reporting.

**Completed:**
- [x] Identified `report_unresolved_imports` as the suppression mechanism
- [x] Tested enabling it - confirms resolution is the real problem
- [x] Reverted change, added comments explaining the blocker

**Blocked on:**
- Phase 2: Global type resolution must work first
- Module resolution improvements needed

---

### Phase 2: Fix Global Type Resolution (EXPERIMENTAL)

**Status:** EXPERIMENT COMPLETED - APPROACH IDENTIFIED BUT COMPLEX TRADEOFFS

**Root Cause (via ask-gemini):** SymbolId collision!

When `resolve_identifier_symbol` finds `Promise` in a lib binder, it returns a `SymbolId` that's **local to that lib binder** (e.g., `SymbolId(50)`). But `get_symbol_globally` first checks the **current file's binder** with that same ID. If the current file has any symbol at index 50, it returns the wrong symbol!

**Why CLI driver works but tsz-server fails:**

| Component | Lib Symbol Handling |
|-----------|---------------------|
| **CLI driver** | Uses `parse_and_bind_parallel_with_lib_files` → calls `merge_lib_contexts_into_binder` → **remaps SymbolIds** to avoid collisions |
| **tsz-server** | Creates fresh binder per file → sets `lib_contexts` separately → **NO remapping** → SymbolId collisions |

---

#### Experiment 1: Enable `report_unresolved_imports` (WITHOUT lib merging)

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Pass Rate | 48.4% | 45.2% | -3.2% |
| Missing TS2304 | 1412 | 736 | -676 (good) |
| Extra TS2304 | 829 | 1946 | +1117 (bad) |
| Extra TS2307 | - | 1677 | +1677 (bad) |
| Extra TS2749 | - | 1664 | +1664 (bad) |

**Conclusion:** We emit MORE errors than we fix because global/module resolution fails.

---

#### Experiment 2: Add lib symbol merging (WITH `report_unresolved_imports` enabled)

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Pass Rate | 45.2% | 45.4% | +0.2% |
| Extra TS2304 | 1946 | 2058 | +112 (worse!) |
| Extra TS2307 | 1677 | 2180 | +503 (worse!) |

**Conclusion:** Lib merging didn't help when error reporting is enabled.

---

#### Experiment 3: Add lib symbol merging (WITH `report_unresolved_imports` disabled)

| Metric | Baseline | After Merging | Delta |
|--------|----------|---------------|-------|
| Pass Rate | 48.4% | 48.0% | -0.4% |
| Extra TS2339 | 1288 | 1966 | +678 (bad) |
| Extra TS2304 | 829 | 697 | -132 (good) |

**Conclusion:** Lib merging fixes some TS2304 but causes MORE TS2339 (property not found) errors. 
When types resolve correctly (instead of becoming `any`), property checks that were silently passing now fail.

---

**Key Insight:** The issues are interconnected:
1. **SymbolId collision** prevents correct global type resolution
2. **But** fixing it exposes property resolution bugs (TS2339)
3. **And** exposes module resolution bugs (TS2307)

**Revised Strategy:**

Instead of fixing Phase 2 first, we should:
1. **Phase 3 first:** Fix property resolution on primitives (TS2339)
2. **Then Phase 2:** Fix global type resolution (will now work without regressions)
3. **Then Phase 1:** Enable `report_unresolved_imports`

**Files to Investigate for TS2339:**
- `src/solver/operations_property.rs` - property lookup
- `src/solver/apparent.rs` - hardcoded primitive methods
- `src/checker/state_checking_members.rs` - member resolution

---

#### Additional Investigation (2026-02-01)

**Property Resolution Deep Dive:**

Tested property resolution manually:
- `"hello".length` → Works ✓ (basic ES5 property)
- `"hello".includes("x")` → TS2339 with ES5 target ✗ (ES2015+ method)
- `"hello".includes("x")` → Works ✓ with ES2015 target

**Gemini Analysis:**

The `register_boxed_types()` function in `checker/type_checking.rs` IS correctly loading the boxed types from lib.d.ts. The issue is:

1. **Target-dependent libs:** Default target is ES5, which only loads `lib.es5.d.ts`
2. **ES2015+ methods missing:** Methods like `includes()`, `find()`, `at()` are defined in `lib.es2015.core.d.ts` and later
3. **Conformance comparison is fair:** Both TSC and tsz use the same target/lib defaults, so this isn't causing the extra TS2339 errors

**Current Pass Rate:** 48.0% (baseline before any changes)

**Remaining Questions:**
- Why do we have ~1966 extra TS2339 when TSC and tsz should have the same libs loaded?
- Could be interface augmentation failures (lib.es2015 extends interfaces from lib.es5)
- Could be inheritance chain not being followed correctly