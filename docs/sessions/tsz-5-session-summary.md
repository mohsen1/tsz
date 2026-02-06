# Session tsz-5: Enum Type Resolution & Index Signatures

**Started**: 2026-02-06
**Status**: Starting
**Focus**: Fix enum type resolution and index signature handling

## Background

Session tsz-4 achieved solid progress:
- Fixed flow narrowing for computed element access (6 tests)
- Made partial progress on index access
- Overall: 504 → 511 passed, 39 → 32 failed

Per Gemini's recommendation, this session focuses on:
1. **Task #17**: Enum type resolution (6 failing tests) - Quick wins (~20% of failures)
2. **Task #18**: Index signature deep dive (2 failing tests) - Architectural fix

## Priority Tasks

### Task #17: Fix enum type resolution and arithmetic 🔥 (PRIORITY)

**6 Failing Tests:**
- arithmetic_valid_with_enum
- cross_enum_nominal_incompatibility
- numeric_enum_number_bidirectional
- numeric_enum_open_and_nominal_assignability
- string_enum_cross_incompatibility
- string_enum_not_assignable_to_string

**Gemini's Assessment:**
- High impact/low effort (quick wins)
- Likely single missing "unwrap" logic in Checker
- Should call Solver to resolve base type of enum for arithmetic/assignment
- Files to investigate: `src/checker/expr.rs`, `src/checker/type_checking.rs`

**Action Plan:**
1. Ask Gemini for approach validation (MANDATORY Two-Question Rule)
2. Find where enum base type resolution happens
3. Ensure Checker delegates to Solver for enum type operations

### Task #18: Index signature deep dive (SECONDARY)

**2 Failing Tests:**
- checker_lowers_element_access_string_index_signature
- checker_lowers_element_access_number_index_signature

**Problem:**
`interface StringMap { [key: string]: boolean }` accessed with `map["foo"]` returns `any` instead of `boolean`.

**Hypothesis:**
- Interface not lowered to ObjectWithIndex correctly
- evaluate_index_access receiving Ref type it can't "look through"
- Lowering issue in src/solver/lower.rs

**Files:** `src/solver/lower.rs`, `src/solver/evaluate_rules/index_access.rs`

## Starting Point

- Solver: 3544/3544 tests pass (100%)
- Checker: 511 passed, **32 failed**, 106 ignored
- Overall: Excellent progress, 32 failures remain

## Success Criteria

- Task #17: All 6 enum tests passing
- Task #18: Index signature tests passing
- Checker properly delegates to Solver for enums and index access
- Reduce failures below 30

## Next Steps

1. **Task #17**: Ask Gemini for enum approach validation
2. Implement enum fix (if straightforward)
3. **Task #18**: Deep dive into index signature lowering with tracing
