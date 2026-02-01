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

**Current Pass Rate: 48.6% (6,009/12,378)** — Updated Feb 1, 2026

After deep analysis with ask-gemini and code inspection, I've identified **5 fundamental architectural issues** that are responsible for the majority of conformance failures. Fixing these in order will yield the biggest improvements.

---

## Root Cause Analysis

### The Error Pattern Tells the Story

| Error | Missing | Extra | Root Cause |
|-------|---------|-------|------------|
| **TS2304** | 1,405 | 831 | Symbol resolution returns ANY instead of erroring (Any Poisoning) |
| **TS2318** | ~~1,185~~ ~~485~~ 336 | - | Global types not resolved from lib contexts (PARTIALLY FIXED) |
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
- Why do we have ~1288 extra TS2339 at baseline?
- Could be inheritance chain not being followed correctly
- Need to investigate specific failing test cases

---

#### Fix Implemented (2026-02-01)

**Type Parameter Canonicalization for Lib Types**

When multiple lib files define the same generic interface (e.g., `Array<T>`), each
lowering created its own type parameter TypeIds. This caused `Array<T1> & Array<T2>`
with T1 != T2, breaking property lookup.

**Fix Applied:**
- Track canonical type parameter TypeIds from first lib definition
- Substitute subsequent definitions' params with canonical ones
- Applied to `resolve_lib_type_by_name` and `resolve_lib_type_with_params`

**Result:** Prevents TS2339 regression (1966 → 1288) when lib types are merged.
Pass rate maintained at 48.4% baseline.

---

#### Fix Implemented (2026-02-01) - TS2318 Global Type Check

**Emit TS2318 for Missing Global Types with --noLib**

TypeScript always emits TS2318 "Cannot find global type X" errors for core types
(Array, String, Boolean, etc.) when they are not available, regardless of --noLib.

**Previous Behavior:**
- Only emitted TS2318 when libs should be loaded but weren't
- With `--noLib`, no global type errors were emitted

**Fix Applied:**
- Changed `check_missing_global_types` to emit TS2318 whenever libs are not loaded
- Matches tsc's behavior: user is responsible for providing core types with --noLib

**Result:** Reduced missing TS2318 from 1185 to 485 (~700 errors fixed).
Pass rate maintained at 48.4% baseline.

**Remaining TS2318 (485):**
- ES2015+ types (Iterator, Promise, etc.) that require feature detection
- Types that are used conditionally based on code features

---

#### Fix Implemented (2026-02-01) - Multi-file Test Lib Context Fix

**Separate Actual Lib File Count from User File Contexts**

Multi-file conformance tests were incorrectly passing `has_lib_loaded()` because
user file contexts were being mixed with lib_contexts in tsz-server.

**The Problem:**
- In `run_check()`, `all_contexts = lib_contexts + user_file_contexts`
- This combined vec was passed to `set_lib_contexts()`
- `has_lib_loaded()` checked `!lib_contexts.is_empty()`
- User files made this return `true` even when NO actual lib files were loaded
- This skipped `check_missing_global_types()` and hid TS2318 errors

**The Fix:**
- Added `actual_lib_file_count` field to `CheckerContext`
- Changed `has_lib_loaded()` to check `actual_lib_file_count > 0`
- tsz-server now calls `set_actual_lib_file_count(lib_files.len())`
- User files no longer affect lib loading detection

**Result:** Reduced missing TS2318 from 485 to 336 (-149 errors).
Pass rate increased from 48.4% (5,995) to 48.5% (6,000), +5 tests.

**Remaining TS2318 (336):**
- Tests with `@lib: es6` where lib.es6.d.ts doesn't exist (expected behavior)
- Tests expecting lib loading to fail for specific configurations

---

#### Investigation (2026-02-01) - Extra TS2339 Root Causes

**Multi-File Test Issues Discovered:**

Multiple interconnected issues causing 1288 extra TS2339 errors:

1. **`TypeKey::Lazy(DefId)` not resolved for property access**
   - Anonymous classes (e.g., `new class { #x = 1 }`) create `Lazy(DefId)` types
   - These DefIds aren't registered in TypeEnvironment
   - When evaluate_type tries to resolve them, returns ERROR
   - Error messages show "Property '#y' does not exist on type 'Lazy(1)'"

2. **TS2300 "Duplicate identifier" for lib types** (Deep Investigation)
   - Tests with ES2015+ targets show errors like "Duplicate identifier 'Array'" at wrong positions
   - Example: Line 5 column 10 shows "Duplicate identifier 'String'" but the code is `function fail`
   - **Root Cause**: Lib symbol IDs are getting into the user binder's symbol table
   - When we look up these IDs, we get lib symbol NAMES but user arena POSITIONS
   - `symbol_is_from_lib()` returns false because `binder.symbol_arenas` isn't populated for lib symbols in tsz-server (only CLI uses parallel merging)
   - Attempted fixes to filter by declaration kind or name match didn't help
   - **Deeper fix needed**: Track which arena each symbol declaration belongs to, or prevent lib symbol IDs from entering user binder

3. **TypeScript libMap not implemented**
   - TypeScript maps `@lib: es6` → `lib.es2015.d.ts` via libMap in commandLineParser.ts
   - Our tsc-runner.ts and tsz-server both lack this mapping
   - Causes lib loading to fail for tests using short names (es6, es7, etc.)

