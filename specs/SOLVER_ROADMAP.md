# Solver Implementation Roadmap

**Goal:** Build a complete, sound TypeScript type solver with proper understanding of type system mechanics.

**Approach:** Mathematical foundation first, then advanced features, then conformance validation.

**Philosophy:** Understand deeply, implement correctly, validate with tests.

---

## Table of Contents

1. [Foundation: Type System Mechanics](#1-foundation-type-system-mechanics)
2. [Core Subtyping Rules](#2-core-subtyping-rules)
3. [Type Operations](#3-type-operations)
4. [Advanced Features](#4-advanced-features)
5. [Inference System](#5-inference-system)
6. [Compatibility Layer](#6-compatibility-layer)
7. [Testing & Validation](#7-testing--validation)

---

## 1. Foundation: Type System Mechanics

### 1.1 Semantic Subtyping (Already Implemented)

**Status:** ✅ Core architecture is sound

**Theory:** Types are sets of values. Subtyping is set inclusion.

- `string` is supertype of `"hello"` (all literal strings are strings)
- `number | string` is supertype of `number` (union adds more values)
- `{ x: number }` is subtype of `{ x: number, y?: string }` (fewer required properties)

**Key References:**
- `specs/SOLVER.md` - Mathematical foundation
- `src/solver/subtype.rs` - Subtyping implementation

**Tasks:**
- [ ] Audit completeness of semantic subtyping rules
- [ ] Verify all primitive types have proper set relationships
- [ ] Test edge cases: `never`, `unknown`, `any`

### 1.2 Coinductive Semantics for Recursive Types

**Status:** ⚠️ Partially implemented, needs review

**Theory:** Recursive types use coinduction (greatest fixed point) to allow infinite types.

```typescript
interface TreeNode {
  value: number;
  left: TreeNode | null;
  right: TreeNode | null;
}
```

**Key Challenge:** Infinite expansion must terminate through structural equality.

**Tasks:**
- [ ] Implement proper occurs-check for recursive type detection
- [ ] Ensure structural equality handles recursion correctly
- [ ] Test mutually recursive interfaces
- [ ] Validate type inference in recursive contexts

### 1.3 Structural Typing & Canonicalization

**Status:** ✅ Core exists, needs completeness

**Theory:** Types are compared by structure, not name. Canonicalization ensures O(1) equality.

```typescript
interface A { x: number }
interface B { x: number }
// A and B are the same type (structural)
```

**Key Implementation:**
- `src/solver/interner.rs` - Type interning for canonicalization
- `TypeId` - O(1) equality checks

**Tasks:**
- [ ] Ensure all constructed types go through interner
- [ ] Verify canonicalization handles all type constructors
- [ ] Test that structural equivalence works for objects, functions, tuples

---

## 2. Core Subtyping Rules

### 2.1 Primitive Subtypes

**Status:** ✅ Mostly implemented

**Hierarchy:**
```
any
├── unknown
│   ├── never (bottom)
│   ├── void
│   ├── undefined
│   ├── null
│   ├── number
│   ├── bigint
│   ├── string
│   ├── boolean
│   ├── symbol
│   └── object (non-primitives)
```

**Tasks:**
- [ ] Verify `any` is top (subtype of everything)
- [ ] Verify `never` is bottom (supertype of everything)
- [ ] Ensure `unknown` behaves correctly (top for safe types)
- [ ] Test implicit any vs explicit any differences

### 2.2 Object Subtyping

**Status:** ⚠️ Partially implemented

**Rules:**
1. Property-wise subtyping: each source property ≤ target property
2. Source may have **extra** properties (excess property checks)
3. Optional properties: required ≤ optional
4. Readonly variance: covariant

```typescript
// Source is subtype of Target (extra property y is OK)
type Source = { x: number; y: string };
type Target = { x: number };

// Source is NOT subtype of Target (y is required in source)
type Source = { x?: number };
type Target = { x: number };
```

**Tasks:**
- [ ] Implement excess property checks for object literals
- [ ] Handle optional property variance correctly
- [ ] Support readonly property variance (covariant)
- [ ] Handle call signatures and construct signatures
- [ ] Implement index signature subtyping

### 2.3 Function Subtyping

**Status:** ⚠️ Partially implemented

**Variance:**
- Parameters: **Bivariant** (TS unsoundness, see compat layer)
- Return type: Covariant
- This: Bivariant

```typescript
// Parameters are bivariant (unsound!)
type Handler = (x: string) => void;
type Handler2 = (x: "hello") => void;
// Handler2 ≤ Handler AND Handler ≤ Handler2 (both ways)
```

**Tasks:**
- [ ] Implement parameter bivariance
- [ ] Implement return type covariance
- [ ] Handle rest parameters correctly
- [ ] Support optional parameters
- [ ] Handle this parameters
- [ ] Implement function type overloading resolution

### 2.4 Array and Tuple Subtyping

**Status:** ⚠️ Partially implemented

**Rules:**
- Arrays are covariant: `Array<number>` ≤ `Array<number | string>`
- Tuples: fixed-length, pairwise subtyping
- Readonly arrays: stricter subtyping rules

```typescript
// Covariant
type A = number[];
type B = (number | string)[];
// A ≤ B

// Tuple subtyping
type T1 = [number, string];
type T2 = [number, string, boolean]; // T1 is NOT subtype of T2
```

**Tasks:**
- [ ] Implement array covariance
- [ ] Implement tuple length checking
- [ ] Handle tuple element subtyping
- [ ] Support readonly array covariance
- [ ] Handle rest elements in tuples
- [ ] Implement labeled tuple element checking

---

## 3. Type Operations

### 3.1 Union Types

**Status:** ✅ Core implemented

**Rules:**
- `A | B` contains values from both A and B
- Subtyping: `A ≤ C` and `B ≤ C` implies `A | B ≤ C`
- Distribution over some operations

**Tasks:**
- [ ] Ensure union normalization (remove duplicates, flatten)
- [ ] Handle union subtyping correctly
- [ ] Implement union type narrowing (discriminated unions)
- [ ] Support implicit union in object types

### 3.2 Intersection Types

**Status:** ⚠️ Partially implemented

**Rules:**
- `A & B` contains values that are both A and B
- Primitives intersect to `never`
- Objects merge properties
- Functions intersect to overload types

```typescript
// Primitive intersection
type A = string & number; // never

// Object intersection
type B = { x: number } & { y: string }; // { x: number; y: string }

// Function intersection (overloading)
type F = ((x: string) => void) & ((x: number) => void);
```

**Tasks:**
- [ ] Implement primitive intersection to `never`
- [ ] Implement object property merging
- [ ] Handle conflicting property types
- [ ] Implement function intersection as overloads
- [ ] Handle method intersection correctly

### 3.3 Type Aliases and Resolution

**Status:** ✅ Basic implementation

**Tasks:**
- [ ] Ensure recursive type aliases are handled correctly
- [ ] Support circular type references
- [ ] Implement type alias expansion during checking
- [ ] Cache expanded types for performance

---

## 4. Advanced Features

### 4.1 Conditional Types

**Status:** ❌ Not implemented

**Syntax:**
```typescript
type Check<T> = T extends string ? "string" : "other";
```

**Behavior:**
- Distribute over unions by default
- Naked type parameters for non-distributive behavior
- Can infer types using `infer` keyword

**Tasks:**
- [ ] Implement `extends` checking in conditionals
- [ ] Handle true/false branches
- [ ] Implement union distribution
- [ ] Support naked type parameters
- [ ] Implement `infer` type extraction
- [ ] Handle recursive conditional types

### 4.2 Mapped Types

**Status:** ❌ Not implemented

**Syntax:**
```typescript
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Partial<T> = { [P in keyof T]?: T[P] };
```

**Modifiers:**
- `readonly`, `?` (optional)
- `+` / `-` for adding/removing modifiers
- Key remapping via `as`

**Tasks:**
- [ ] Implement `keyof T` type computation
- [ ] Implement property iteration
- [ ] Handle modifier mapping
- [ ] Support key remapping with `as`
- [ ] Implement homomorphic mapped types (preserve structure)
- [ ] Handle template literal key types

### 4.3 Template Literal Types

**Status:** ❌ Not implemented

**Syntax:**
```typescript
type EventName<T extends string> = `on${Capitalize<T>}`;
type Click = EventName<"click">; // "onClick"
```

**Operations:**
- String concatenation in types
- Built-in string manipulators: `Uppercase`, `Lowercase`, `Capitalize`, `Uncapitalize`
- Union distribution over interpolation
- Inference from template literals

**Tasks:**
- [ ] Implement template literal parsing
- [ ] Handle string interpolation in types
- [ ] Implement string manipulation utilities
- [ ] Support union distribution in templates
- [ ] Handle template literal type inference
- [ ] Implement template literal pattern matching

### 4.4 Decorators and Metadata

**Status:** ❌ Not implemented

**Note:** This is lower priority as it's an ECMAScript feature, not core type system.

---

## 5. Inference System

### 5.1 Generic Type Inference

**Status:** ⚠️ Partially implemented

**Mechanisms:**
- Infer type arguments from function call arguments
- Infer from return type usage
- Infer from context (variable declaration, etc.)
- Constraint solving for generic constraints

```typescript
function id<T>(x: T): T { return x; }
const x = id(42); // T inferred as number
```

**Tasks:**
- [ ] Implement argument-to-parameter type inference
- [ ] Implement return type inference
- [ ] Handle contextual typing
- [ ] Implement constraint propagation
- [ ] Support default type arguments
- [ ] Handle partial inference (some type args explicit, some inferred)

### 5.2 Conditional Type Inference

**Status:** ❌ Not implemented

**Mechanism:** Infer type parameters within `infer` clauses in conditional types.

```typescript
type UnpackPromise<T> = T extends Promise<infer U> ? U : T;
type P = UnpackPromise<string | Promise<number>>; // string | number
```

**Tasks:**
- [ ] Implement `infer` keyword parsing
- [ ] Handle inference in conditional true branch
- [ ] Support multiple `infer` in same conditional
- [ ] Handle infer with constraints (`infer U extends string`)
- [ ] Implement inference variance checking

### 5.3 `this` Type Inference

**Status:** ❌ Not implemented

**Tasks:**
- [ ] Implement polymorphic `this` type
- [ ] Handle `this` in class methods
- [ ] Support `this` parameters in functions
- [ ] Handle `this` type narrowing in class hierarchies

---

## 6. Compatibility Layer

### 6.1 TypeScript Unsoundness Catalog

**Status:** ⚠️ Partially implemented

**Purpose:** TypeScript has intentional unsound behaviors for ergonomics. The compat layer implements these.

**Key Unsound Behaviors:**
1. **Bivariant function parameters** (see `TS_UNSOUNDNESS_CATALOG.md`)
2. **Optional parameters looseness**
3. **Weak types** (all optional properties)
4. **Excess property checks** (object literals only)
5. **Assignability to empty interfaces**
6. **Enum assignability**
7. **Catch clause parameters** (any vs unknown)

**Tasks:**
- [ ] Audit compat module against catalog
- [ ] Ensure all unsound rules are option-driven
- [ ] Implement bivariant parameter checking
- [ ] Implement weak type checks
- [ ] Implement excess property checks
- [ ] Handle enum assignability rules
- [ ] Implement catch clause parameter handling

### 6.2 Compiler Options

**Status:** ⚠️ Partially implemented

**Key Options:**
- `strict` - Enable all strict options
- `noImplicitAny` - Disallow implicit any
- `strictNullChecks` - Distinguish null/undefined
- `strictFunctionTypes` - Disable parameter bivariance
- `strictPropertyInitialization` - Check class property init
- `exactOptionalPropertyTypes` - Disallow undefined in optional
- `noUncheckedIndexedAccess` - Add undefined to indexed access

**Tasks:**
- [ ] Ensure all compiler options are plumbed to solver
- [ ] Implement strict mode flag behavior
- [ ] Test interactions between options
- [ ] Document option impacts on type checking

---

## 7. Testing & Validation

### 7.1 Solver Unit Tests

**Status:** ✅ Good foundation

**Approach:** Test each type system operation in isolation.

**Test Categories:**
- Subtyping rules (primitives, objects, functions, tuples)
- Type operations (union, intersection, conditionals)
- Inference cases (generic inference, conditional inference)
- Edge cases (never, unknown, any, recursive types)

**Tasks:**
- [ ] Add tests for missing subtyping rules
- [ ] Add conditional type tests
- [ ] Add mapped type tests
- [ ] Add template literal tests
- [ ] Test recursive type edge cases
- [ ] Add performance benchmarks for complex types

### 7.2 Type System Laws

**Status:** ⚠️ Partially implemented

**Purpose:** Ensure type system satisfies mathematical properties.

**Key Laws:**
- Reflexivity: `T ≤ T`
- Transitivity: `A ≤ B` and `B ≤ C` implies `A ≤ C`
- Antisymmetry (with canonicalization): `A ≤ B` and `B ≤ A` implies `A = B`
- Top: `T ≤ any`
- Bottom: `never ≤ T`

**Tasks:**
- [ ] Add law tests to `src/solver/law_tests.rs`
- [ ] Test reflexivity for all type constructors
- [ ] Test transitivity chains
- [ ] Verify antisymmetry with interner
- [ ] Property-based testing for type operations

### 7.3 Conformance Validation

**Status:** ✅ Infrastructure exists

**Purpose:** Validate solver against TypeScript test suite.

**Approach:**
1. Implement feature in solver correctly
2. Run relevant conformance tests
3. Fix any bugs found
4. Move to next feature

**Note:** Do not use conformance results to drive implementation priorities.

**Tasks:**
- [ ] Run conformance tests for implemented features
- [ ] Fix bugs revealed by tests
- [ ] Document test failures due to unimplemented features
- [ ] Track progress metrics (without obsessing over percentages)

---

## Implementation Order

### Phase 1: Solidify Core (2-4 weeks)
1. Complete core subtyping rules (objects, functions, arrays, tuples)
2. Ensure coinductive semantics for recursive types
3. Verify semantic subtyping completeness
4. Add comprehensive law tests

### Phase 2: Type Operations (2-3 weeks)
1. Complete union and intersection types
2. Implement conditional types
3. Implement mapped types
4. Add template literal types

### Phase 3: Inference (2-3 weeks)
1. Complete generic type inference
2. Implement conditional type inference
3. Handle `this` type inference
4. Add contextual typing

### Phase 4: Compatibility (1-2 weeks)
1. Audit and complete compat layer
2. Implement all compiler options
3. Document unsound behaviors
4. Test option interactions

### Phase 5: Validation (ongoing)
1. Run conformance tests continuously
2. Fix bugs as they're found
3. Add solver unit tests for gaps
4. Performance optimization

---

## Success Metrics

**Solver Completeness:**
- [ ] All subtyping rules implemented and tested
- [ ] All type operations working
- [ ] All inference mechanisms functional
- [ ] Compat layer complete

**Quality:**
- [ ] All type system laws passing
- [ ] Zero solver panics
- [ ] Comprehensive unit test coverage (>90%)
- [ ] Performance: <1ms for typical type checks

**Compatibility:**
- [ ] All TS unsound behaviors in compat layer
- [ ] All compiler options respected
- [ ] Conformance pass rate: 70%+ (realistic, not forced)

---

## Key References

- `specs/SOLVER.md` - Type system mathematical foundation
- `specs/TYPESCRIPT_ADVANCED_TYPES.md` - TypeScript feature implementation details
- `specs/ECMAScript® 2026 Language Specification.md` - Language semantics
- `specs/TS_UNSOUNDNESS_CATALOG.md` - TypeScript unsound behaviors
- `src/solver/` - Solver implementation
- `src/solver/law_tests.rs` - Type system law tests
- `src/solver/compat_*.rs` - Compatibility layer

---

## Notes

**Why This Order?**
1. **Foundation first** - Can't build advanced features on shaky core
2. **Understanding over percentages** - Conformance will follow from correctness
3. **Incremental validation** - Test each layer before building on top

**Anti-Patterns to Avoid:**
- Chasing conformance percentages by adding test-aware hacks
- Implementing features without understanding their semantics
- Skipping core type system mechanics for "shiny" features
- Optimizing prematurely (measure first, then optimize)

**Red Flags:**
- Type checking produces different results for structurally identical types
- Solver panics on valid TypeScript code
- Conformance tests pass but understanding is shallow
- Performance degrades with type complexity

**When to Move Forward:**
- All laws in current phase are passing
- Implementation is understood, not just working
- Tests validate both correctness and edge cases
- Documentation is updated
