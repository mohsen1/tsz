# Session TSZ-15: Indexed Access Types (T[K]) and keyof

**Started**: 2026-02-05
**Status**: ✅ COMPLETE
**Focus**: Investigate and verify indexed access types (`T[K]`) and `keyof T` operator implementation

## Problem Statement

TypeScript's indexed access types and `keyof` operator are fundamental features that need implementation:

### Feature 1: keyof Operator
```typescript
type Person = {
    name: string;
    age: number;
    location: string;
};

type PersonKeys = keyof Person; // "name" | "age" | "location"
```

### Feature 2: Indexed Access Type
```typescript
type Person = {
    name: string;
    age: number;
};

type Name = Person["name"]; // string
type Age = Person["age"];   // number
```

### Feature 3: keyof with Generics
```typescript
function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}
```

### Feature 4: Union of Property Access
```typescript
type Person = {
    name: string;
    age: number;
};

type PersonValues = Person[keyof Person]; // string | number
```

## Success Criteria

### Test Case 1: Basic keyof
```typescript
type Person = {
    name: string;
    age: number;
};

type Keys = keyof Person;
const k1: Keys = "name";  // Should work
const k2: Keys = "age";   // Should work
const k3: Keys = "xyz";   // Should error
```

### Test Case 2: Indexed Access
```typescript
type Person = {
    name: string;
    age: number;
};

type Name = Person["name"]; // Should be string
type Age = Person["age"];   // Should be number

const n: Name = "hello";     // Should work
const a: Age = 42;           // Should work
const x: Name = 42;          // Should error - number not assignable to string
```

### Test Case 3: keyof with Union Types
```typescript
type A = { x: number };
type B = { y: string };

type U = A | B;
type Keys = keyof U; // Should be never (no common keys)

// OR if using distributive keyof:
type Keys2 = keyof A | keyof B; // "x" | "y"
```

### Test Case 4: Array/Tuple Indexed Access
```typescript
type Arr = string[];
type ElementType = Arr[number]; // string

type Tuple = [number, string];
type First = Tuple[0]; // number
type Second = Tuple[1]; // string
type Third = Tuple[2]; // undefined
```

### Test Case 5: Generic Property Access
```typescript
function getProp<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

const person = { name: "Alice", age: 30 };
const name = getProp(person, "name"); // Should infer as string
const age = getProp(person, "age");   // Should infer as number
const invalid = getProp(person, "xyz"); // Should error
```

## Implementation Plan

### Phase 1: Understand TypeScript Semantics

**MANDATORY**: Ask Gemini PRE-implementation question:
```bash
./scripts/ask-gemini.mjs --include=src/solver "I am starting session tsz-15: Indexed Access Types.

I need to implement:
1) keyof T operator - returns union of property name literals
2) T[K] indexed access - looks up type of property K in type T
3) Generic constraint K extends keyof T
4) Special cases: arrays, tuples, unions, intersections

Questions:
1. What files should I modify for keyof operator?
2. How do I extract property names from an object type?
3. How do I implement indexed access type lookup?
4. What are the special rules for arrays/tuples?
5. How does keyof behave with union/intersection types?
6. Are there any existing patterns I should follow?

Please provide: file paths, function names, and implementation guidance."
```

### Phase 2: Implement keyof Operator

**Expected Location**: `src/solver/` (type computation)

**Tasks**:
1. Create new function for computing keyof
2. Extract property names from object types
3. Return union of string literal types
4. Handle special cases:
   - Arrays/tuples: return numeric literals
   - Unions: distribute over union members
   - Intersections: intersect the key sets
   - Primitives: return appropriate keys (e.g., string: number | toString | etc.)

**Expected TypeKey**: May need new `TypeKey` variant or use existing union/literal types

### Phase 3: Implement Indexed Access T[K]

**Expected Location**: `src/solver/` (type computation)