**Code Paths Involved:**
- `src/solver/format.rs:219` - Formats Lazy types as "Lazy(N)" instead of resolving
- `src/solver/evaluate.rs:359` - Returns ERROR when Lazy type can't be resolved
- `src/checker/state_type_analysis.rs:1549,1606` - Emits TS2339 with unresolved type

**Attempted Fixes:**
- Adding Lazy handling to `operations_property.rs` - No effect (error emitted elsewhere)
- Suppressing Lazy types in `error_property_not_exist_at` - Didn't reach all code paths

**Proper Fix Required:**
1. Register anonymous class types in TypeEnvironment during lowering
2. Or resolve Lazy types before emitting errors throughout the checker
3. Implement libMap for lib name aliasing in both tsc-runner and tsz-server

---

## Latest Conformance Run (Feb 1, 2026)

```
Pass Rate: 48.5% (5,999/12,378)
Time: 2.7s (4641 tests/sec)
```

### Highest Impact Fixes (from conformance output)

| Priority | Tests Fixed | Category | Error Codes | Action |
|----------|-------------|----------|-------------|--------|
| **1** | ~719 | Null/undefined checks | TS18050, TS18047, TS18048, TS18049 | Implement strictNullChecks enforcement |
| **2** | ~556 | Global/lib type resolution | TS2318, TS2583, TS2584 | Fix utility type resolution in lib.d.ts |
| **3** | ~517 | Module/import resolution | TS2307, TS2792, TS2834, TS2835 | Fix module resolver for node/bundler modes |
| **4** | ~456 | Operator type constraints | TS2365, TS2362, TS2363, TS2469 | Implement binary operator type checking |
| **5** | ~343 | Type assignability | TS2322, TS2345, TS2741 | Review specific failing patterns |
| **6** | ~321 | Duplicate identifier | TS2300, TS2451, TS2392, TS2393 | Check edge cases in merging |

### Error Summary

| Direction | Top Errors |
|-----------|------------|
| **Missing** | TS2304(1405), TS18050(679), TS2322(670), TS2307(604), TS2339(591) |
| **Extra** | TS2322(1358), TS2339(1288), TS1005(1131), TS2345(993), TS2304(831) |

### Problem Tests

- **Crashed:** `compiler/augmentExportEquals2.ts`
- **Timed Out:** `compiler/thislessFunctionsNotContextSensitive3.ts`

---

## RECOMMENDED NEXT ACTION

**Implement strictNullChecks enforcement (~719 tests)**

This is the highest-impact fix that doesn't have the same dependency issues as Phase 1/2.

### Why strictNullChecks?

1. **Highest ROI:** 719 tests is more than any other single category
2. **Independent:** Doesn't require fixing symbol resolution first
3. **Missing errors only:** TS18050 shows 679 MISSING - we're not emitting errors we should
4. **Well-defined:** TypeScript's behavior is clear - check for null/undefined before member access

### What needs to be done:

1. **TS18050** "The value 'X' is possibly null"
   - Emitted when accessing property/method on nullable type without null check
   - Check `state_type_analysis.rs` for where member access is checked
   
2. **TS18047** "X is possibly null"
   - For direct uses of nullable values in non-null contexts
   
3. **TS18048** "X is possibly undefined"
   - Same as 18047 but for undefined
   
4. **TS18049** "X is possibly null or undefined"
   - Combined case

### Files to investigate:

- `src/checker/state_type_analysis.rs` - Where property access is checked
- `src/checker/state_checking_members.rs` - Member resolution
- `src/solver/narrowing.rs` - Control flow narrowing (might need updates)

### Alternative: Fix the crash first (quick win)

The crashed test `augmentExportEquals2.ts` indicates a bug. Fixing crashes is always good for stability.

```bash
# To investigate the crash:
./scripts/conformance/run.sh --filter=augmentExportEquals2 --print-test
```

---

## Fix Implemented (Feb 1, 2026) - TS18050 for Binary Operators

**Emit TS18050 "The value X cannot be used here" for null/undefined operands in arithmetic**

When null or undefined literals are used in binary arithmetic operations (`*`, `/`, `%`, `-`, etc.),
TypeScript emits TS18050 instead of TS2362/TS2363.

**Previous Behavior:**
- `null * 5` emitted TS2362 "The left-hand side of an arithmetic operation must be of type..."
- TSC emits TS18050 "The value 'null' cannot be used here"

**Fix Applied:**
- Modified `emit_binary_operator_error` in `src/checker/error_reporter.rs`
- Check for null/undefined operands first and emit TS18050
- Still emit TS2362/TS2363 for the OTHER operand if it's also invalid (matching TSC behavior)

**Result:** +10 tests passing (5,999 → 6,009)

**Test Coverage:**
- `arithmeticOperatorWithOnlyNullValueOrUndefinedValue.ts` - PASS
- `arithmeticOperatorWithNullValueAndValidOperands.ts` - PASS  
- `arithmeticOperatorWithUndefinedValueAndValidOperands.ts` - PASS
- `arithmeticOperatorWithNullValueAndInvalidOperands.ts` - PASS
- `arithmeticOperatorWithUndefinedValueAndInvalidOperands.ts` - PASS