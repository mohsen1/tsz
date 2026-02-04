# Session tsz-3 - Type System Bug Fixes

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Individual diagnostic and type checking fixes

## Context

Previous session (tsz-3-control-flow-analysis) completed:
- instanceof narrowing
- in operator narrowing
- Truthiness narrowing verification
- Tail-recursive conditional type evaluation fix
- Investigation revealed discriminant narrowing is fundamentally broken (archived for future work)

## Current Task: Abstract Mixin Intersection TS2339

### Problem Statement

Test `test_abstract_mixin_intersection_ts2339` fails with unexpected TS2339 errors.

**Test Case**:
```typescript
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => IMixin) {
    abstract class MixinClass extends baseClass implements IMixin {
        mixinMethod() {}
    }
    return MixinClass;
}

class DerivedFromConcrete extends Mixin(ConcreteBase) {
}
wasConcrete.baseMethod(); // TS2339: Property 'baseMethod' does not exist
wasConcrete.mixinMethod(); // TS2339: Property 'mixinMethod' does not exist
```

### Investigation Findings

1. **TypeId(152)** is the type of `DerivedFromConcrete`
2. It has `ObjectShapeId(4)` which doesn't include `baseMethod` or `mixinMethod`
3. **First error**: `'{ new (args: any): MixinClass }'` is not assignable to `'TBaseClass & { new (args: any): error }'`
   - This suggests the heritage clause resolution isn't correctly handling function calls that return constructor types

### Root Cause (Hypothesis)

When extending a function call (`extends Mixin(ConcreteBase)`), the heritage clause resolution:
1. Evaluates `Mixin(ConcreteBase)` to get a constructor type
2. Should merge properties from both `TBaseClass` and the mixin interface
3. Currently not correctly merging the base class properties into the derived class type

### Files to Investigate

- `src/checker/class_inheritance.rs` - Class hierarchy analysis
- `src/checker/herititage*.rs` - Heritage clause resolution
- Property resolution for intersection types

### Complexity

This is a **complex issue** requiring deep knowledge of:
- Generic function instantiation
- Constructor type resolution from function calls
- Intersection type property merging
- Class heritage clause processing

This may be too complex for a quick fix and might require architectural changes.
