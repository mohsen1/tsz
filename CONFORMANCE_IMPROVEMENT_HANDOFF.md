# Conformance Improvement Handoff Document

## Current State

- **Pass rate**: 48.3% (5,982 / 12,379 tests) on full suite; ~57% on 500-test sample
- **Target**: 80% (9,903 tests)
- **Gap**: ~3,921 more tests need to pass
- **Branch**: `claude/research-assignability-solver-KlCQf`

## Critical Finding

**All 210 failing tests (in 500-test sample) fail EXCLUSIVELY due to extra errors** -- tsz produces errors that tsc does not. Zero tests fail because tsz misses an error tsc reports. This means every fix is about *suppressing false positive errors*.

## Changes Already Made (In This Branch, Uncommitted)

### 1. TS2304 Fix: Deterministic Symbol Ordering
**File**: `src/checker/state_type_environment.rs` (lines 838-919)
**File**: `src/binder.rs` (line 114 - added `PartialOrd, Ord` to `SymbolId`)

**Root cause discovered**: `build_type_environment()` collected symbols via `std::collections::HashSet` (non-deterministic iteration order via `RandomState`). When parameter symbols were processed BEFORE their parent function symbol, the type parameter scope was empty, causing type annotations like `T` to resolve to `ERROR` and emit spurious TS2304.

**Fix applied**:
- Changed `HashSet` to `BTreeSet` for deterministic ordering
- Added `sort_by_key` to process type-defining symbols (functions, classes, interfaces, type aliases, enums, modules) BEFORE variable/parameter symbols
- Added `PartialOrd, Ord` derives to `SymbolId`

### 2. TS2304 Fix: ERROR Cache Re-resolution
**File**: `src/checker/state_type_environment.rs` (lines 1164-1228)

