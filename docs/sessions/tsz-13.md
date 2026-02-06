# Session TSZ-13: Foundational Cleanup & Index Signatures

**Started**: 2026-02-06
**Status**: 🔄 NOT STARTED
**Predecessor**: TSZ-12 (Cache Invalidation - Partial Complete)

## Task

Complete "almost done" features (Readonly infrastructure, Enum error counts) and implement Element Access Index Signatures for high impact.

## Problem Statement

### Task 1: Readonly & Enum Cleanup (~8 tests - Quick Wins)

**Readonly Infrastructure (~6 tests)**:
- Tests manually create `CheckerState` without loading lib files
- Getting error 2318 ("Cannot find global type") instead of 2540 ("Cannot assign to readonly")
- Readonly subtyping logic already fixed in tsz-11
- Need to fix test setup or emitter issues

**Tests affected**:
- `test_readonly_array_element_assignment_2540`
- `test_readonly_element_access_assignment_2540`
- `test_readonly_index_signature_element_access_assignment_2540`
- `test_readonly_method_signature_assignment_2540`
- `test_readonly_index_signature_variable_access_assignment_2540`
- (+1 more)

**Enum Types (~2 tests)**:
- Error count mismatches (expect 1 error, get 2)
- Nominal typing already works
- Likely diagnostic reporting issue in declarations

**Tests affected**:
- `test_cross_enum_nominal_incompatibility`
- `test_string_enum_cross_incompatibility`

### Task 2: Element Access Index Signatures (~3 tests - High Impact)

**Problem**: `obj[key]` lookup doesn't properly fall back to index signatures when no property matches.

**TypeScript behavior**:
1. First check for exact property match
2. If no match, check string index signature (`[x: string]: T`)
3. If no match, check number index signature (`[x: number]: T`)
4. Handle generic type parameters as keys (`T[K]`)

**Tests affected**:
- `test_checker_lowers_element_access_string_index_signature`
- `test_checker_lowers_element_access_number_index_signature`
- `test_checker_property_access_union_type`

## Expected Impact

- **Task 1**: Fix ~8 tests (6 readonly + 2 enum)
- **Task 2**: Fix ~3 tests + potential halo effect on other tests
- **Total**: +11 tests, aim for 8260+ passing
- **Categories**: Eliminate "Readonly" and "Enum" from failure list

## Files to Modify

### Task 1: Readonly & Enum
1. **src/tests/checker_state_tests.rs** - Fix test setup to use lib fixtures
2. **src/checker/declarations.rs** - Adjust enum diagnostic reporting
3. **src/emitter/types.rs** - Check if readonly keyword is missing

### Task 2: Index Signatures
1. **src/solver/evaluate.rs** - `evaluate_index_access` function
2. **src/solver/visitor.rs** - Ensure index signatures are traversed
3. **src/checker/expr.rs** - `check_element_access_expression`

## Implementation Plan

### Phase 1: Readonly & Enum Cleanup (Quick Wins)

1. Investigate readonly test failures
2. Fix test setup or add lib loading
3. Adjust enum error reporting to match tsc counts

### Phase 2: Index Signatures (Main Feature)

Ask Gemini Question 1 (Approach):
> "I need to implement Index Signature resolution in evaluate_index_access. When looking up a key in an object, if no property exists, how should I correctly fallback to string/number index signatures according to TS rules? Should this happen in the Solver or should the Checker provide the signature?"

Based on Gemini's guidance:
1. Implement index signature fallback logic
2. Handle generic type parameters as keys
3. Test with conformance tests

### Phase 3: Test and Commit

Run full test suite and commit fixes.

## Test Status

**Start**: 8247 passing, 53 failing
**Goal**: 8260+ passing, 45- failing

## Notes

**Gemini's Recommendation**:
- Index Signatures are foundational - required for `T[K]` expressions, Mapped Types, `keyof` operations
- Has "halo effect" on many other tests that rely on indexer lookups
- Quick wins first (Readonly/Enum) to clean up categories
- Then tackle high-impact Index Signatures

**Deferred Features**:
- Flow Narrowing: Requires CFG construction (deferred from tsz-10)
- Module Resolution: Path-mapping edge cases (lower priority)

**Rationale**:
- Complete "almost done" features to reduce technical debt
- Index signatures are more foundational than overload resolution
- Higher pass rate makes complex features easier to implement
