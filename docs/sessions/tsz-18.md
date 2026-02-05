# Session TSZ-18: Conformance Testing & Bug Fixing

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Find and fix actual bugs in implemented features through focused testing

## Problem Statement

Recent sessions discovered that many "missing" features are already implemented:
- tsz-15: Indexed Access Types (370 + 825 lines)
- tsz-16: Mapped Types (755 lines)
- tsz-17: Template Literals (229 lines)

However, **"implemented" ≠ "correct"**. AGENTS.md shows that even recently implemented features (like discriminant narrowing) had critical bugs.

## Strategy

Per Gemini Pro recommendation: "Since you know where the code lives for Mapped Types, Indexed Access, and Template Literals, your most valuable contribution is proving they actually work."

**Approach**:
1. Create comprehensive test cases for each feature
2. Run against both tsz and tsc
3. Identify discrepancies
4. Fix bugs found
5. Document fixes

## Focus Areas

### Area 1: Indexed Access Types (tsz-15)
**Location**: `src/solver/evaluate_rules/keyof.rs`, `src/solver/evaluate_rules/index_access.rs`

**Test Categories**:
- Basic keyof and indexed access
- Union distribution edge cases
- Array/tuple indexed access
- Generic constraint handling
- noUncheckedIndexedAccess flag

### Area 2: Mapped Types (tsz-16)
**Location**: `src/solver/evaluate_rules/mapped.rs`

**Test Categories**:
- Partial, Required, Pick, Record
- Array/tuple preservation
- Key remapping with `as` clause
- Modifier operations (+?, -?, +readonly, -readonly)
- Homomorphic mapped types

### Area 3: Template Literals (tsz-17)
**Location**: `src/solver/evaluate_rules/template_literal.rs`

**Test Categories**:
- Union expansion and Cartesian products
- Literal type conversion
- Expansion limits
- Mixed literal types
- Template literal type inference

## Success Criteria

### Criterion 1: Test Coverage
- [ ] Create 50+ test cases for indexed access types
- [ ] Create 50+ test cases for mapped types
- [ ] Create 30+ test cases for template literals
- [ ] Document all test cases with expected vs actual behavior

### Criterion 2: Bug Discovery
- [ ] Find at least 5 bugs in indexed access implementation
- [ ] Find at least 5 bugs in mapped type implementation
- [ ] Find at least 3 bugs in template literal implementation

### Criterion 3: Bug Fixes
- [ ] Fix all discovered bugs
- [ ] All fixes pass tsc comparison
- [ ] No regressions in existing functionality

### Criterion 4: Documentation
- [ ] Document each bug found
- [ ] Document fix approach
- [ ] Add regression tests

## Session History

Created 2026-02-05 following completion of tsz-15, tsz-16, tsz-17 which all found existing implementations. Following Gemini Pro recommendation to shift from "investigation" to "validation and fixing".

## Progress

### 2026-02-05: Session Pivoted (Per Gemini Pro Guidance)
**Original Plan**: Create focused test cases and manually compare tsz vs tsc
**Problem**: TypeScript conformance suite not checked out, manual testing insufficient

**New Plan** (Per Gemini Recommendation):
1. Initialize TypeScript submodule: `git submodule update --init TypeScript`
2. Run targeted conformance tests on implemented features
3. Find ACTUAL failures (not theoretical bugs)
4. Fix bugs using mandatory Gemini workflow
5. Break the "already implemented" discovery loop

**Status Update**:
- Attempted to initialize TypeScript submodule - not configured in .gitmodules
- TSC cache exists (12,399 results, 88.7% pass rate = 754 failing tests!)
- Cannot run conformance tests without TypeScript/tests/cases directory
- Need alternative approach: create specific high-value test cases

**Next Step**: Ask Gemini for specific, high-value edge case test cases for keyof, mapped types, and template literals. Run against both tsz and tsc to find discrepancies.

## Next Steps

1. Create comprehensive test suite for Indexed Access Types
2. Compare tsz vs tsc results
3. Identify and categorize bugs
4. Fix bugs systematically
5. Repeat for Mapped Types and Template Literals

## Dependencies

- **tsz-15**: Indexed Access Types (COMPLETE) - testing this implementation
- **tsz-16**: Mapped Types (COMPLETE) - testing this implementation
- **tsz-17**: Template Literals (COMPLETE) - testing this implementation

## Related Sessions

- **tsz-15**: Indexed Access Types (COMPLETE) - now validating for correctness
- **tsz-16**: Mapped Types (COMPLETE) - now validating for correctness
- **tsz-17**: Template Literals (COMPLETE) - now validating for correctness
