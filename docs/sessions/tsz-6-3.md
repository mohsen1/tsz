# Session TSZ-6-3: Property Access Visitor Refactoring

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - Incremental Migration Strategy
**Previous Session**: TSZ-4-3 (Enum Polish - COMPLETE)

## Context

TSZ-4 (Enum Nominality & Accessibility) is COMPLETE with all three phases finished:
- TSZ-4-1: Strict Null Checks & Lawyer Layer
- TSZ-4-2: Enum Member Distinction
- TSZ-4-3: Enum Polish

TSZ-6 Phases 1-2 are COMPLETE:
- Phase 1: Constraint-Based Lookup for TypeParameters
- Phase 2: Generic Member Projection for TypeApplications

This session continues TSZ-6 with Phase 3, re-scoped as incremental visitor migration.

## Goal

Implement property access resolution for Union and Intersection types using Visitor Pattern (North Star Rule 2).

**Problem**: Current `resolve_property_access_inner` uses large `match` statements (anti-pattern) and cannot properly handle composite types.

**Solution**: Refactor to TypeVisitor pattern with incremental migration strategy.

## Strategy: Shadow Implementation (from Gemini)

**AVOID**: "Massive refactor" that breaks everything at once
**APPROACH**: Incremental migration via bridge pattern

### Migration Steps

**Step 1**: Define the Visitor
- Make `PropertyAccessEvaluator` implement `TypeVisitor` trait
- Create visitor methods for each type

**Step 2**: The Bridge
- Keep `resolve_property_access_inner` as entry point
- Instantiate visitor and call `visit_type` instead of direct match

**Step 3**: Variant-by-Variant Migration
- Move `TypeKey::Object` logic into `visit_object`
- Replace match arm with `visitor.visit_object(...)`
- Verify with tests
- Repeat for `Array`, `Intrinsic`, etc.

**Step 4**: Add New Visitors
- Implement `visit_union` (all must have)
- Implement `visit_intersection` (any can have)

### Benefits

- Low risk (one type at a time)
- Testable after each step
- No "big bang" breaking changes
- Builds toward North Star compliance

## Milestones

### Milestone 1: Foundation (LOW Complexity) ✅ COMPLETE
- Make `PropertyAccessEvaluator` implement `TypeVisitor` ✅
- Implement `visit_intrinsic` (trivial) ✅
- Create bridge in `resolve_property_access_inner` ✅
- **Completed**: 2026-02-05
- **Notes**: Used inline visitor pattern to avoid &mut self casting issues

### Milestone 2: Core Types (MEDIUM Complexity) ✅ COMPLETE
- Implement `visit_object` ✅
- Implement `visit_array` ✅
- Move Object/Array logic from match to visitor ✅
- **Completed**: 2026-02-05
- **Notes**: Refactored to implement TypeVisitor for &PropertyAccessEvaluator, added helper methods

### Milestone 3: Composite Types (HIGH Complexity) - IN PROGRESS

**Status**: Implementation plan ready, code location identified
**Estimated**: 4-6 hours
**Next Implementation**: visit_union and visit_intersection

**Implementation Location**:
- Union logic: src/solver/operations_property.rs lines 1010-1164 (TypeKey::Union match arm)
- Intersection logic: src/solver/operations_property.rs lines 1166-1265 (TypeKey::Intersection match arm)

**Implementation Strategy**:
1. Create helper methods (visit_union_impl, visit_intersection_impl) similar to Object/Array pattern
2. Implement TypeVisitor methods (visit_union, visit_intersection) that delegate to helpers
3. Update bridge pattern to dispatch to these visitors
4. Test with union/intersection property access cases

**Key Edge Cases** (from Gemini):
- Union short-circuits: `any` or `error` → immediate success
- Unknown filtering: Only return IsUnknown if ALL members are unknown
- All Must Have: PropertyNotFound if ANY member lacks the property
- Nullable partitioning: Separate valid results from nullable causes
- Index signature contagion: Propagate `from_index_signature` flag
- Union fallback: Check union-level index signatures if all members fail
- Intersection aggregation: Collect results from ALL members that have the property
- Implement `visit_object`
- Implement `visit_array`
- Move Object/Array logic from match to visitor
- **Estimated**: 3-4 hours

