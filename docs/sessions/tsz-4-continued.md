# Session TSZ-4-2: Enum Member Distinction

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
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

## Next Steps

1. Read current enum handling in `src/solver/compat.rs`
2. Implement enum member distinction
3. Test with conformance suite
4. Document results
