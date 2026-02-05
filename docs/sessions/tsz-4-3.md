# Session TSZ-4-3: Enum Polish

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - Ready for Implementation
**Previous Session**: TSZ-4-2 (Enum Member Distinction - COMPLETE)

## Context

TSZ-4-2 (Enum Member Distinction) is COMPLETE with 10/10 unit tests passing.
Conformance suite shows significant progress: 35/80 enum tests pass, only 5 missing TS2322 errors.

## Goal

Fix the 5 remaining missing TS2322 enum errors to achieve 100% enum nominal typing correctness.

## Investigation Results (2026-02-05) - COMPLETE ✅

### Identified Root Cause

**Failing Test**: `enumLiteralAssignableToEnumInsideUnion.ts`

**Test Case**:
```typescript
namespace X { export enum Foo { A, B } }
namespace Z { export enum Foo { A = 1 << 1, B = 1 << 2 } }
const e1: X.Foo | boolean = Z.Foo.A; // Should error TS2322 but doesn't
```

**Root Cause**:
- Source: `Z.Foo.A` (TypeKey::Enum with DefId for Z.Foo)
- Target: `X.Foo | boolean` (TypeKey::Union)
- The **Checker layer's** `enum_assignability_override` only handles cases where BOTH source and target are `TypeKey::Enum`
- When target is a Union, it falls through without checking if the union contains an enum with different DefId

**Key Discovery**:
Conformance tests use the **Checker layer** implementation in `src/checker/state_type_environment.rs`, NOT the Solver layer implementation in `src/solver/compat.rs`!

## Implementation Plan (Validated by Gemini)

### Location
`src/checker/state_type_environment.rs` in the `enum_assignability_override` function

### Approach
Add union checking logic in the `else` branch (when not both are TypeKey::Enum):

```rust
// Check if source is enum and target is a union containing an enum
if let Some(TypeKey::Enum(s_def, _)) = source_key {
    if let Some(TypeKey::Union(members)) = target_key {
        let member_list = self.ctx.types.type_list(members);

        // Check each constituent of the union
        for &member in member_list.iter() {
            if let Some(TypeKey::Enum(member_def, _)) = self.ctx.types.lookup(member) {
                if s_def != member_def {
                    // Nominal mismatch - reject!
                    return Some(false);
                }
            }
        }
        // All enum constituents match (or no enum constituents)
    }
}
```

### Edge Cases to Handle (from Gemini)
1. **Literal Enums**: Ensure `enum E { A = 1 }` where `1` is assignable to `E` still works
2. **The `any` hole**: `X.Foo | any` should still allow assignment (unless strict mode)
3. **Intersection Types**: `X.Foo & { brand: string }` should still reject `Z.Foo.A`
4. **Recursive Resolution**: Avoid triggering `cycle_stack` circular type resolution

## Success Criteria

- [ ] Implement union checking in Checker layer
- [ ] Test with `enumLiteralAssignableToEnumInsideUnion.ts` - should see TS2322
- [ ] Run full enum conformance suite - 0 missing TS2322 errors
- [ ] Ask Gemini Question 2 (Pro) for implementation review
- [ ] Fix any bugs found by Gemini review
- [ ] Final conformance verification

## Next Steps (When Resuming)

1. Implement the fix in `src/checker/state_type_environment.rs`
2. Build and test with `enumLiteralAssignableToEnumInsideUnion.ts`
3. Run `./scripts/conformance.sh run --filter "enum"` to verify all 5 errors fixed
4. Ask Gemini Question 2 (Pro): `./scripts/ask-gemini.mjs --pro --include=src/checker/state_type_environment.rs`
5. Fix any bugs identified by Gemini
6. Commit and document results

## Estimated Complexity

**LOW** (2-3 hours)
- Implementation is straightforward
- Clear test case to verify
- Gemini validation available
- Isolated from TSZ-3 circular dependency issues

## Dependencies

- TSZ-4-2 (Enum Member Distinction) - COMPLETE ✅
- Gemini consultation for approach - COMPLETE ✅
- Gemini Question 2 (Pro review) - PENDING

## Related Sessions

