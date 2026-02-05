# Session tsz-3: CFA Refinement & Advanced Features

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
**Previous Session**: tsz-3 Phase 1 (CFA Features - Complete)

## Goal

Implement advanced CFA features and unblock architectural issues to achieve 100% TypeScript parity.

## Context from Completed Session

Previous tsz-3 Phase 1 successfully delivered:
- ✅ Phase 1: Bidirectional Narrowing (x === y where both are references)
- ✅ Phase 2: Assertion Functions (asserts x is T)

**Session tsz-12 is now SUPERSEDED by this session** - all unique content has been merged.

---

## Phase 1: Nested Discriminants (🔄 ACTIVE - ARCHITECTURAL INVESTIGATION)

**Status**: 🟡 IN PROGRESS - ARCHITECTURAL INVESTIGATION

**Problem**: Support narrowing for nested discriminant paths like `action.payload.kind`.

**TypeScript Behavior**:
```typescript
type Action =
    | { type: 'UPDATE', payload: { kind: 'user', data: User } }
    | { type: 'UPDATE', payload: { kind: 'product', data: Product } };

function reducer(action: Action) {
    switch (action.payload.kind) {
        case 'user':
            return action.payload.data.name; // action.payload narrowed correctly
        case 'product':
            return action.payload.data.price;
    }
}
```

**Current Limitation**:
- `discriminant_property_info` only returns immediate parent property
- Need to recursively walk `PropertyAccessExpression` to build full path

**Implementation Plan**:
1. Modify `discriminant_property_info` to build `property_path: Vec<Atom>`
2. Update `narrow_by_discriminant` to handle paths of any length
3. Handle optional chaining (a?.b.c) in the path
4. Test with nested patterns 3-4 levels deep

**Root Cause from Previous Attempt**:
The check `if self.is_matching_reference(base, target)` prevents nested narrowing:
- For `action.payload.kind === 'item'`: `base` is `action`, `target` is `action.payload.kind`
- They are NOT the same reference, so discriminant guard is not created
- Removing the check broke other narrowing cases

**Requires**: AccessPath/FlowContainer abstraction or alternative approach

---

## Phase 2: Assertion Functions (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED & TESTED

**Merged from tsz-12** - Assertion functions integration already completed in previous tsz-3.

---

## Phase 3: Edge Case Fixes (⏸️ PENDING)

### 3.1: Zombie Freshness
**Issue**: Fresh object literals might lose freshness after narrowing.

**Investigation**: Check `narrow_by_discriminant` in `src/solver/narrowing.rs`. Ensure narrowed types preserve `ObjectFlags::FRESH_LITERAL`.

### 3.2: Truthiness of 0 and ""
**Issue**: `if (x)` where `x: string | number` should narrow to `string (excluding "") | number (excluding 0)`.

**Investigation**: Verify `narrow_by_truthiness` handles `0` and `""` correctly.

### 3.3: Narrowing `any`
**Issue**: `typeof x === "string"` where `x: any` should narrow to `string` within the block.

**Investigation**: Check `narrow_by_typeof` in `src/solver/narrowing.rs`. Currently returns `ANY` immediately (line ~687). Should narrow to the specific type.

**Previous Attempt**: Broke 5 circular extends tests when trying to narrow `any`
**Requires**: Investigation into circular extends errors before retrying

---

## Session Notes

This session combines the goals of tsz-12 with the architectural investigation needed to unblock Phase 1 of the new tsz-3.

**Key Principles**:
- Follow Two-Question Rule strictly for ALL solver/checker changes
- Pre-implementation investigation to avoid breaking existing tests
- Architectural understanding before code changes

**Files Modified in Previous Session**:
- `src/checker/control_flow.rs`: Flow context, CALL node handling
- `src/solver/narrowing.rs`: Subtype narrowing, Application type resolution