**Tasks**:
1. Create function for indexed access lookup
2. Resolve T to its base type (handle Lazy, Ref, etc.)
3. Resolve K to literal type(s)
4. Look up property type(s) in T
5. Handle union in K: return union of property types
6. Handle special cases:
   - Array/T[number]: return element type
   - Tuple types with numeric literals
   - Missing properties: return undefined or error

**Edge Cases**:
- K is union of literals: return union of all property types
- K is not a literal type: error or return unknown
- Property doesn't exist: return unknown or undefined

### Phase 4: Support Generic Constraints

**Expected Changes**: `src/solver/constraints.rs` or similar

**Tasks**:
1. Implement `K extends keyof T` constraint checking
2. Use keyof implementation to get valid keys
3. Check that K is subtype of keyof T
4. Use indexed access for `T[K]` in return types

### Phase 5: Integration with Lowering

**Expected Location**: `src/solver/lower.rs`

**Tasks**:
1. Handle `TypeKind::IndexedAccessType` in lowering
2. Lower `keyof` queries to appropriate TypeIds
3. Ensure proper caching and internment

### Phase 6: Testing

**Tasks**:
1. Add unit tests for keyof operator
2. Add unit tests for indexed access
3. Test with arrays, tuples, unions, intersections
4. Verify tsc compatibility
5. Run full test suite for regressions

## Architecture Considerations

### Solver vs Checker Split

**Keyof Operator**:
- Pure type computation → **Solver**
- Likely in `src/solver/operations.rs` or new `src/solver/indexed.rs`

**Indexed Access T[K]**:
- Pure type computation → **Solver**
- May need to handle Application types (generics)

**Generic Constraint K extends keyof T**:
- Constraint checking → **Solver**
- Likely in `src/solver/constraints.rs` or `src/solver/compat.rs`

### Visitor Pattern

Per NORTH_STAR.md 8.1, use visitor pattern for type resolution:
- Handle Lazy, Ref, Application types before extracting properties
- Use existing visitors from `src/solver/visitor.rs`

## MANDATORY Gemini Workflow

Per AGENTS.md and CLAUDE.md, **MUST ask Gemini TWO questions**:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --include=src/solver "I am starting session tsz-15: Indexed Access Types (T[K] and keyof).

I need to implement:
1) keyof T - returns union of property name literal types
2) T[K] - indexed access, returns type of property K in type T
3) Support for K extends keyof T in generic constraints
4) Special cases: arrays, tuples, unions, intersections

My planned approach:
1) Implement keyof as pure type computation in Solver
2) Implement indexed access T[K] in Solver
3) Handle generic constraints using keyof
4) Add lowering support for IndexedAccessType AST nodes

Questions:
1) What files should I create/modify?
2) Should I use existing TypeKey variants or create new ones?
3) How do I extract property names from object types?
4) What are the edge cases for arrays/tuples?
5) How does keyof behave with unions/intersections?

Please provide: file paths, function names, and implementation guidance."
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented indexed access types in [FILES].

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this correct for TypeScript's indexed access semantics?
2) Did I miss any edge cases?
3) Are there type system bugs?
4) Does keyof match tsc behavior?