**Root cause**: Once a type reference resolved to `ERROR` (because type params weren't in scope), the ERROR was cached in `node_types` and never re-resolved -- even when type params were later available.

**Fix applied**: Changed cache logic for `TYPE_REFERENCE`, `TYPE_QUERY`, `UNION_TYPE`, and `TYPE_LITERAL` nodes. Old logic:
```rust
if cached == TypeId::ERROR || self.ctx.type_parameter_scope.is_empty() {
    return cached; // Always returns cached ERROR
}
```
New logic:
```rust
if cached != TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
    return cached;
}
if cached == TypeId::ERROR && self.ctx.type_parameter_scope.is_empty() {
    return cached;
}
// Otherwise: re-resolve (ERROR + type params in scope, or non-ERROR + type params changed)
```

## Priority-Ordered Fix List (By Test Impact)

Based on analysis of 500 tests, here are the error codes to fix, ranked by number of tests they'd convert from fail to pass:

### Tier 1: Quick Wins (100% conversion rate, single-code failures)

| Rank | Code | Tests Fixed | Description | Investigation Notes |
|------|------|-------------|-------------|---------------------|
| 1 | **TS2524** | 12 | `'await' expressions are only allowed within async functions` | All in async arrow/function tests across es5/es6/es2017. tsz spuriously emits this in valid async contexts. Look at `src/checker/` for where 2524 is emitted. |
| 2 | **TS1042** | 10 | `'async' modifier cannot be used here` | All in async/es5 and async/es6 categories. tsz rejects `async` in valid positions. Look at parser modifier validation. |
| 3 | **TS2372** | 6 | `Parameter 'X' cannot reference itself` | 100% conversion. tsz wrongly flags parameter self-references. |
| 4 | **TS2585** | 3 | `Cannot find name. Did you mean to change target library?` | 100% conversion. Lib suggestion logic too aggressive. |
| 5 | **TS2326** | 3 | `Types of parameters are incompatible` | 100% conversion. |

### Tier 2: High Impact (mixed single/multi-code failures)

| Rank | Code | Tests Fixed (solo) | Total Tests Affected | Description |
|------|------|-------------------|---------------------|-------------|
| 6 | **TS2339** | 11 | 19 | `Property 'X' does not exist on type 'Y'` - type resolution/member lookup |
| 7 | **TS2322** | 10 | 19 | `Type not assignable` - assignability solver issues |
| 8 | **TS2307** | 8 | 16 | `Cannot find module` - module resolution |
| 9 | **TS2445** | 8 | 14 | `Property protected and only accessible...` - protected access check |
| 10 | **TS2524** | 12 | 12 | (See Tier 1) |

### Tier 3: Medium Impact

| Code | Tests Fixed (solo) | Total Affected | Description |
|------|-------------------|----------------|-------------|
| **TS1109** | 6 | 11 | `Expression expected` - parser false positive |
| **TS2511** | 6 | 6 | `Cannot create instance of abstract class` |
| **TS2341** | 6 | 6 | `Property private and only accessible...` |
| **TS2507** | 5 | 12 | `Type not a constructor function type` |
| **TS2749** | 5 | 11 | `Value used as type` |
| **TS2554** | 4 | - | `Expected X arguments, but got Y` |
| **TS2654** | 5 | - | `Abstract method was not implemented` |

### Projected Impact

Fixing Tier 1 (TS2524 + TS1042 + TS2372 + TS2585 + TS2326) = **34 tests**
Fixing Tier 2 (TS2339 + TS2322 + TS2307 + TS2445) = **37 more tests** (solo only)
**Total from Tier 1+2**: ~71 tests = ~69% pass rate on 500 sample

Extrapolating to full suite: fixing all above could bring us from 48% to ~65-70%.

## Key Architecture Notes

### Error Emission Pipeline
- **Parser errors** (TS1005, TS1042, TS1109): `src/parser/state.rs` - `parse_error_at()`, `error_token_expected()`
- **Name resolution** (TS2304, TS2318, TS2552): `src/checker/state_type_analysis.rs` - `resolve_identifier_in_value_position()`
- **Property access** (TS2339, TS2445, TS2341): `src/checker/type_computation.rs` (line ~890) calls `check_property_accessibility()` in `src/checker/property_checker.rs`
- **Assignability** (TS2322): `src/checker/error_reporter.rs` - `error_type_not_assignable_at()`
- **Module resolution** (TS2307): Binder doesn't do filesystem resolution; needs external `module_exports`
- **Abstract classes** (TS2511): `src/checker/accessibility.rs` - `report_cannot_instantiate_abstract_class()`

### Symbol Resolution Flow
```
resolve_identifier_symbol(idx)
  -> Phase 1: Scope chain traversal (local -> parent -> module)
  -> Phase 2: File locals (includes merged lib symbols)
  -> Phase 3: Lib binders directly (if libs not merged)
  -> Phase 4: Emit TS2304
```

### Protected Access Check Flow (property_checker.rs:40-112)
```
check_property_accessibility(object_expr, property_name, error_node, object_type)
  -> resolve_class_for_access(expr, type) -> (class_idx, is_static)
  -> find_member_access_info(class_idx, name, is_static) -> {level, declaring_class}
  -> For Protected: check current_class derives from declaring_class
     AND receiver_class derives from current_class
```

### Type Environment Building (state_type_environment.rs:838)
```
build_type_environment()
  -> Collect unique symbols from binder.node_symbols
  -> [NEW] Sort: type-defining symbols first, then variables/parameters
  -> For each symbol: get_type_of_symbol() -> insert into TypeEnvironment
  -> Used by is_assignable_to_with_env()
```

## How to Run Tests

```bash
# Full conformance suite (takes ~2-5 min)
bash conformance/run.sh --timeout=600

# 500 test sample
bash conformance/run.sh --max=500 --timeout=300

# Verbose output (shows per-test results)
bash conformance/run.sh --max=500 --verbose --timeout=300

# Single test file
/home/user/tsz/.target/release/tsz <path-to-test.ts>

# Filter tests by name pattern
bash conformance/run.sh --filter=asyncArrow --print-test --max=50

# Build
cargo build                    # debug
cargo build --release --bin tsz        # release binary
cargo build --release --bin tsz-server # release server (used by conformance runner)
```

**Important**: TypeScript submodule must be initialized:
```bash
git submodule update --init --depth=1 TypeScript
```

## Investigation Approach for Each Error Code

For each error code to fix:

1. **Find emission site**: `grep -rn "TS<code>\|<code>" src/checker/ src/parser/`
2. **Find failing test files**: Run `bash conformance/run.sh --filter=<category> --print-test --max=10`
3. **Run individual file**: `/home/user/tsz/.target/release/tsz <test-file> 2>&1`
4. **Compare with tsc**: The conformance runner compares tsz output against cached tsc results in `conformance/.tsc-cache/`
5. **Fix the false positive**: Either tighten the condition that triggers the error, or suppress it in specific valid contexts
6. **Verify**: Re-run the failing tests to confirm they now pass

## Specific Investigation Leads

### TS2524 (12 tests, highest priority)
- Search: `grep -rn "2524\|await.*only\|AWAIT.*ASYNC" src/`
- Likely in `src/checker/` - the checker wrongly determines an async context isn't async
- All 12 tests are in async arrow function / function declaration categories
- Check how `is_in_async_context()` or equivalent works

### TS1042 (10 tests)
- Search: `grep -rn "1042\|ASYNC_MODIFIER\|modifier.*cannot" src/parser/`
- Parser modifier validation is too strict
- Check modifier validation in `src/parser/state_declarations.rs` or `src/parser/state.rs`

### TS2339 (11 solo + 8 partial)
- Property lookup fails for class members, especially with inheritance
- Check `get_property_of_type()` or equivalent in type computation
- May be related to how class instance types expose their members

### TS2445/TS2341 (8+6 tests)
- Protected/private access checks are too strict
- `src/checker/property_checker.rs:40-112` has the main logic
- `resolve_class_for_access()` in `src/checker/symbol_resolver.rs:1998` determines the class context
- Issue may be in `resolve_receiver_class_for_access()` or `is_class_derived_from()`

### Timeouts (4 tests)
- All are `classExtendsItself*.ts` variants
- Class extends itself causes infinite recursion in type checker
- Need cycle detection in class heritage resolution (likely `get_base_class_idx()`)

## Files Modified (Summary)

| File | Changes | Status |
|------|---------|--------|
| `src/binder.rs` | Added `PartialOrd, Ord` to `SymbolId` | Uncommitted |
| `src/checker/state_type_environment.rs` | Deterministic symbol ordering + ERROR cache fix | Uncommitted |
| `src/checker/control_flow.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker/parameter_checker.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker/state_checking.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker/state_checking_members.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker/type_checking_queries.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker/types/diagnostics.rs` | Pre-existing changes from main | Pre-existing |
| `src/checker_state_tests.rs` | Pre-existing changes from main | Pre-existing |
| `src/parser/state.rs` | Pre-existing changes from main | Pre-existing |
| `src/parser/state_declarations.rs` | Pre-existing changes from main | Pre-existing |
| `src/parser/state_expressions.rs` | Pre-existing changes from main | Pre-existing |
| `src/parser/state_statements.rs` | Pre-existing changes from main | Pre-existing |

The new changes (TS2304 fixes) are in `src/binder.rs` and `src/checker/state_type_environment.rs`. All other modified files have pre-existing changes from the main branch merge.
