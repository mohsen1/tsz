# TS2540 False Positives Investigation

**Date:** 2026-01-27
**Error Code:** TS2540 - "Cannot assign to X because it is a read-only property"
**Impact:** 10,381 false positive errors
**Status:** Fixed ✅

## Problem Summary

The type checker was incorrectly emitting TS2540 errors for intersection types where a property was marked as `readonly` in some constituents but mutable in others.

## Root Cause

The `property_is_readonly` function in `src/solver/operations.rs` (line 3345-3350) used `.any()` for both union and intersection types:

```rust
Some(TypeKey::Union(types)) | Some(TypeKey::Intersection(types)) => {
    let types = interner.type_list(types);
    types
        .iter()
        .any(|t| property_is_readonly(interner, *t, prop_name))
}
```

This logic was incorrect for intersections.

## TypeScript's Readonly Property Rules

### Union Types (`A | B`)
A property is readonly if it's readonly in **ANY** constituent type.

**Example:**
```typescript
interface Base { readonly value: number; }
interface Mutable { value: number; }

declare let x: Base | Mutable;
x.value = 12; // Error TS2540 - conservative, since Base has it readonly
```

### Intersection Types (`A & B`)
A property is readonly **ONLY** if it's readonly in **ALL** constituent types.

**Example:**
```typescript
type Mixed = { readonly a: number } & { a: number };
declare let x: Mixed;
x.a = 2; // OK - one constituent is mutable

type AllReadonly = { readonly a: number } & { get a(): number };
declare let y: AllReadonly;
y.a = 2; // Error TS2540 - all constituents are readonly
```

## Test Cases

TypeScript conformance tests demonstrating the correct behavior:

1. **intersectionsAndReadonlyProperties.ts**
   - `{ readonly a: number } & { a: number }` → Assignment allowed
   - `{ get a(): number } & { set a(v: number) }` → Assignment allowed
   - `{ readonly a: number } & { get a(): number }` → Error TS2540

2. **unionTypeReadonly.ts**
   - `Base | Mutable` (where Base has readonly) → Error TS2540
   - Conservative approach for unions

3. **intersectionTypeReadonly.ts**
   - `Base & Identical` (both readonly) → Error TS2540
   - `Base & Mutable` (mixed) → Assignment allowed
   - `Base & DifferentType` (both readonly, different types) → Error TS2540

## Fix Implementation

Split the union and intersection handling to use different logic:

```rust
Some(TypeKey::Union(types)) => {
    // For unions: property is readonly if it's readonly in ANY constituent type
    let types = interner.type_list(types);
    types
        .iter()
        .any(|t| property_is_readonly(interner, *t, prop_name))
}
Some(TypeKey::Intersection(types)) => {
    // For intersections: property is readonly ONLY if it's readonly in ALL constituent types
    let types = interner.type_list(types);
    types
        .iter()
        .all(|t| property_is_readonly(interner, *t, prop_name))
}
```

## Note

The `IndexSignatureResolver.is_readonly` function in `src/solver/index_signatures.rs` already had the correct implementation using `.any()` for unions and `.all()` for intersections (lines 171-178). This fix brings `property_is_readonly` into alignment.

## Related Files

- `src/solver/operations.rs` (lines 3329-3353) - Main fix location
- `src/solver/index_signatures.rs` (lines 148-178) - Reference implementation
- `src/checker/state.rs` (lines 9895-9989) - Call site for readonly checking
- `src/checker/type_checking.rs` (lines 11088-11105) - Checker's is_property_readonly

## References

- TypeScript issue #17676 - Mapped type readonly behavior
- TypeScript issue #37823 - Readonly property assignment in constructors
- TypeScript Documentation: Understanding readonly modifier

## Expected Impact

Reduction from 10,381 TS2540 errors to <100 expected errors.
