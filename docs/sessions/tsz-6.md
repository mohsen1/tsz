# Session TSZ-6: Member Resolution on Generic and Placeholder Types

**Started**: 2026-02-05
**Status**: 🔄 Phase 1 Complete - Moving to Phase 2
**Focus**: Implement member resolution for Type Parameters, Type Applications, and Union/Intersection types

## Summary

This session implements **Member Resolution on Generic Types** to enable property and method access on placeholder types before type parameters are resolved.

### Problem Statement

**Current Bug**: Property access fails on generic types
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
    // Error: Property 'map' does not exist on type 'T[]'
}
```

**Root Cause**:
- When checking `arr.map(f)`, the type of `arr` is `T[]` (a `TypeKey::Array(T)`)
- `T` is a `TypeParameter` (not yet resolved)
- The Solver doesn't know how to project members from the global `Array<T>` interface onto a specific `TypeParameter`
- Result: Property lookup fails with "Property 'map' does not exist"

### Solution

Implement member resolution for generic types in three phases:

1. **Phase 1**: Constraint-Based Lookup (Type Parameters)
   - If `T extends { id: string }`, then `t.id` should succeed by looking at the constraint

2. **Phase 2**: Generic Member Projection (TypeApplications)
   - For `T[]`, find the `Array<U>` interface and substitute `U` with `T`
   - Result: `map<V>(callback: (val: T) => V): V[]`

3. **Phase 3**: Union/Intersection Member Resolution
   - Handle `(T | U).prop` - property must exist in all constituents
   - Handle `(T & U).prop` - property can exist in any constituent

## Implementation Plan

### Phase 1: Constraint-Based Lookup

**File**: `src/solver/operations.rs`

**Task**: Modify property lookup to handle TypeParameters by checking constraints

**Logic**:
1. In property/member lookup, check if base is a `TypeParameter`
2. If yes, check if it has a `constraint`
3. If yes, recurse into the constraint to find the property
4. If no constraint, fall back to `Object` members (TypeScript behavior)

**Status**: ✅ COMPLETE (2026-02-05)

**Implementation** (src/solver/operations_property.rs:856-872):
- Modified `resolve_property_access_inner` TypeParameter case
- If constraint exists: recurse into constraint
- If no constraint: fallback to `resolve_object_member` (Object members)
- Matches TypeScript behavior for unconstrained type parameters

**Test Results**:
- `getId<T extends { id: string }>(obj)` - ✅ Resolves from constraint
- `toString<T>(obj)` - ✅ Resolves Object.toString() method

**Commit**: `feat(solver): add Object fallback for unconstrained TypeParameters`

### Phase 2: Generic Member Projection

**Files**: `src/solver/instantiate.rs`, `src/solver/operations.rs`

**Task**: Implement member projection for TypeApplications

**Logic**:
1. When looking up `map` on `T[]`:
   - Recognize this as `Array<T>`
   - Find the global `Array<U>` interface
   - Find the `map` signature: `map<V>(callback: (val: U) => V): V[]`
   - Substitute `U` with `T` from the application
   - Result: `map<V>(callback: (val: T) => V): V[]`

2. Use existing `instantiate_type` or specialized `instantiate_signature`

**Status**: 🔄 REDIRECTED (2026-02-05)

**Progress Made**:
1. ✅ Modified `resolve_array_property` to create `Array<element_type>` Application
2. ✅ Modified `resolve_application_property` to handle Callable bases
3. ✅ Found the `map` property on the Array Callable
4. ⚠️ Type substitution blocked - identified root cause and new approach

**Gemini Redefined Approach** (2026-02-05):

The blocker is an "Eager vs. Lazy" issue. `instantiate_type` on the entire Callable exceeds recursion depth. The solution is **Lazy Member Instantiation**.

### Redefined Strategy

#### Task 2.1: Fix Interface Lowering Scope
**File**: `src/solver/lower.rs`
**Function**: `with_arena` (line 241), `lower_merged_interface_declarations` (line 261)

**Issue Found** (2026-02-05 via Gemini):
The `type_param_scopes.clone()` in `with_arena` creates a **copy** of the RefCell data, not a shared reference.

**Impact**:
- When `lower_merged_interface_declarations` processes `Array<T>`:
  1. Creates child lowerer via `with_arena(line 281)` with COPIED scope stack
  2. Pushes type param scope to `self` (line 291)
  3. Child lowerer collects type params (line 293)
  4. **BUG**: Child's scope changes are NOT visible to `self` or other children
  5. Methods lowered without `T` in scope → default to `Error`

**Fix Required**:
Wrap shared state in `Rc<RefCell<...>>` so `clone()` shares the underlying data.

**Changes Needed**:
1. Modify `TypeLowering` struct to use `Rc<RefCell<Vec<TypeParamScope>>>`
2. Update `with_arena` to use `Rc::clone()` instead of `RefCell::clone()`
3. Ensure all scope operations work with `Rc<RefCell<...>>`

**Reference Examples**:
- `lower_function_type` (line 545) uses `with_type_params` helper (line 441)
- Correctly wraps parameters/return types in push/pop block

#### Task 2.2: Implement Lazy Member Instantiation
**File**: `src/solver/operations_property.rs`
**Function**: `resolve_application_property`

**New Logic**:
1. Find property in CallableShape (e.g., `map`)
2. Get TypeSubstitution from Application args → type params
3. Call `instantiate_type` **ONLY on `prop.type_id`** (not the whole Callable)
4. This avoids recursing into other 37 Array methods

**Key Insight**: Don't instantiate the entire Callable. Instantiate just the property type we found.

#### Task 2.3: Handle `this` Types
**File**: `src/solver/operations_property.rs`
**Issue**: Array methods return `this` or `this[]`
**Fix**: Use `substitute_this_type` from `instantiate.rs` before returning result

### Potential Pitfalls
1. **Circular Refs**: `Array.map` returns `U[]` which is another Array Application. Ensure `PropertyAccessGuard` prevents cycles.
2. **Cached Errors**: If methods were cached as Error, need to fix lowering or clear cache.
3. **Callable vs Symbol Gap**: Callable doesn't know its TypeParameters. Need to get them from the Application's base.

**Next Implementation Step**:
Modify `resolve_application_property` to substitute only the found property's type, not the entire Callable.

**Commits**:
- `feat(solver): add Object fallback for unconstrained TypeParameters` (Phase 1)
- `feat(solver): Phase 2 progress - Array<T> member projection (blocked)`

### Phase 3: Union/Intersection Member Resolution

**File**: `src/solver/operations.rs`

**Task**: Handle property access on unions and intersections

**Logic**:
- **Unions**: `(T | U).prop` - property must exist in all constituents, result is union of types
- **Intersections**: `(T & U).prop` - property can exist in any constituent, result is union of found types

## Implementation Guidance (from Gemini Flash 2026-02-05)

### Correct Approach

**File**: `src/solver/operations_property.rs`

**Function**: `resolve_property_access_inner` (NOT `get_property_type`)

**Key Functions**:
- `resolve_property_access_inner` (main logic around line 641)
- `resolve_application_property` (handles TypeApplication)
- `resolve_array_property` (handles Array types, line 1018)
- `resolve_object_member` (handles Object members, line 968)

### Implementation Sequence

1. **TypeParameter Handling**:
   - In `resolve_property_access_inner`, handle `TypeKey::TypeParameter`
   - If `info.constraint` is `Some`, recurse into constraint
   - If `info.constraint` is `None`, fallback to `Object` members
   - Use `resolve_object_member` or ask resolver for global `Object` interface

2. **TypeApplication Handling**:
   - Use existing `resolve_application_property` logic
   - It already builds `TypeSubstitution` and calls `instantiate_type`
   - Ensure it handles `Lazy` bases correctly

3. **Array Type Handling**:
   - `Array` is a `TypeKey::Array(element_type)` (compiler-managed, not TypeApplication)
   - `resolve_array_property` (line 1018) already handles this
   - Ensure `resolver.get_array_base_type()` returns global `Array<T>` interface
   - Use `instantiate_generic` to map `T` to specific `element_type`
   - Then resolve property on instantiated interface

4. **This Type Substitution**:
   - Critical for fluent APIs (e.g., `class C { m(): this }`)
   - Use `substitute_this_type` from `src/solver/instantiate.rs` (line 538)
   - Substitute `ThisType` with receiver's type

### Edge Cases to Handle

- **Recursive Constraints**: `T extends U, U extends T` - PropertyAccessGuard handles this
- **Readonly Arrays**: `readonly T[]` - unwrap `ReadonlyType` before checking for `Array`
- **Numeric Indices**: `arr[0]` vs `arr["0"]` - use `is_numeric_index_name`
- **Infinite Expansion**: Guard against recursive type aliases with `enter_property_access_guard`

### Potential Pitfalls

- **Missing Resolver**: If `resolver.get_array_base_type()` returns `None`, have graceful fallback
- **`any` vs `error`**: Never return `TypeId::ANY` on failure - return `PropertyNotFound` or `IsUnknown`
- **Circular Constraints**: Use `constraint_pairs` pattern from `CallEvaluator` (line 114)

## Success Criteria

### Test Case 1: Array Method
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}
// Expected: Resolves Array.map() with val: T
```