Be specific if it's wrong - tell me exactly what to fix."
```

## Dependencies

None - this is a self-contained type system feature.

## Related Sessions

- **tsz-2**: Coinductive Subtyping (COMPLETE) - needed for property access subtyping
- **tsz-5**: Generic Type Inference (ACTIVE) - uses keyof for generic constraints
- **tsz-6**: Member Resolution (COMPLETE) - related property lookup patterns

## Related Issues

- Fundamental TypeScript feature required for many type patterns
- Prerequisite for mapped types, conditional types with keyof
- Required for advanced generic patterns

## Why This Session

**Gemini Pro Recommendation** (from tsz-14 deferral):

> "This is a good choice because:
> 1. **High Value:** Indexed access (`T[K]`) and `keyof` are fundamental TypeScript features required for many other types.
> 2. **Orthogonal:** This work is largely independent of the 'Contextual Typing' (`tsz-5`) and 'Control Flow' (`tsz-7`) workstreams, so you won't be blocked.
> 3. **Solver-Focused:** Like your successful work in `tsz-1`, this relies heavily on pure type computations in the Solver, which aligns with the North Star architecture."

## Session History

Created 2026-02-05 following completion of tsz-1 (Discriminant Narrowing) and deferral of tsz-14 (Literal Type Widening - blocked on contextual typing infrastructure from tsz-5).

## Investigation Findings (2026-02-05)

### Discovery: Feature Already Implemented

Following the same pattern as tsz-1, **Indexed Access Types and keyof are ALREADY FULLY IMPLEMENTED** in the codebase.

**Gemini Pro Response** revealed the existing implementation locations:
- `src/solver/evaluate_rules/keyof.rs` - 370 lines of comprehensive keyof handling
- `src/solver/evaluate_rules/index_access.rs` - 825 lines of indexed access implementation

### Implementation Coverage

**keyof Operator** (`evaluate_rules/keyof.rs`):
- ✅ Object types - returns union of property name literals
- ✅ Arrays - returns `number` + array methods
- ✅ Tuples - returns numeric indices + array methods
- ✅ Unions - implements distributive contravariance: `keyof (A | B) = keyof A & keyof B`
- ✅ Intersections - implements covariance: `keyof (A & B) = keyof A | keyof B`
- ✅ Primitives - returns prototype keys (e.g., `keyof string` includes "length", "charAt", etc.)
- ✅ `keyof any` - returns `string | number | symbol`
- ✅ `keyof unknown` - returns `never`
- ✅ Type parameters - defers evaluation or uses constraint

**Indexed Access Types** (`evaluate_rules/index_access.rs`):
- ✅ Object property access - `Person["name"]`
- ✅ Array element access - `T[number]`
- ✅ Tuple index access - `[string, number][0]`
- ✅ Union distribution - `T[A | B]` → `T[A] | T[B]`
- ✅ Optional properties - includes `undefined` when appropriate
- ✅ Index signatures - string/number index handling
- ✅ `noUncheckedIndexedAccess` flag support
- ✅ Generic constraints - `K extends keyof T` in function signatures

### Testing Results

**Test Files Created**:
1. `test_keyof_index.ts` - Basic keyof and indexed access - ✅ PASS
2. `test_keyof_index_errors.ts` - Error cases - ✅ PASS (catches correct errors)
3. `test_keyof_edge_cases.ts` - Union/intersection/tuples - ✅ PASS
4. `test_keyof_complex.ts` - Optional props, index sigs, readonly - ✅ PASS
5. `test_keyof_only.ts` - Discriminated unions, conditional types - ✅ PASS

**Compatibility with tsc**:
- All test cases match tsc behavior exactly
- Error messages are consistent with TypeScript
- No type system bugs found in keyof/indexed access implementation

**Note**: One discriminant narrowing issue was found in `test_keyof_discriminated.ts` but this is **NOT a keyof bug** - it's a separate narrowing issue unrelated to this session.

### Architecture Quality

The implementation follows excellent patterns:
- **Visitor Pattern**: Uses `IndexAccessVisitor`, `ArrayKeyVisitor`, `TupleKeyVisitor` from `visitor.rs`
- **Type Resolution**: Properly handles `Lazy`, `Ref`, `Application` types
- **Depth Limiting**: Uses `recurse_keyof` and `recurse_index_access` to prevent infinite recursion
- **Error Handling**: Graceful fallbacks and proper error types
- **Special Cases**: Comprehensive handling of arrays, tuples, primitives, unions, intersections

### Outcome

✅ **Session marked COMPLETE** - Indexed Access Types and keyof operator are fully implemented and working correctly. All success criteria from the session plan pass.

**No implementation work needed** - this was an investigation session similar to tsz-1, where the feature was discovered to be already implemented correctly.


