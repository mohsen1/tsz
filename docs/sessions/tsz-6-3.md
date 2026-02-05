# Session TSZ-6-3: Property Access Visitor Refactoring

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - Milestones 1-2 Complete, Milestone 3 Ready for Implementation
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

### Milestone 3: Composite Types (HIGH Complexity) - READY FOR IMPLEMENTATION

**Status**: Gemini consultation complete, implementation validated
**Estimated**: 4-6 hours
**Next Implementation**: visit_union and visit_intersection

**Gemini Validation Results** (from Question 1):
- ✅ Logic confirmed correct: Union = "All Must Have", Intersection = "Any Can Have"
- ✅ Edge cases clarified (any, error, never, unknown handling)
- ✅ Index signature flag behavior defined
- ✅ Helper functions identified: `self.interner.union()` and `self.interner.intersection()`

**Implementation Location**:
- Union logic: src/solver/operations_property.rs lines 1010-1164 (TypeKey::Union match arm)
- Intersection logic: src/solver/operations_property.rs lines 1166-1265 (TypeKey::Intersection match arm)

**Detailed Edge Cases** (from Gemini):

**Unions**:
- `any` in union → immediate success (returns `any`)
- `error` in union → immediate success (returns `error`)
- `never` → filter out (empty union, but shouldn't happen in practice)
- `unknown` → PropertyNotFound UNLESS property is on Object prototype (toString, etc.)
- `null`/`undefined` → collect into PossiblyNullOrUndefined result
- `from_index_signature`: CONTAGIOUS (if ANY member uses index, flag is true)

**Intersections**:
- `never` in intersection → return `never` immediately
- Collect results from ALL members that have the property
- `from_index_signature`: RESTRICTIVE (true ONLY if all members used index signatures)
- If no named properties found, check union-level index signatures as fallback

**Implementation Steps**:
1. Create helper methods (visit_union_impl, visit_intersection_impl)
2. Implement TypeVisitor methods (visit_union, visit_intersection)
3. Update bridge pattern to dispatch to these visitors
4. Use `self.interner.union()` and `self.interner.intersection()` for final types
5. Handle PropertyAccessGuard for recursion protection
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

**For Next Session - Continue with Milestone 3 Implementation**:

1. **Implement visit_union_impl** (extract from lines 1010-1164):
   - Handle any/error short-circuits
   - Filter unknown members (only return IsUnknown if ALL are unknown)
   - Iterate members, collect valid_results and nullable_causes
   - Return PropertyNotFound if ANY member lacks property
   - Handle union-level index signatures as fallback
   - Return Union of all valid_results

2. **Implement visit_intersection_impl** (extract from lines 1166-1265):
   - Iterate members, collect ALL Success results
   - Track nullable_causes and saw_unknown
   - Handle intersection-level index signatures if no results
   - Return Intersection of all found types

3. **Add to TypeVisitor**:
   - Implement `visit_union(&mut self, list_id: u32) -> Self::Output`
   - Implement `visit_intersection(&mut self, list_id: u32) -> Self::Output`
   - Delegate to helper methods

4. **Update Bridge Pattern**:
   - Add Union and Intersection to the match dispatch
   - Remove old match arms once visitors are working

5. **Test and Verify**:
   - Test union property access (all members have property)
   - Test union property access (some members lack property)
   - Test intersection property access
   - Test edge cases: any, error, unknown, never

6. **Mandatory Gemini Question 2** (Pro Review):
   ```bash
   ./scripts/ask-gemini.mjs --pro --include=src/solver/operations_property.rs "I implemented visit_union and visit_intersection for PropertyAccessEvaluator.
   [PASTE CODE HERE]
   Please review: 1) Does this correctly handle property distribution in unions? 2) Does it correctly intersect property types in intersections? 3) Are there edge cases with optional properties or 'any' I missed?"
   ```

**Estimated Time**: 4-6 hours for implementation + Pro review

**Session Handoff Notes**:
- Foundation is solid (Milestones 1-2 complete)
- Gemini Question 1 validation complete
- All edge cases documented
- Code locations clearly identified
- Ready for immediate implementation

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
