# Session tsz-3: Index Access Type Evaluation (`T[K]`)

**Started**: 2026-02-06
**Status**: Active - Planning Phase
**Predecessor**: tsz-3-complete (In operator, TS2339, infer keywords, Anti-Pattern 8.1)

## Task Definition

**Implement Index Access Type Evaluation** to enable resolving types like `User["id"]` to their actual property types.

This is a fundamental building block for advanced type logic. Without it, Mapped Types (like `Partial<T>`) and complex generic lookups cannot be resolved.

## Problem Statement

Currently, `tsz` can represent Index Access Types in the `TypeKey` enum but does not evaluate them. When encountering `User["id"]`, it should resolve to the actual type of the `id` property.

### Requirements
1. **Simple property lookups** on object types
2. **Index signature lookups** (`{ [key: string]: T }`)
3. **Union Distribution**: `T[K1 | K2]` → `T[K1] | T[K2]`
4. **Object Distribution**: `(T1 | T2)[K]` → `T1[K] | T2[K]`
5. **Deferred evaluation** for generic types

## Failing Test Case

```typescript
interface User {
    id: number;
    name: string;
    age?: number;
}

type T1 = User["id"];          // Expected: number
type T2 = User["id" | "name"]; // Expected: number | string
type T3 = User["age"];         // Expected: number | undefined

interface Dictionary {
    [key: string]: boolean;
}
type T4 = Dictionary["anyKey"]; // Expected: boolean
```

## Files to Modify

Per Gemini's recommendation:
1. **`src/solver/evaluate.rs`** - Primary file for `evaluate_index_access` logic
2. **`src/solver/mod.rs`** - Ensure routing to `evaluate_index_access`
3. **`src/solver/visitor.rs`** - Verify traversal of `IndexAccess` variants

## Implementation Approach

**Per MANDATORY Gemini Workflow, must ask Question 1 first:**

1. **Distribution First**: Handle unions in both `index_type` and `object_type`
2. **Property Lookup**: Check object properties, then index signatures
3. **Intrinsic Handling**: Handle primitives (`string["length"]` → `number`)
4. **Deferred Evaluation**: Return `IndexAccess` as-is for unresolved type parameters

## Next Step

Ask Gemini Question 1 (Approach Validation) before implementing.
