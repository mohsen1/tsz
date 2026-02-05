# Session TSZ-6-3: Union/Intersection Member Resolution

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - Gemini Question 1 COMPLETE ✅
**Previous Session**: TSZ-4-3 (Enum Polish - COMPLETE)

## Context

TSZ-4 (Enum Nominality & Accessibility) is COMPLETE with all three phases finished:
- TSZ-4-1: Strict Null Checks & Lawyer Layer
- TSZ-4-2: Enum Member Distinction
- TSZ-4-3: Enum Polish

TSZ-6 Phases 1-2 are COMPLETE:
- Phase 1: Constraint-Based Lookup for TypeParameters
- Phase 2: Generic Member Projection for TypeApplications

This session continues TSZ-6 with Phase 3: Union/Intersection Member Resolution.

## Goal

Implement property access resolution for Union and Intersection types.

### Problem Statement

Currently, property access fails on composite types:
```typescript
type A = { x: string };
type B = { y: number };
type U = A | B;
type I = A & B;

const u: U = { x: 'hello' };
console.log(u.x); // Error: Property 'x' does not exist on type 'A | B'

const i: I = { x: 'hello', y: 42 };
console.log(i.x); // Should work, but may not resolve correctly
```

### Expected TypeScript Behavior

**Unions (A | B)**:
- Property exists only if in **ALL** constituents
- Result type is **Union** of property types
- Example: `(A | B).prop` exists only if both A and B have `prop`
- Result: `type(A.prop) | type(B.prop)`

**Intersections (A & B)**:
- Property exists if in **ANY** constituent
- Result type is **Intersection** of property types
- Example: `(A & B).prop` exists if either A or B has `prop`
- Result: `type(A.prop) & type(B.prop)` (may resolve to `never` if incompatible)

## Implementation Plan (from Gemini Question 1) - VALIDATED ✅

### Architecture: Solver-First - VALIDATED ✅

**CRITICAL**: Follow North Star Rule 2 - Use Visitor Pattern
- Do NOT match on `TypeKey::Union` or `TypeKey::Intersection` in Checker
- MUST use visitor pattern from `src/solver/visitor.rs`

**Gemini Validation**: "Your approach is correct and highly recommended. Moving this logic into a Visitor ensures recursion is handled centrally and North Star compliance is satisfied."

### File Locations (from Gemini)

| File | Function/Struct | Action |
|:---|:---|:---|
| `src/solver/visitor.rs` | `PropertyVisitor` (New) | Create new visitor for member resolution |
| `src/solver/operations_property.rs` | `PropertyAccessEvaluator` | Refactor to implement `TypeVisitor` |
| `src/solver/operations_property.rs` | `resolve_property_access_inner` | Replace `match key` with `self.visit_type(obj_type)` |

### Implementation Steps

**Step 1**: Refactor `PropertyAccessEvaluator` to implement `TypeVisitor`
- Move logic from `match` statements into `visit_*` methods
- Implement `visit_object`, `visit_array`, `visit_intrinsic`, etc.
- Ensure `visiting: FxHashSet<TypeId>` is checked before recursing

**Step 2**: Implement `visit_union` (All Must Have)
```rust
fn visit_union(&mut self, list_id: u32) -> Self::Output {
    let members = self.interner.type_list(TypeListId(list_id));
    for &member in members {
        let res = self.visit_type(self.interner, member);
        if res.is_not_found() {
            return PropertyAccessResult::PropertyNotFound; // Early exit
        }
    }
    // All members have the property - union the types
    self.merge_union_results(results)
}
```

**Step 3**: Implement `visit_intersection` (Any Can Have)
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
        type_id: self.interner.intersect(successes),
    }
}
```

**Step 4**: Handle `visit_lazy` for automatic recursion
- Resolver handles `Lazy` -> `Union` -> `Lazy` chains
- No manual `evaluate_type` calls needed

**Step 5**: Update `resolve_property_access_inner`
- Replace large `match key` block with `evaluator.visit_type(obj_type)`

### Edge Cases (from Gemini)

1. **Infinite Recursion**: Use `visiting` set before `visit_lazy`
2. **`any` in Unions**: `A | any` always succeeds (returns `any`)
3. **`unknown` in Unions**: Usually fails unless property common to all
4. **Optional Properties**: `{ x: string } | { x?: number }` → `string | number | undefined`
5. **Index Signatures**: Handle in intersections carefully

## Success Criteria

## Complexity Assessment

**Complexity: HIGH**

### Risks
1. **Recursive Types**: Unions/Intersections with `TypeKey::Ref` can cause infinite recursion
2. **Performance**: Large unions (100+ constituents) can cause O(n²) slowdowns
3. **Intersection Merging**: Getting `prop: string & number` wrong breaks object composition
4. **Rule 2 Violation**: Temptation to `match` on `TypeKey` directly in Checker

### Mitigations
- Use Solver's `cycle_stack` for recursion detection
- Memoize results in Solver layer
- Follow Two-Question Rule (AGENTS.md)
- Use visitor pattern religiously

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

### Verification
- [ ] Ask Gemini Question 1 (Approach Validation)
- [ ] Implement MemberCollector visitor
- [ ] Integrate into property resolution
- [ ] Ask Gemini Question 2 (Pro Review)
- [ ] Fix any bugs found by Gemini
- [ ] Add unit tests
- [ ] Run conformance suite
- [ ] Document results

## Dependencies

- TSZ-4-1: Strict Null Checks - COMPLETE ✅
- TSZ-4-2: Enum Member Distinction - COMPLETE ✅
- TSZ-4-3: Enum Polish - COMPLETE ✅
- TSZ-6 Phase 1-2: Member Resolution Basics - COMPLETE ✅
- Gemini Question 1: COMPLETE ✅ (approach validated)
- Gemini Question 2: PENDING (implementation review)

## Estimated Complexity

**HIGH** (10-15 hours)
- New visitor implementation
- Complex type merging logic
- Recursive type handling
- Multiple integration points
- Requires Two-Question Rule validation

## Next Steps (Immediate)

1. ✅ Ask Gemini Question 1 (Approach Validation) - COMPLETE
2. ⏸️ Implement `TypeVisitor` for `PropertyAccessEvaluator` - IN PROGRESS
   - Refactor large `match key` block into visitor methods
   - Implement `visit_union` and `visit_intersection`
   - Handle recursive types via `visit_lazy`
3. ⏸️ Ask Gemini Question 2 (Pro Review) - PENDING
4. ⏸️ Test and iterate - PENDING

**Current Status**: Analyzing `resolve_property_access_inner` function to understand current implementation before refactoring to visitor pattern.

**Implementation Complexity**: This is a LARGE refactoring that touches the core property resolution logic. The function has 200+ lines with complex match statements handling Object, Array, Union, Intersection, TypeParameter, Application, etc.

**Caution**: Given the scope and complexity, this refactoring should be done incrementally:
- First: Make `PropertyAccessEvaluator` implement `TypeVisitor` trait
- Then: Move existing logic into `visit_*` methods one type at a time
- Finally: Add `visit_union` and `visit_intersection` methods

**Risk**: Breaking existing property access functionality. Need comprehensive testing.

## Notes

- This is "Gold Standard" work for building the TSZ type system
- Builds on enum union checking knowledge from TSZ-4-3
- Isolated from TSZ-3 circular dependency issues
- Must follow Solver-First Architecture (North Star 3.1)
