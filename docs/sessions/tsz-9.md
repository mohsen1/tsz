# Session TSZ-9: Enum Type System

**Started**: 2026-02-06
**Status**: ✅ IN PROGRESS
**Predecessor**: TSZ-8 (Investigation - Conditional Types Already Done)

## Accomplishments

### Enum Arithmetic ✅ Fixed

**Problem**: `MyEnum.A + MyEnum.B` was emitting TS2362 errors instead of being recognized as valid arithmetic.

**Root Cause**: `NumberLikeVisitor`, `StringLikeVisitor`, and `BigIntLikeVisitor` in `src/solver/binary_ops.rs` didn't override `visit_enum()`, so they defaulted to `false`.

**Solution**: Added `visit_enum()` to each visitor to recurse into the enum's `member_type`:
```rust
fn visit_enum(&mut self, _def_id: u32, member_type: TypeId) -> Self::Output {
    self.visit_type(self.db, member_type)
}
```

**Impact**: +7 tests fixed (8225 → 8232 passing, 75 → 68 failing)
**Commit**: `76c33b4bd`

## Task

Fix **Enum Type System** - enum arithmetic, nominal typing, and cross-enum assignability.

## Problem Statement

Tests are failing for enum-specific behaviors:
1. **Enum arithmetic**: Operations like `enum + number`, `enum - enum`
2. **Nominal typing**: Different enum types are incompatible even with same values
3. **Cross-enum incompatibility**: String enums, numeric enums should not mix
4. **Open assignability**: Numeric enums assignable to number

## Expected Impact

- **Direct**: Fix ~7 enum-related tests
- **Type Safety**: Proper enum nominal typing
- **Compatibility**: Match TypeScript enum behavior

## Failing Tests

- `test_enum_arithmetic_valid`
- `test_cross_enum_nominal_incompatibility`
- `test_numeric_enum_open_and_nominal_assignability`
- `test_string_enum_cross_incompatibility`
- `test_string_enum_not_assignable_to_string`
- `test_enum_member_to_whole_enum`
- `test_numeric_enum_number_bidirectional`

## Implementation Plan

### Phase 1: Investigate Current State
1. Examine `src/solver/subtype.rs` - enum nominal typing checks
2. Review `src/solver/operations.rs` - enum arithmetic operations
3. Check `src/solver/types.rs` - TypeKey::Enum handling

### Phase 2: Implement Fixes
1. Fix enum arithmetic (enum + number, enum - enum)
2. Ensure nominal typing (different enums incompatible)
3. Fix cross-enum assignability (string vs numeric)
4. Verify open assignability (numeric enum → number)

### Phase 3: Test
1. Run all enum-related tests
2. Verify arithmetic operations
3. Check nominal type enforcement
4. Verify no regressions

## Files to Modify

- `src/solver/subtype.rs` - Enum nominal typing
- `src/solver/operations.rs` - Enum arithmetic
- `src/solver/compat.rs` - Enum assignability rules

## Test Status

**Start**: 8225 passing, 75 failing
**Current**: 8232 passing, 68 failing
**Result**: +7 tests fixed so far

## Remaining Work

Some enum tests still have issues (expect 1 error but get 2), likely due to:
- Cross-enum assignment reporting both member and type errors
- Need to investigate error reporting logic

## Implementation

### Phase 1: Enum Arithmetic ✅ Complete
- Added `visit_enum()` to NumberLikeVisitor
- Added `visit_enum()` to StringLikeVisitor
- Added `visit_enum()` to BigIntLikeVisitor
- Tests now pass

### Phase 2: Investigate Remaining Issues
- `test_cross_enum_nominal_incompatibility` - expects 1 error, gets 2
- `test_string_enum_cross_incompatibility` - expects 1 error, gets 2
- May need to deduplicate error reporting

## Related NORTH_STAR.md Rules

- **Rule 1**: Solver-First Architecture - Enum operations are pure type operations
- **Judge vs Lawyer**: Nominal typing is a Judge (structural) + Lawyer (nominality) split

## Next Steps

1. Investigate current enum implementation
2. Ask Gemini for approach validation (Question 1) - **CRITICAL**
3. Implement based on guidance
4. Ask Gemini for implementation review (Question 2)

## Note

**CRITICAL**: Enum behavior has subtle edge cases (const enums, union enums, ambient enums). Must ask Gemini for approach validation before implementing.
