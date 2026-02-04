# Session tsz-1

## Current Work

**Investigating TS1136 parse error for double comma in object literal**

Working on fixing the parser to emit TS1136 "Property assignment expected" for invalid property names like comma instead of TS2304 "Cannot find name ','."

### Progress
- Added `is_identifier_or_keyword()` check in `parse_property_name()` to emit TS1136 for invalid tokens
- Parser now correctly emits TS1136 (test passes)
- Binary still shows TS2304 - error is coming from solver type resolution, not error_reporter
- Need to find where in the solver the TS2304 is actually being created

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

---

## Investigation Notes: TS1136 vs TS2304

**Test Case**: `Boolean({ x: 0,, })`
- Expected: TS1136 "Property assignment expected"
- Actual: TS2304 "Cannot find name ','."

**Root Cause Identified**:
- Parser correctly emits TS1136 (test passes)
- `error_cannot_find_name_at()` is NOT being called for this case
- TS2304 is created elsewhere in the type resolution system
- Invalid identifier "," is added to AST for error recovery
- Type resolution tries to resolve "," and emits TS2304

**Attempts Made**:
1. ✅ Parser fix in `parse_property_name()` - emits TS1136
2. ❌ Check in `error_cannot_find_name_at()` - not called
3. ❌ Check in `error_cannot_find_name_with_suggestions()` - not called
4. ❌ Filter in `TypeDiagnosticBuilder::cannot_find_name()` - doesn't work

**Next Approach**:
Need to find where type resolution emits TS2304 and add filter there. The error bypasses error_cannot_find_name_at entirely.

---

## Punted Todos

- **TS1040 false positive**: Async functions in regular namespaces incorrectly flagged as ambient context. Requires deeper runtime debugging or more targeted Gemini queries with smaller context.