### Test Case 2: Type Parameter Constraint
```typescript
function getId<T extends { id: string }>(obj: T) {
    return obj.id;
}
// Expected: Resolves 'id' property from constraint
```

### Test Case 3: Union Property
```typescript
function getProp<T extends { a: string }, U extends { a: number }>(x: T | U) {
    return x.a;
}
// Expected: Resolves 'a' as string | number
```

## Dependencies

- **tsz-5**: Multi-Pass Generic Inference (COMPLETE) - provides two-pass inference infrastructure
- **tsz-4**: Strict Null Checks & Lawyer Layer (COMPLETE) - provides constraint handling

## MANDATORY Gemini Workflow

Per AGENTS.md, **MUST ask Gemini TWO questions**:

### Question 1 (PRE-implementation) - REQUIRED
Before modifying `src/solver/operations.rs`:

```bash
./scripts/ask-gemini.mjs --include=src/solver/operations.rs --include=src/solver/instantiate.rs "
I am starting tsz-6: Member Resolution on Generic and Placeholder Types.
The goal is to make 'arr.map(f)' work when 'arr' is 'T[]'.

My planned approach:
1) Modify property lookup in Solver to handle TypeParameter by checking constraints.
2) Implement 'member projection' for TypeApplications (e.g., Array<T>) by substituting the interface's type parameters with the application's arguments.
3) Ensure the Checker's property access logic correctly calls these new Solver capabilities.

Questions:
1) What is the exact function in src/solver/operations.rs that handles property/member lookup?
2) How should I handle the substitution of interface members? Should I use the existing 'instantiate_type' or is there a specialized 'instantiate_signature'?
3) Are there pitfalls with 'this' types when resolving members on generic interfaces?
4) How does tsc handle member lookup on a naked TypeParameter with no constraint? (Does it default to Object members?)
"
```

### Question 2 (POST-implementation) - REQUIRED
After implementing changes:

```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/operations.rs --include=src/solver/instantiate.rs "
I implemented [FEATURE] in [FILE].

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this logic correct for TypeScript?
2) Did I miss any edge cases?
3) Are there type system bugs?

Be specific if it's wrong - tell me exactly what to fix.
"
```

## Related Sessions

- **tsz-5**: Multi-Pass Generic Inference (COMPLETE)
- **tsz-4**: Strict Null Checks & Lawyer Layer (COMPLETE)
- **tsz-2**: Coinductive Subtyping (COMPLETE)

## Session History

Created 2026-02-05 following completion of tsz-5 (Multi-Pass Generic Inference).
