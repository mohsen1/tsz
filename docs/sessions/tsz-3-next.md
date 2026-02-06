# Session tsz-3: Discriminant Narrowing - Implementation Details

**Started**: 2026-02-06
**Status**: ✅ READY TO IMPLEMENT
**Predecessor**: Object Literal Freshness (Completed)

## Task Summary

Fix discriminant narrowing for optional properties and intersection types.

## Gemini Validation (Question 1)

✅ **Approach is correct** but with specific implementation details.

### Key Insights

1. **Don't modify NarrowingVisitor** - DU narrowing is a "filtering" operation, not "intersecting"
2. **Use get_type_at_path** - Must be source of truth for property resolution
3. **Pass resolver** - Currently TODO at line 281 in PropertyAccessEvaluator

## Specific Functions to Modify

### 1. `src/solver/narrowing.rs::get_type_at_path`

**Changes needed:**
- Add `Intersection` handling - check all members for property
- Pass `resolver` into `PropertyAccessEvaluator` (TODO at line 281)
- Return `union2(property_type, TypeId::UNDEFINED)` for optional properties

### 2. `src/solver/narrowing.rs::narrow_by_discriminant`

**Changes needed:**
- Ensure correct order: `is_subtype_of(db, literal_value, prop_type)`
- For optional `prop?: "a"` (which is `"a" | undefined`):
  - `is_subtype_of("a", "a" | undefined)` = True → Keep member ✅

### 3. `src/solver/narrowing.rs::narrow_by_excluding_discriminant`

**Changes needed:**
- For intersections: if ANY part has property matching excluded literal, remove entire intersection
- Check: `is_subtype_of(prop_type, excluded_value)`
  - `is_subtype_of("a" | undefined, "a")` = False → Keep (could be undefined)
  - `is_subtype_of("a", "a")` = True → Exclude

## TypeScript Behaviors

### Intersections
- **Property merging**: `(A & B).prop` checks all members
- **Positive narrowing** (`===`): "Exists" logic - if ANY part matches, keep ALL
- **Negative narrowing** (`!==`): If ANY part matches excluded, remove ALL
- **Conflicting discriminants**: `type: "a" & "b"` = `never`

### Optional Properties
- `prop?: "a"` is effectively `prop: "a" | undefined`
- Property access must return `type | undefined`

## Implementation Table

| Feature | Function | Logic |
|---------|----------|-------|
| Type Resolution | `resolve_type` | Use fuel counter to unwrap Lazy/Application |
| Intersection Support | `get_type_at_path` | Handle TypeKey::Intersection, check all members |
| Optional Props | `get_type_at_path` | Return `type \| undefined` |
| DU Logic | `narrow_by_discriminant` | `is_subtype_of(literal, property_type)` |

## Potential Pitfalls

1. **Recursive Intersections**: `type T = { type: "a" } & T` - need cycle detection
2. **Missing Resolver**: Without resolver, type aliases fail
3. **Order of Operations**: Always `resolve_type` before checking TypeKey

## Test Cases

```typescript
// Optional discriminants
type Shape = { kind?: 'circle'; radius: number } | { kind: 'square'; side: number };
function test(s: Shape) {
    if (s.kind === 'circle') {
        s.radius; // Should narrow correctly
    }
}

// Intersection discriminants
type A = { kind: 'a' } & { data: number };
type B = { kind: 'b' } & { info: string };
function test(x: A | B) {
    if (x.kind === 'a') {
        x.data; // Should work
    }
}
```

## Next Steps

1. Implement `get_type_at_path` intersection handling
2. Pass resolver to PropertyAccessEvaluator
3. Handle optional properties (union with undefined)
4. Test and ask Gemini Question 2 (Review)