### Milestone 3: Composite Types (HIGH Complexity)
- Implement `visit_union` (All Must Have logic)
- Implement `visit_intersection` (Any Can Have logic)
- Handle recursive types via `visit_lazy`
- **Estimated**: 4-6 hours

## Implementation Plan (from Gemini Question 1)

### Code Skeletons

**visit_union (All Must Have)**:
```rust
fn visit_union(&mut self, list_id: u32) -> Self::Output {
    let members = self.interner.type_list(TypeListId(list_id));
    let mut results = Vec::new();

    for &member in members {
        let res = self.visit_type(self.interner, member);
        match res {
            PropertyAccessResult::Success { .. } => results.push(res),
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If ANY member lacks the property, the union lacks it
                return PropertyAccessResult::PropertyNotFound { ... };
            }
            _ => return res, // Propagate errors/unknown
        }
    }
    // Union of all property types
    self.merge_union_results(results)
}
```

**visit_intersection (Any Can Have)**:
```rust
fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
    let members = self.interner.type_list(TypeListId(list_id));
    let mut successes = Vec::new();

    for &member in members {
        let res = self.visit_type(self.interner, member);
        if let PropertyAccessResult::Success { type_id } = res {
            successes.push(type_id);
        }
    }

    if successes.is_empty() {
        return PropertyAccessResult::PropertyNotFound;
    }
    // Intersection of all found types
    PropertyAccessResult::Success {
        type_id: self.intersect_types(successes),
    }
}
```

## Edge Cases (from Gemini)

1. **Infinite Recursion**: Check `visiting` set before `visit_lazy`
2. **`any` in Unions**: `A | any` always succeeds
3. **`unknown` in Unions**: Usually fails unless property common to all
4. **Optional Properties**: `{ x: string } | { x?: number }` → `string | number | undefined`
5. **Index Signatures**: Handle in intersections carefully

## Success Criteria

### Test Cases

**Union Property Access**:
```typescript
type A = { x: string };
type B = { x: number };
type C = { y: boolean };
type U = A | B;  // Both have 'x'
type V = A | C;  // Only A has 'x'

const u: U = { x: 'hello' };
console.log(u.x); // ✅ OK, type is string | number

const v: V = { x: 'hello' };
console.log(v.x); // ❌ Error: Property 'x' does not exist on type 'A | C'
```

**Intersection Property Access**:
```typescript
type A = { x: string };
type B = { x: number };
type I = A & B;

const i: I = { x: 'hello' };
console.log(i.x); // ✅ OK, type is string & number
```

## Dependencies

- TSZ-4-1: Strict Null Checks - COMPLETE ✅
- TSZ-4-2: Enum Member Distinction - COMPLETE ✅
- TSZ-4-3: Enum Polish - COMPLETE ✅
- TSZ-6 Phase 1-2: Member Resolution Basics - COMPLETE ✅
- Gemini Question 1: COMPLETE ✅ (approach validated)
- Gemini Question 2: PENDING (post-implementation)

## Estimated Complexity

**Overall**: HIGH (10-15 hours)
- Milestone 1: LOW (2-3 hours)
- Milestone 2: MEDIUM (3-4 hours)
- Milestone 3: HIGH (4-6 hours)

## Next Steps (Immediate)

1. **Start Milestone 1**: Implement TypeVisitor trait for Intrinsic types
2. Create bridge pattern in resolve_property_access_inner
3. Test and verify
4. Continue to Milestone 2 (Object, Array)
5. Ask Gemini Question 2 (Pro Review) after Milestone 2

## Notes

**Why This Approach**:
- Avoids "massive refactor" risk
- Builds toward North Star compliance incrementally
- Testable after each milestone
- Unlocks Union/Intersection property access (architecturally mandatory)

**Key Files**:
- `src/solver/visitor.rs` (TypeVisitor trait)
- `src/solver/operations_property.rs` (PropertyAccessEvaluator)
- `src/solver/tests/` (unit tests)

**Alternative Tasks** (if needed):
- Literal Type Narrowing (CFA)
- Template Literal Types
- Mapped Type Evaluation
