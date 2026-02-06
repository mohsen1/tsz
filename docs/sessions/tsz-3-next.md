# Session tsz-3: Discriminant Narrowing for Optional Properties and Intersections

**Started**: 2026-02-06
**Status**: 🔄 READY TO START
**Predecessor**: Object Literal Freshness (Completed - implementation plan ready)

## Task Summary

Fix discriminant narrowing to support optional properties and intersection types. This is a high-priority correctness issue identified in AGENTS.md.

## Problem

Discriminant narrowing is failing for:
1. **Optional discriminants**: `{ kind?: 'circle' }` - the `?` makes narrowing fail
2. **Intersection discriminants**: `{ kind: 'a' } & { data: number }` - properties inside intersections
3. **Complex types**: Ref, Lazy wrappers not being resolved

## Why High Priority

Discriminant narrowing is core to TypeScript usability. False positives here make the compiler unusable for standard patterns like Redux and discriminated unions.

## Test Cases to Fix

### Optional Discriminants
```typescript
type Shape = { kind?: 'circle'; radius: number } | { kind: 'square'; side: number };
function test(s: Shape) {
    if (s.kind === 'circle') {
        s.radius; // Should narrow to first constituent
    }
}
```

### Intersection Discriminants
```typescript
type A = { kind: 'a' } & { data: number };
type B = { kind: 'b' } & { info: string };
function test(x: A | B) {
    if (x.kind === 'a') {
        x.data; // Should not error
    }
}
```

## Implementation Plan

### Files to Modify

1. **`src/solver/narrowing.rs`**
   - Function: `narrow_by_discriminant` (or visitor-based implementation)
   - Fix: Use `is_subtype_of(literal, property_type)` not reverse
   - Handle optional properties (undefined in union)

2. **`src/solver/visitor.rs`**
   - Ensure traversal unwraps Intersection, Ref, Lazy
   - Check all members of Intersection for discriminant

3. **`src/checker/flow_analysis.rs`**
   - Verify discriminant detection and passing to Solver

### Known Bug from AGENTS.md

From investigation dated 2026-02-04:
- **Reversed subtype check**: Code had `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
- **Missing type resolution**: Didn't handle Lazy/Ref/Intersection
- **Broken for optional properties**: Failed on `{ prop?: "a" }`

## Next Step

Ask Gemini Question 1 (Pre-implementation) to validate approach before coding.

## Test Files

- Create new test in `src/solver/tests/` or `src/checker/tests/`
- Check conformance tests for discriminant narrowing
