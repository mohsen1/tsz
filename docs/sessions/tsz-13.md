# Session TSZ-13: Foundational Cleanup & Index Signatures

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS - Investigation Phase
**Predecessor**: TSZ-12 (Cache Invalidation - Complete)

## Task

Complete "almost done" features (Readonly infrastructure, Enum error counts) and implement Element Access Index Signatures for high impact.

## Investigation Findings

### Enum Error Duplication (~2 tests)

**Tests affected**:
- `test_cross_enum_nominal_incompatibility`
- `test_string_enum_cross_incompatibility`

**Issue**: Tests expect 1 TS2322 error but get 2
- Code: `let e2: E2 = e1;` where e1 is type E1, e2 is declared as E2
- tsc produces: 1 error (correct)
- tsz produces: 2 errors (duplicate)

**Investigation**:
- Error is reported in `src/checker/state_checking.rs:445` in `check_variable_declaration`
- Only one error reporting call found in the code path
- Loop in `check_variable_statement` calls `check_variable_declaration` once per declaration
- **Root cause**: Unknown - requires deeper debugging of duplicate diagnostic reporting

**Status**: Minor diagnostic deduplication issue, not a type system bug. Enum nominal typing works correctly (fixed in tsz-9).

### Readonly Infrastructure (~6 tests)

**Tests affected**:
- `test_readonly_array_element_assignment_2540`
- `test_readonly_element_access_assignment_2540`
- `test_readonly_index_signature_element_access_assignment_2540`
- `test_readonly_method_signature_assignment_2540`
- `test_readonly_index_signature_variable_access_assignment_2540`
- (+1 more)

**Issue**: Tests get error 2318 ("Cannot find global type") instead of 2540 ("Cannot assign to readonly")
- Root cause: Tests manually create `CheckerState` without loading lib files
- Readonly subtyping logic already fixed in tsz-11
- **Fix needed**: Update test setup to use lib fixtures

**Status**: Test infrastructure issue, straightforward to fix.

## Remaining Work

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

## Test Status

**Start**: 8247 passing, 53 failing
**Current**: 8247 passing, 53 failing
**Goal**: 8260+ passing, 45- failing

## Notes

**Session Progress**:
- Investigated enum error duplication (found diagnostic issue, not type system bug)
- Identified readonly test infrastructure fix path
- Ready to implement index signatures (high impact feature)

**Gemini's Recommendation**:
- Index Signatures are foundational - required for `T[K]` expressions, Mapped Types, `keyof` operations
- Has "halo effect" on many other tests that rely on indexer lookups

**Next Steps**:
1. Fix readonly infrastructure tests (quick wins)
2. Implement index signatures (high impact)
3. Address enum error duplication if time permits
