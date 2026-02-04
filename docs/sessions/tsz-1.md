# Session tsz-1

## Current Work

**Testing TS1005 parse error handling**

Testing parse error TS1005 "'{0}' expected" to verify parser correctly handles missing delimiters.

### Test Case: missingCloseParenStatements.ts
```typescript
} while (i < 5 && (a1 > 5);
```

**TSC errors at line 12:**
- TS1005: ')' expected. (position 35)

**tsz errors at line 12:**
- TS2304: Cannot find name 'i'. (position 18) - WRONG

**Analysis**: This is NOT a missing TS1005 issue. tsz is incorrectly emitting TS2304 for variable `i` which should be in scope from the `do` block. This is a symbol resolution issue, not a parse error issue.

**Other TS1005 cases in same file** (lines 3, 5, 9) - All emit TS1005 correctly ✅

This suggests TS1005 is working correctly for most cases. The missing TS1005 instances from conformance (13 missing) may be different edge cases.

### Test Case
```typescript
function A(): (public B) => C {}
```

**Expected errors (TSC):**
- TS2355 at (1,15) - function must return value ✅
- TS2369 at (1,16) - parameter property in wrong place ✅
- TS2304 at (1,29) - Cannot find name 'C' ❌ (missing in tsz)

### Root Cause (from Gemini analysis)
The `get_type_from_function_type` method in `src/checker/type_node.rs` delegates everything to `TypeLowering::lower_type()`, which:
- Computes the function signature type (Solver's job - WHAT)
- Does NOT emit diagnostics for child nodes (Checker's job - WHERE)

The Checker must explicitly walk the return type node to trigger TS2304 errors, similar to how type arguments are handled in `state_type_resolution.rs` lines 65-67:
```rust
// Explicit walk required to trigger diagnostics for children
for &arg_idx in &args.nodes {
    let _ = self.get_type_from_type_node(arg_idx);
}
```

### Fix Status - BLOCKED on Architecture

**Attempted fix in `src/checker/type_node.rs`** (commit 414469fb2) - INCOMPLETE

Added explicit walk of return type in `get_type_from_function_type()`:
```rust
if !func_data.type_annotation.is_none() {
    let _ = self.check(func_data.type_annotation);
}
```

**Why it doesn't work**:
- `self.check()` -> `compute_type()` -> `get_type_from_type_reference()` in `TypeNodeChecker`
- `TypeNodeChecker::get_type_from_type_reference()` delegates to `TypeLowering`
- `TypeLowering` computes types but doesn't emit diagnostics (by design)
- TS2304 emission happens in `CheckerState::get_type_from_type_reference()` (state_type_resolution.rs:140-141)
- Function types are NOT explicitly handled in `state_type_resolution.rs`

**Architecture Issue**:
- `TypeNodeChecker` is low-level - computes types, no diagnostics
- `CheckerState` is high-level - emits diagnostics like TS2304
- Function types need explicit handling in `CheckerState` to walk return types through diagnostic pipeline
- Currently function types fall through to default case which bypasses TS2304 emission

**Required Fix**: Add explicit function type handling in `state_type_resolution.rs` that:
1. Detects function type nodes
2. Explicitly walks the return type using `self.get_type_from_type_node()`
3. Then delegates to TypeLowering for the actual type computation

This is a non-trivial architectural fix requiring careful implementation.

### Priority Candidates (from session history)

1. **Parse Errors (42 missing total)**
   - TS1109 (Expression expected): missing=22
   - TS1055 ('{0}' expected): missing=11
   - TS1359 (Type identifier expected): missing=9

2. **Symbol Resolution (20 missing)**
   - TS2304 (Cannot find name): missing=11
   - TS2585 (Cannot find name, suggestion): missing=9

3. **Lib Context for ES5 Async (105 missing)**
   - TS2705: Need to verify lib context handling

### Verification Complete
- TS1136 fix verified with cargo run
- Conformance test baseline: 40/100 passed (quick test)
- All TS2304 unit tests pass (7/7)
- Parser test passes
- Ready for next task

---

## Conformance Results Summary

---

## Conformance Results Summary

### Error Mismatches (500 tests)
1. **TS2705** (missing=105): ES5 async functions require Promise - lib context handling
2. **TS1109** (missing=22): Expression expected - parse error
3. **TS2664** (missing=11): Module not found - module resolution
4. **TS1055** (missing=11): '{0}' expected - parse error
5. **TS2304** (missing=11): Cannot find name - binder symbol resolution
6. **TS1359** (missing=9): Type identifier expected - parse error
7. **TS2585** (missing=9): Cannot find name, did you mean? - binder
8. **TS2524** (missing=7): Abstract class issues - checker
9. **TS2654** (extra=6): Multiple default exports - false positive
10. **TS1042** (missing=6): async modifier cannot be used here

### Investigated Issues

#### TS1040 False Positive (Punted)
**Test**: `namespace M { async function f1() { } }`
- Expected: No errors (TypeScript accepts this)
- Actual: TS1040 emitted
- Root cause: Unable to identify - context flag logic appears correct but error still emitted
- Only affects async functions inside regular (non-declare) namespaces

#### TS2705 Investigation (Completed)
**Error**: "An async function or method in ES5 requires the 'Promise' constructor"
- Should be emitted when: target=ES5, async functions used, Promise not in lib
- Missing 105 times in conformance
- Tests examined have `es2015.promise` in lib, so TS2705 shouldn't emit
- Root cause: Need to find test WITHOUT Promise in lib to verify behavior

---

## Recommendations

### Priority 1: Parse Errors (42 missing total)
- **TS1109** (Expression expected): missing=22
- **TS1055** ('{0}' expected): missing=11
- **TS1359** (Type identifier expected): missing=9
- **Action**: Find specific failing tests, compare parser output with TSC

### Priority 2: Lib Context for ES5 Async (105 missing)
- **TS2705**: Need to verify lib context handling
- **Action**: Find test case with ES5 target + no Promise lib

### Priority 3: Symbol Resolution (20 missing)
- **TS2304** (Cannot find name): missing=11
- **TS2585** (Cannot find name, suggestion): missing=9
- **Action**: Investigate binder symbol resolution

---

## History (Last 20)

*2026-02-03 22:00 - Started conformance analysis, ran 500 tests, identified top issues*
*2026-02-03 23:30 - Investigated TS1040 bug, traced parser code, unable to identify root cause*
*2026-02-03 23:45 - Investigated TS2705, found tests include Promise in lib*
*2026-02-03 23:50 - Investigated parse errors, confirmed 42 missing parse errors*
*2026-02-04 02:00 - Fixed is_const compilation errors (collaborative with tsz-4)*
*2026-02-04 03:00 - Added TS1136 parser fix for invalid property names, test passes*
*2026-02-04 03:30 - Fixed fresh_type_param calls missing is_const argument*
*2026-02-04 04:00 - Investigated TS2304 emission: error_cannot_find_name_at NOT being called*
*2026-02-04 04:15 - Added filter in TypeDiagnosticBuilder::cannot_find_name - not working yet*
*2026-02-04 05:00 - Added debug logging - confirmed neither function is being called*
*2026-02-04 05:30 - **SOLVED**: TS1136 now correctly emitted instead of TS2304 for invalid property names. Added filters in error_reporter and solver diagnostics to skip obviously invalid identifiers.*

---

## Completed Work

### TS1136 vs TS2304 Fix (COMPLETED 2026-02-04)

**Problem**: Invalid property names like comma in `{ x: 0,, }` were emitting TS2304 instead of TS1136.

**Root Cause**:
- Parser correctly emits TS1136 for invalid property names
- Invalid identifier "," is added to AST for error recovery
- Type resolution processes "," and emits TS2304 through `error_cannot_find_name_at()`
- TS2304 error message obscures the more helpful TS1136 parse error

**Solution**:
1. Added filter in `error_cannot_find_name_at()` to skip emitting TS2304 for obviously invalid identifiers (single punctuation characters)
2. Added same filter in `TypeDiagnosticBuilder::cannot_find_name()` for consistency

**Verification**:
- Binary now shows: `error TS1136: Property assignment expected.` (correct)
- All TS2304 tests pass
- Parser test confirms TS1136 is emitted

---

## Punted Todos

- **TS1040 false positive**: Async functions in regular namespaces incorrectly flagged as ambient context. Requires deeper runtime debugging or more targeted Gemini queries with smaller context.
