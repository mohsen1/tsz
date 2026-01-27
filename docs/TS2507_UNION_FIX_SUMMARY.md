# TS2507 Union Type Fix Summary

## Problem
False positive TS2507 errors ("Type is not a constructor function type") were occurring when using union types containing Application types (generic type aliases) in constructor expressions.

## Root Cause
In `get_construct_type_from_type`, the handling of Union types (line 1902) was not resolving Ref types or evaluating Application types before checking for construct signatures. This was inconsistent with:
- Intersection type handling (line 1827) - which evaluates Application types
- TypeParameter constraint handling (line 1869) - which evaluates Application types

## Solution
Modified the Union type case in `get_construct_type_from_type` to:
1. First resolve Ref types (type alias references) using `resolve_type_for_property_access`
2. Then evaluate Application types using `evaluate_application_type`
3. Finally check for construct signatures using `get_construct_signature_return_type`

## Code Changes
File: `src/checker/type_computation.rs`
Lines: 1899-1908

```rust
for &member in members.iter() {
    // Resolve Refs (type alias references) to their actual types
    let resolved_member = self.resolve_type_for_property_access(member);
    // Then evaluate any Application types
    let evaluated_member = self.evaluate_application_type(resolved_member);
    let construct_sig_return = self.get_construct_signature_return_type(evaluated_member);
    if let Some(return_type) = construct_sig_return {
        instance_types.push(return_type);
    } else {
        all_constructable = false;
        break;
    }
}
```

## Test Cases
### Valid Cases (Should NOT error):
```typescript
type Constructor1<T> = new (...args: any[]) => T;
type Constructor2<T> = new (...args: any[]) => T;
type UnionCtor = Constructor1<Base1> | Constructor2<Base2>;

function createUnion(ctor: UnionCtor, arg: any) {
    return new ctor(arg);  // Now works correctly
}

class A { constructor(public x: number) {} }
class B { constructor(public y: string) {} }
type AClass = typeof A;
type BClass = typeof B;
type UnionClass = AClass | BClass;

function createFromUnion(ctor: UnionClass, ...args: any[]) {
    return new ctor(...args);  // Now works correctly
}
```

### Invalid Cases (Should still error):
```typescript
type NonConstructible = string | number;
function test(ctor: NonConstructible) {
    return new ctor();  // Correctly errors: TS2507
}

type MixedUnion = typeof A | string;
function test2(ctor: MixedUnion) {
    return new ctor();  // Correctly errors: TS2507 (string not constructible)
}
```

## Impact
This fix reduces false positive TS2507 errors by handling complex union type compositions that include:
- Generic type aliases (Application types)
- Type references (Ref types)
- Nested constructor types

The fix brings union type handling in line with intersection and type parameter handling, ensuring consistent constructor type resolution across all complex type compositions.

## Related Work
- Previous fix (commit 01d35bbab): Enhanced `get_construct_signature_return_type` for Ref/TypeQuery types
- This commit: Extends Application type evaluation to Union types in `get_construct_type_from_type`

## Remaining Issues
Based on the initial analysis, other potential areas for TS2507 reduction:
1. Nested generic types - may need additional recursion handling
2. Intersection type caching - could improve performance
3. Object type with construct signatures - edge cases in interface definitions
