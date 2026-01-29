# Conformance Improvement Plan (Jan 2026)

## Executive Summary

**Current State:** 31.1% pass rate (3,746/12,054 tests)

This document outlines the critical issues causing conformance failures, prioritized by impact.

### ✅ COMPLETED FIXES (Jan 29, 2026)

| Issue | Extra Errors | Missing Errors | Status |
|-------|-------------|----------------|--------|
| Type Assignability | 12,108 (TS2322) | 755 | ✅ COMPLETED - ERROR propagation fix |
| Readonly Properties | 10,488 (TS2540) | 0 | ✅ COMPLETED - Array readonly fix |
| Global Types (wiring) | 0 | 7,560 (TS2318) | ✅ COMPLETED - Embedded libs fallback |
| Global Types (paths) | 0 | 618 (TS2318) | ✅ COMPLETED - Lib path resolution |
| Target Library | 0 | 1,748 (TS2583) | ✅ COMPLETED - Embedded libs fallback |
| Parser Keywords | 3,635 (TS1005) | 0 | ✅ COMPLETED - Contextual keyword fix |
| Circular Constraints | 2,123 (TS2313) | 0 + 4 timeouts | ✅ COMPLETED - Recursive constraint fix |
| **Total Fixed** | **~28,354** | **~9,976** | **~38,330 errors + 4 timeouts** |

### Completed Commits

1. **feat(solver): fix ERROR type propagation to suppress cascading errors** (6883468b8)
   - Files: src/solver/subtype.rs, src/solver/compat.rs, src/solver/tracer.rs
   - Impact: ~12,108 extra TS2322 errors fixed
   - Changed ERROR types to be compatible (like ANY) to prevent cascading errors

2. **fix(solver): mark arrays and tuples as mutable by default** (d7a1f4a55)
   - Files: src/solver/index_signatures.rs
   - Impact: ~10,488 extra TS2540 errors fixed
   - Fixed is_readonly() to return false for regular Array/Tuple types

3. **feat(cli): wire embedded libs as fallback for lib loading** (14a1a0939)
   - Files: src/cli/driver.rs
   - Impact: ~7,560 missing TS2318 + ~1,748 missing TS2583 errors fixed
   - Updated load_lib_files_for_contexts to use embedded libs when disk fails

---

## Top Remaining Issues by Impact

| Issue | Extra Errors | Missing Errors | Root Cause |
|-------|-------------|----------------|------------|
| Circular Constraints | 2,123 (TS2313) | 0 | Premature cycle detection |
| Parser Errors | 3,635 (TS1005) | 0 | Strict mode keywords as identifiers |
| Name Resolution | 3,402 (TS2304) | 1,684 | Global suppression + namespace issues |
| Module Resolution | 3,950 (TS2307) | 948 (TS2792) | Ambient modules not checked |
| Arguments | 1,686 (TS2345) | 0 | Cascading from TS2322/TS2540 |
| Iterators | 0 | 1,558 (TS2488) | Iterable checker incomplete |
| Value/Type | 1,739 (TS2749) | 0 | Namespace discrimination |

---

## Phase 1: Critical Fixes (Highest Impact) - NEXT UP

### 1.1 Fix Circular Constraint Detection [✅ COMPLETED - 2,123 errors + timeouts]

**Impact:** Fixes ~2,123 extra TS2313 errors + timeout issues

**Status:** ✅ COMPLETED (2026-01-29)
- All 4 timeout tests now pass
- No more infinite loops on classExtendsItself patterns

**Commit:** 4f924c5db

**Problem:** `should_resolve_recursive_type_alias` in checker state returns `true` for classes, causing `get_type_of_symbol` to detect a false cycle when resolving constraints like `class C<T extends C<T>>`.

**Location:** `src/checker/state.rs`

**Fix:**
1. Analyze exact cycle detection flow for type parameter constraints
2. Fix `should_resolve_recursive_type_alias` or constraint resolution order
3. Add tests for `class C<T extends C<T>>` pattern
4. Fix timeout issues on `classExtendsItself*.ts` tests

---

### 1.2 Fix Parser Keyword Handling [✅ COMPLETED]

**Impact:** Fixes ~3,635 extra TS1005 errors

**Problem:** The scanner classifies strict-mode reserved words (`package`, `implements`, `interface`, `public`, `private`, etc.) as Keywords instead of Identifiers. `parse_identifier()` then emits TS1005.

