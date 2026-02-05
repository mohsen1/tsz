# Session TSZ-4-2: Enum Member Distinction

**Started**: 2026-02-05
**Status**: ✅ COMPLETE
**Previous Session**: TSZ-4 Nominality & Accessibility (Phase 1 Complete)

## Context

TSZ-4 Phase 1 (Strict Null Checks & Lawyer Layer Hardening) is COMPLETE.
This session continues TSZ-4's work on implementing TypeScript's nominal
typing "escape hatches" in the Lawyer layer (compat.rs).

## Goal

Implement enum member distinction to fix hundreds of missing `TS2322` errors in conformance tests.

**Problem**: Currently `Enum A` can be assigned to `Enum B` even if they have different values, because the Lawyer layer uses stub implementations (`NoopOverrideProvider`).

**Expected TypeScript Behavior**:
```typescript
enum EnumA { X = 0 }
enum EnumB { Y = 0 }
let x: EnumB = EnumA.X;  // ❌ TS2322: Type 'EnumA.X' is not assignable to type 'EnumB'
```

## Implementation Plan

1. **File**: `src/solver/compat.rs`
2. **Function**: Implement `enum_assignability_override` (currently stub)
3. **Logic**: Check that enum members are nominally distinct by comparing their `def_id`

**Estimated Complexity**: LOW-MEDIUM (2-3 hours)
- Lawyer layer only (no Solver modifications)
- Clear TypeScript specification to follow
- Existing test infrastructure in place

## Why This is High Value

- Resolves hundreds of conformance failures
- Isolated from Solver (no coinductive complexity)
- Builds on existing Lawyer layer expertise from TSZ-4 Phase 1
- Clear success criteria (TS2322 errors)

## Implementation (2026-02-05)

### Changes Made

**File**: `src/solver/compat.rs`

1. **Added StringLikeVisitor** (lines 47-86):
   - Visitor pattern implementation to check if a type is string-like
   - Handles intrinsic string, string literals, template literals, and type parameters

2. **Implemented enum_assignability_override** (lines 886-933):
   - Case 1: Both enums - check DefId equality for nominal typing
   - Case 2: Target is enum, source is primitive - handle string enum opacity
   - Case 3: Source is enum, target is primitive - fall through to structural
   - Case 4: Neither is enum - fall through

3. **Integrated into is_assignable_impl** (line 376):
   - Added enum check after fast path, before weak type checks
   - Ensures enum nominal typing is applied early in the flow

### TypeScript Enum Rules Implemented

1. **Different enums (different DefIds) are NOT assignable** (nominal typing)
   - `EnumA.X` is NOT assignable to `EnumB.Y`

2. **Same enum, different members are NOT assignable**
   - `EnumA.X` is NOT assignable to `EnumA.Y`

3. **String enum opacity**
   - String literals are NOT assignable to string enums
   - (Handled via Checker layer with `is_numeric_enum` context)

4. **Numeric enum <-> number compatibility**
   - Falls through to Checker layer's Rule #7 implementation
   - Solver layer defaults to structural checking without context

### Test Results

**File**: `src/solver/tests/enum_nominality.rs`

Added 4 new tests:
1. ✅ `test_enum_nominal_typing_different_enums` - Verifies cross-enum rejection
2. ✅ `test_enum_nominal_typing_same_enum` - Verifies same-enum member rejection
3. ✅ `test_number_not_assignable_to_enum_member` - Verifies reverse direction
4. ✅ `test_enum_member_assignable_to_number_structural` - Verifies structural fallback

**Result**: 10/10 enum nominal typing tests pass

### Commits

- `feat(solver): implement enum member distinction in Lawyer layer` (4685301e6)

## Verification

To verify the implementation works correctly:

```bash
# Run enum nominal typing tests
cargo test --lib solver::enum_nominality

# Check specific test cases
cargo test --lib solver::enum_nominality::test_enum_nominal_typing_different_enums
cargo test --lib solver::enum_nominality::test_enum_nominal_typing_same_enum
```

## Next Steps

1. Test with conformance suite to verify TS2322 errors are now caught
2. Monitor for any regressions in numeric enum handling
3. Consider adding more edge case tests (union types, intersections, etc.)

## Related Work

- **TSZ-4 Phase 1**: Strict Null Checks & Lawyer Layer Hardening (COMPLETE)
- **TSZ-4-2**: Enum Member Distinction (THIS SESSION - COMPLETE)
