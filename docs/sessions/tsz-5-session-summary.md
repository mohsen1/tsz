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

## Progress (2026-02-06)

### Task #17: Enum Type Resolution - PARTIAL COMPLETE ✅

**Problem Solved:**
- Enum members (`E.A`) are now assignable to their parent enum type (`E`)

**Solution Implemented:**
1. **TypeEnvironment enum parent tracking** (`src/solver/subtype.rs`):
   - Added `enum_parents: HashMap<u32, DefId>` field to track member->parent relationships
   - Added `register_enum_parent(member_def_id, parent_def_id)` method
   - Added `get_enum_parent(member_def_id)` method
   - Implemented `get_enum_parent_def_id` for `TypeResolver` trait

2. **Enum parent registration** (`src/checker/state_type_analysis.rs`):
   - Register enum parent relationships when enum member types are computed
   - Populate mapping in `type_env` during type caching

3. **CheckerContext symbol_to_def mapping** (`src/checker/context.rs`):
   - Implemented `symbol_to_def_id` for `CheckerContext` (was missing!)
   - This enables looking up DefIds from SymbolRefs in type resolution

4. **Binder parent tracking** (`src/binder/state_binding.rs`):
   - Set `sym.parent = enum_sym_id` for enum members (already done)

5. **CompatChecker member-to-parent handling** (`src/solver/compat.rs`):
   - Added `(Some(sp), None)` case to handle member->parent assignments
   - Returns `Some(true)` when `t_def == sp` (target is parent enum)
   - Falls through to structural check for union enum types

**Fixed Tests:**
- ✅ test_cross_enum_nominal_incompatibility (E1.A -> E1 now works)
- ✅ test_string_enum_cross_incompatibility (S1.A -> S1 now works)
- ✅ test_enum_member_to_whole_enum (member -> whole enum now works)

**Still Failing:**
- ❌ test_numeric_enum_number_bidirectional
- ❌ test_numeric_enum_open_and_nominal_assignability
- ❌ test_string_enum_not_assignable_to_string
- ❌ test_number_literal_to_numeric_enum_type
- ❌ test_number_to_numeric_enum_type

**Current Status:**
- Checker: 513 passed, **30 failed**, 106 ignored
- Progress: 511 → 513 passed, 32 → 30 failed
- 3 enum tests now passing

**Files Modified:**
- `src/solver/subtype.rs`: Added enum parent tracking infrastructure
- `src/solver/compat.rs`: Handle member-to-parent assignability
- `src/checker/context.rs`: Implemented `symbol_to_def_id`
- `src/checker/state_type_analysis.rs`: Register enum parent relationships
- `src/binder/state_binding.rs`: Set parent symbol for enum members

**Commit:** a399321d7 "feat(tszz-11): fix enum member-to-parent assignability"

## Next Steps

1. **Task #17**: Investigate remaining 5 enum test failures (numeric enum bidirectional, string enum opacity)
2. **Task #18**: Deep dive into index signature lowering with tracing
