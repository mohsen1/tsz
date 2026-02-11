# Conformance Test Session - 2026-02-12

## Current State
- **Pass Rate**: 2,125/3,139 (67.7%) in slice 1/4
- **Total Tests**: 12,583
- **Slice 1 Range**: offset=0, max=3,146

## Analysis Results

### False Positives (Extra Errors We Emit)
Top offenders (fix = instant wins):
- **TS2322**: 85 tests (Type not assignable) - Many due to conditional type alias bug
- **TS2345**: 81 tests (Argument not assignable)
- **TS2339**: 60 tests (Property does not exist)
- **TS7006**: 26 tests (Parameter implicitly has any)
- **TS2349**: 16 tests

###Not Implemented Error Codes (Easiest to Add)
- **TS2323**: 9 tests - "Cannot redeclare exported variable" (duplicate export default)
- **TS2301**: 8 tests
- **TS1191**: 8 tests
- **TS7005**: 7 tests - "Variable implicitly has 'any' type"
- **TS7023**: 6 tests

### Missing Error Codes (Partial Implementation)
These codes work in some cases but need broader coverage:
- **TS2322**: missing in 56 tests
- **TS2304**: missing in 47 tests (Cannot find name)
- **TS2339**: missing in 30 tests
- **TS2300**: missing in 20 tests (Duplicate identifier)

## Investigation: Conditional Type Alias Bug

**Impact**: ~84 TS2322 false positives

**Root Cause**: TypeLowering creates `Lazy(def_id)` for ALL type alias references, but `resolve_lazy()` is never called during assignability checking.

**Status**: Documented in `docs/bugs/conditional-type-alias-lazy-resolution.md`

**Recommendation**: Defer until solver architecture matures. Requires changes to subtype/assignability checker to resolve Lazy types.

## Quick Wins for Next Session

### Option 1: Implement TS2323 (9 tests)
**What**: "Cannot redeclare exported variable"
**Where**: Duplicate `export default` declarations
**Complexity**: Medium
**Steps**:
1. Add TS2323 to `diagnostics.rs`
2. Track export default declarations in module checker
3. Emit error on duplicate
4. Write unit test

### Option 2: Implement TS7005 (7 tests)
**What**: "Variable implicitly has 'any' type"
**Where**: Variables initialized to `null`/`undefined` with noImplicitAny
**Complexity**: Low-Medium
**Issue**: We likely emit TS7010 instead of TS7005 for variables
**Steps**:
1. Find where implicit any errors are emitted
2. Distinguish variable vs parameter vs return type
3. Emit correct error code based on context

### Option 3: Fix TS1005 False Positives (42 tests)
**What**: Extra "'{' expected" parser errors
**Complexity**: Medium-High
**Issue**: Parser recovery emitting cascading errors
**Risk**: Could break working parser error recovery

## Recommendations

**For Next Session:**
1. **Start with TS2323** - Clear scope, 9 test impact
2. Write failing unit test first
3. Implement minimal fix
4. Verify with `cargo nextest run`
5. Re-run conformance slice
6. Commit and sync immediately

**Strategy:**
- Focus on NOT IMPLEMENTED codes (easier than fixing false positives)
- One error code at a time
- Always write unit tests first
- Commit after each working fix

## Lessons Learned

1. **Deep bugs take time**: Conditional type alias bug required 2+ hours of investigation
2. **Document complex bugs**: Save future investigation time
3. **Pick achievable targets**: Implementing new error codes > fixing deep false positives
4. **Unit tests are critical**: Don't code without tests
5. **Sync frequently**: Commit small, working changes

## Code Locations

### Diagnostics
- `crates/tsz-checker/src/types/diagnostics.rs` - Error code definitions

### Export Checking
- Search for "export" in checker
- Module declaration handling

### Implicit Any Checking
- Likely in variable declaration checking
- Currently emits TS7010, needs TS7005 for variables

### Parser Error Recovery
- Parser emits TS1005 cascading errors
- Needs investigation of recovery logic
