# Session TSZ-6-3: Union/Intersection Member Resolution

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
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

## Implementation Plan (from Gemini)

### Architecture: Solver-First

**CRITICAL**: Follow North Star Rule 2 - Use Visitor Pattern
- Do NOT match on `TypeKey::Union` or `TypeKey::Intersection` in Checker
- MUST use visitor pattern from `src/solver/visitor.rs`

### File Locations

1. **Visitor**: `src/solver/visitor.rs` (new visitor for member collection)
2. **Operations**: `src/solver/operations_property.rs` (integration)
3. **Testing**: `src/solver/tests/` (unit tests)

### Implementation Steps

**Step 1**: Create `MemberCollector` visitor in `src/solver/visitor.rs`
- Extract all properties from a type
- For Unions: Intersect the property sets
- For Intersections: Union the property sets
- Handle `TypeKey::Ref` and recursive types

**Step 2**: Integrate into `resolve_property_access_inner`
- Use visitor instead of direct TypeKey matching
- Preserve existing logic for Object, Array, etc.

**Step 3**: Handle result type merging
- Union properties: union the types
- Intersection properties: intersect the types
- Handle conflicts (e.g., `string & number` = `never`)

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
- Gemini Question 1: PENDING
- Gemini Question 2: PENDING

## Estimated Complexity

**HIGH** (10-15 hours)
- New visitor implementation
- Complex type merging logic
- Recursive type handling
- Multiple integration points
- Requires Two-Question Rule validation

## Next Steps (Immediate)

1. **MANDATORY**: Ask Gemini Question 1 (Approach Validation)
2. Create `MemberCollector` visitor skeleton
3. Implement union member collection
4. Implement intersection member collection
5. Integrate into property resolution
6. Ask Gemini Question 2 (Pro Review)
7. Test and iterate

## Notes

- This is "Gold Standard" work for building the TSZ type system
- Builds on enum union checking knowledge from TSZ-4-3
- Isolated from TSZ-3 circular dependency issues
- Must follow Solver-First Architecture (North Star 3.1)