- **TSZ-4-1**: Strict Null Checks & Lawyer Layer - COMPLETE
- **TSZ-4-2**: Enum Member Distinction - COMPLETE
- **TSZ-4-3**: Enum Polish (THIS SESSION) - IN PROGRESS
- **TSZ-6 Phase 3**: Union/Intersection Member Resolution - NEXT AFTER THIS
- **TSZ-3**: CFA Narrowing - BLOCKED (expert Solver knowledge required)

## Notes

- This is the "last mile" for enum correctness
- Completing this provides stable foundation before TSZ-6 Phase 3
- Must follow Two-Question Rule per AGENTS.md
- Code is clean and ready for implementation

## Task 1: Debug Missing TS2322 Errors

### Problem
Conformance suite shows 5 missing TS2322 errors in enum tests.

### Investigation (2026-02-05)

**Identified specific test**: `enumLiteralAssignableToEnumInsideUnion.ts`

**Test case**:
```typescript
namespace X {
    export enum Foo { A, B }
}
namespace Z {
    export enum Foo {
        A = 1 << 1,
        B = 1 << 2,
    }
}
const e1: X.Foo | boolean = Z.Foo.A; // Should error: TS2322
```

**Root Cause Found**:
- Source: `Z.Foo.A` (TypeKey::Enum with DefId for Z.Foo)
- Target: `X.Foo | boolean` (TypeKey::Union)
- The **Checker layer's** `enum_assignability_override` only handles cases where BOTH source and target are `TypeKey::Enum`
- When target is a Union, it falls through without checking if the union contains an enum with different DefId

**Key Insight**:
Conformance tests use the **Checker layer** implementation in `src/checker/state_type_environment.rs`, NOT the Solver layer implementation in `src/solver/compat.rs`!

### Solution

Add union checking logic to the Checker layer's `enum_assignability_override`:

```rust
// In the else branch (when not both are TypeKey::Enum)
if let Some(TypeKey::Enum(s_def, _)) = source_key {
    if let Some(TypeKey::Union(members)) = target_key {
        // Check each constituent of the union
        for &member in member_list.iter() {
            if let Some(TypeKey::Enum(member_def, _)) = self.ctx.types.lookup(member) {
                if s_def != member_def {
                    // Nominal mismatch - reject!
                    return Some(false);
                }
            }
        }
    }
}
```

### Next Steps

1. Implement the fix in `src/checker/state_type_environment.rs`
2. Test with `enumLiteralAssignableToEnumInsideUnion.ts`
3. Run full enum conformance suite to verify all 5 missing errors are fixed
4. Remove debug output from Solver layer (not used by conformance)

## Task 2: TSZ-6 Phase 3 Initiation

After enum polish is complete, begin implementation of Union/Intersection Member Resolution.

### Implementation Approach (from Gemini)

**File**: `src/solver/operations.rs` or `src/solver/operations_property.rs`

**Logic for Unions** (A | B):
- Property exists only if in **all** constituents
- Result type is Union of property types
- Filter out `never` constituents
- Handle `any` propagation

**Logic for Intersections** (A & B):
- Property exists if in **any** constituent
- Result type is Intersection of property types
- Handle index signatures

### Critical: Avoid TSZ-3 Circular Dependencies

- Use `PropertyAccessGuard` to prevent infinite recursion
- Pass parent union/intersection type to guard, not individual constituents
- Be careful with "eager" type resolution that triggers cycles

## Success Criteria

### Task 1
- [ ] Identify all 5 missing TS2322 enum test cases
- [ ] Understand why enum_assignability_override isn't catching them
- [ ] Fix the bugs
- [ ] Verify with conformance suite (0 missing TS2322 errors)

### Task 2
- [ ] Implement `resolve_union_property_access`
- [ ] Implement `resolve_intersection_property_access`
- [ ] Add unit tests
- [ ] Verify with conformance suite

## Estimated Complexity

- **Task 1**: LOW (2-4 hours) - debugging and fixing edge cases
- **Task 2**: MEDIUM-HIGH (6-10 hours) - new implementation with careful handling of recursion

## Dependencies

- TSZ-4-2 (Enum Member Distinction) - COMPLETE
- TSZ-6 Phases 1-2 (Member Resolution for TypeParameters/Applications) - COMPLETE
- Gemini consultation for TSZ-6 Phase 3 Question 1 - COMPLETE

## Next Steps

1. Investigate the 5 missing TS2322 enum errors
2. Fix any bugs found
3. Document fixes and verify conformance
4. Begin TSZ-6 Phase 3 implementation