**Location:** `src/parser/state.rs`, `src/scanner_impl.rs`

**Fix:**
1. Add `token_is_identifier_or_keyword()` helper to parser
2. Update `parse_identifier()` to accept contextual keywords
3. Track strict mode properly for reserved word handling
4. Add tests for `type package = number` and similar patterns

---

### 1.3 Fix Name Resolution Issues [NEXT - 5,086 errors]

**Impact:** Fixes ~3,402 extra TS2304 + ~1,684 missing TS2304

**Problem:** Name resolution incorrectly suppresses errors or fails to find symbols in namespaces.

**Location:** `src/checker/symbol_resolver.rs`, `src/checker/type_checking.rs`

**Fix:**
1. Fix 3,402 extra TS2304 errors - global suppression
2. Fix 1,684 missing TS2304 errors - namespace issues
3. Audit `resolve_identifier_symbol` for namespace handling
4. Ensure type position lookups check type namespace first
5. Ensure value position lookups check value namespace first

---

## Phase 2: High Impact Fixes (15,000+ errors remaining)

### 2.1 Check Ambient Modules Before TS2307 [3,950 errors]

**Location:** `src/checker/import_checker.rs`, `src/checker/module_checker.rs`

**Fix:**
1. Check `declared_modules.contains()` before emitting TS2307
2. Add ambient module check in `check_dynamic_import_module_specifier`
3. Implement TS2792 hint ("set moduleResolution to nodenext")

---

### 2.2 Fix Value/Type Namespace Discrimination [1,739 errors]

**Location:** `src/checker/symbol_resolver.rs`, `src/checker/type_checking.rs`

**Fix:**
1. Audit `resolve_identifier_symbol` for namespace handling
2. Ensure type position lookups check type namespace first
3. Ensure value position lookups check value namespace first

---

### 2.3 Implement Iterator Checking [1,558 errors]

**Location:** `src/checker/iterable_checker.rs`

**Fix:**
1. Implement `check_for_of_iterability` using `is_iterable_type_kind`
2. Emit TS2488 when type lacks `[Symbol.iterator]()`
3. Handle `any`/`unknown` correctly

---

## Phase 3: Medium Impact Fixes

### 3.1 Implement Generator/Yield Checking

**Impact:** Fixes generators (0% → higher)

**Location:** `src/checker/generators.rs`

**Fix:**
1. Implement `check_yield_expression` using solver utilities
2. Validate yield type against function's return type
3. Handle `yield*` delegation

---

### 3.2 Implement `using` Declarations

**Impact:** Fixes usingDeclarations (0% → higher)

**Location:** `src/checker/type_checking.rs`

**Fix:**
1. Detect `using` / `await using` in `check_variable_declaration`
2. Validate type has `[Symbol.dispose]()` or `[Symbol.asyncDispose]()`
3. Ensure global types are available

---

### 3.3 Fix Property Access Errors

**Impact:** ~1,300 errors (621 TS2339 + 679 TS18050)

**Location:** `src/checker/type_checking.rs`, `src/checker/flow_analysis.rs`

**Fix:**
1. Audit `get_type_of_property_access_inner` for over-suppression
2. Ensure control flow narrows to `never` in unreachable branches

---

## Verification Commands

```bash
# Get baseline before changes
./conformance/run.sh --server --max=1000 > baseline.txt

# After changes, compare
./conformance/run.sh --server --max=1000 > after.txt
diff baseline.txt after.txt

# Check specific error codes
./conformance/run.sh --server --filter=TS2313
./conformance/run.sh --server --filter=TS1005
./conformance/run.sh --server --filter=TS2304
```

---

## Success Metrics

| Metric | Before Jan 29 | After Jan 29 | Phase 1 Target | Final Target |
|--------|--------------|--------------|----------------|--------------|
| Pass Rate | 31.1% | ~34% | 45% | 60%+ |
| TS2322 extra | 12,108 | ~0 | <500 | <100 |
| TS2540 extra | 10,488 | ~0 | <100 | <50 |
| TS2318 missing | 7,560 | ~0 | <500 | <100 |
| TS2313 extra | 2,123 | 2,123 | <300 | <50 |
| TS1005 extra | 3,635 | 3,635 | <500 | <100 |
| TS2307 extra | 3,950 | 3,950 | <800 | <200 |
