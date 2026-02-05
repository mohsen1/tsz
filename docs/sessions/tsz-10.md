# Session TSZ-10: Fix Discriminant Narrowing Regressions

**Started**: 2026-02-05
**Status**: Active
**Goal**: Fix 3 critical bugs in discriminant narrowing identified in AGENTS.md

## Problem Statement

From AGENTS.md evidence (2026-02-04 investigation):

Recent implementation of discriminant narrowing (commit `f2d4ae5d5`) introduced **3 critical bugs**:

1. **Reversed subtype check** - Asking `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - Not handling `Lazy`, `Ref`, `Intersection` types within narrowing logic
3. **Optional property failures** - Breaking on `{ prop?: "a" }` cases

**Impact**:
- Breaks type narrowing for discriminant properties
- Causes incorrect type inference in conditional branches
- Blocks valid TypeScript code from working correctly

## Technical Details

**Files**:
- `src/solver/narrowing.rs` - Discriminant narrowing implementation
- `src/solver/visitor.rs` - Type visitor infrastructure
- `src/solver/types.rs` - Type structures (Lazy, Ref, Intersection)

**Root Causes**:
- Subtype check arguments were reversed
- Type resolution not called before subtype checks
- Optional properties not handled in discriminant matching

## Implementation Strategy

### Phase 1: Test Cases (Pre-Implementation)
1. Create failing test cases demonstrating each bug
2. Add to `src/checker/tests/` or manual test file
3. Verify tests fail with current code

### Phase 2: Fix Bug #1 - Reversed Subtype Check
1. Locate the reversed subtype check in `narrowing.rs`
2. Reverse arguments: `is_subtype_of(literal, property_type)`
3. Add test to verify fix

### Phase 3: Fix Bug #2 - Missing Type Resolution
1. Add type resolution calls before subtype checks
2. Handle `TypeKey::Lazy(DefId)` - resolve to structural type
3. Handle `TypeKey::Ref(SymbolRef)` - resolve to definition
4. Handle `TypeKey::Intersection` - resolve all members
5. Add test to verify fix

### Phase 4: Fix Bug #3 - Optional Properties
1. Add optional property handling in discriminant matching
2. Test case: `{ type?: "stop", speed: number }`
3. Verify optional discriminants work correctly

### Phase 5: Validation
1. Run all tests to verify no regressions
2. Ask Gemini Pro to review implementation
3. Document fixes in session file

## Success Criteria

- [ ] Discriminant narrowing works for literal properties
- [ ] Type resolution handles Lazy/Ref/Intersection types
- [ ] Optional properties in discriminants work correctly
- [ ] All existing tests still pass
- [ ] No regressions introduced

## Session History

*Created 2026-02-05 after TSZ-9 encountered implementation complexity.*
*Recommended by Gemini as high-value, tractable task.*
*Focuses on fixing known regressions in localized code area.*
