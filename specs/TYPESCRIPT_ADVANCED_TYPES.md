# TypeScript Advanced Types: Migration Reference

**Purpose:** Deep technical reference for implementing TypeScript's advanced type system in Rust
**Status:** Comprehensive Implementation Guide
**Last Updated:** January 2026

---

## Table of Contents

1. [Overview](#1-overview)
2. [Conditional Types](#2-conditional-types)
3. [Mapped Types](#3-mapped-types)
4. [Template Literal Types](#4-template-literal-types)
5. [Type Inference](#5-type-inference)
6. [Variance](#6-variance)
7. [Type Relationships](#7-type-relationships)
8. [Advanced Patterns from Type Libraries](#8-advanced-patterns-from-type-libraries)
9. [Known Issues and Edge Cases](#9-known-issues-and-edge-cases)
10. [Implementation Recommendations](#10-implementation-recommendations)

---

## 1. Overview

### 1.1 Advanced Type Categories

TypeScript's advanced types can be categorized into three major groups:

1. **Conditional Types** (`T extends U ? X : Y`)
   - Distributive behavior over unions
   - `infer` keyword for pattern matching
   - Deferred evaluation for generic types

2. **Mapped Types** (`{ [K in keyof T]: ... }`)
   - Homomorphic vs non-homomorphic mappings
   - Key remapping with `as` clause
   - Modifier manipulation (`+/-readonly`, `+/-?`)

3. **Template Literal Types** (`` `prefix${T}suffix` ``)
   - String pattern matching
   - Intrinsic string manipulation (`Uppercase`, etc.)
   - Inference from template patterns

### 1.2 Core Implementation Files

| File | Lines | Purpose |
|------|-------|---------|
| `checker.ts` | ~54,000 | Type checking and inference engine |
| `types.ts` | ~10,600 | Type definitions and flags |
| `parser.ts` | ~11,600 | Syntax parsing for type nodes |

### 1.3 Key Data Structures

```typescript
// TypeFlags bitmask (from types.ts:6324)
const enum TypeFlags {
    Conditional     = 1 << 24,  // T extends U ? X : Y
    TemplateLiteral = 1 << 27,  // `prefix${T}suffix`
    StringMapping   = 1 << 28,  // Uppercase/Lowercase type
    // ... (see types.ts for full list)
}

// ObjectFlags for mapped types (from types.ts:6505)
const enum ObjectFlags {
    Mapped           = 1 << 5,   // Mapped type
    Instantiated     = 1 << 6,   // Instantiated type
    // ...
}
```

---

## 2. Conditional Types

### 2.1 Syntax and Structure

```typescript
type ConditionalType = T extends U ? TrueType : FalseType;
```

**AST Node:** `ConditionalTypeNode` (SyntaxKind.ConditionalType = 262)

**Internal Representation:**
```typescript
interface ConditionalType extends Type {
    root: ConditionalRoot;
    checkType: Type;
    extendsType: Type;
    trueType: Type;
    falseType: Type;
    isDistributive: boolean;
    inferTypeParameters?: TypeParameter[];
}

interface ConditionalRoot {
    node: ConditionalTypeNode;
    checkType: Type;
    extendsType: Type;
    isDistributive: boolean;
    inferTypeParameters?: TypeParameter[];
    outerTypeParameters?: TypeParameter[];
    instantiations?: Map<string, Type>;
}
```

### 2.2 Resolution Algorithm

**Location:** `checker.ts:19712` - `getConditionalType()`

```
getConditionalType(root, mapper, forConstraint, aliasSymbol, aliasTypeArguments):
│
├── Loop (tail recursion optimization, max 1000 iterations):
│   │
│   ├── Instantiate checkType and extendsType with mapper
│   │
│   ├── Handle error/wildcard types → return early
│   │
│   ├── Check for tuple types with same arity (defer if generic)
│   │
│   ├── If root has inferTypeParameters:
│   │   ├── Create inference context
│   │   ├── Combine mappers for proper constraint handling
│   │   └── Run inferTypes() to collect candidates
│   │
│   ├── If types are non-generic (not deferred):
│   │   │
│   │   ├── DEFINITELY FALSE check:
│   │   │   │  Use permissive instantiation (wildcard type)
│   │   │   │  If checkType is 'any' OR not assignable to extendsType:
│   │   │   │
│   │   │   ├── For 'any' or when forConstraint with possible overlap:
│   │   │   │   └── Add trueType to extraTypes
│   │   │   │
│   │   │   ├── If falseType is nested conditional with same/no distribution:
│   │   │   │   └── Continue with nested conditional (tail recursion)
│   │   │   │
│   │   │   └── Return instantiated falseType
│   │   │
│   │   └── DEFINITELY TRUE check:
│   │       │  Use restrictive instantiation (no constraints)
│   │       │  If extendsType is any/unknown OR checkType assignable:
│   │       │
│   │       ├── If trueType is nested conditional with same/no distribution:
│   │       │   └── Continue with nested conditional (tail recursion)
│   │       │
│   │       └── Return instantiated trueType
│   │
│   └── DEFERRED: Create conditional type instance
│
└── Return result (with extraTypes as union if needed)
```

### 2.3 Distributive Behavior

**Location:** `checker.ts:20928` - `getConditionalTypeInstantiation()`

A conditional type `T extends U ? X : Y` is **distributive** when:
- `T` is a naked type parameter (not wrapped)
- The type parameter appears in check position

**Distribution Rule:**
```
(A | B) extends U ? X : Y
→ (A extends U ? X : Y) | (B extends U ? X : Y)
```

**Implementation:**
```typescript
// From checker.ts:20940-20946
const distributionType = root.isDistributive ?
    getReducedType(getMappedType(checkType, newMapper)) :
    undefined;

result = distributionType &&
         checkType !== distributionType &&
         distributionType.flags & (TypeFlags.Union | TypeFlags.Never) ?
    mapTypeWithAlias(distributionType, t =>
        getConditionalType(root, prependTypeMapping(checkType, t, newMapper), forConstraint),
        aliasSymbol, aliasTypeArguments) :
    getConditionalType(root, newMapper, forConstraint, aliasSymbol, aliasTypeArguments);
```

### 2.4 `infer` Keyword

**Purpose:** Extract type from a pattern during conditional type evaluation.

```typescript
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;
type Flatten<T> = T extends Array<infer U> ? U : T;
```

**Implementation Details:**

1. `infer` creates a type parameter in `inferTypeParameters`
2. During evaluation, `inferTypes()` collects candidates
3. Candidates are unified based on variance:
   - **Covariant:** Union of candidates
   - **Contravariant:** Intersection of candidates

**Constraints on `infer`:**
```typescript
type FirstString<T> = T extends [infer S extends string, ...unknown[]] ? S : never;
```

### 2.5 Critical Edge Cases

From GitHub issues analysis:

| Issue | Description | Complexity |
|-------|-------------|------------|
| #55733 | Unsound assignments with conditional types | Cursed |
| #62798 | Invariant calculation breaks referential transparency | Cursed |
| #60237 | Infinite instantiation with recursive conditionals | Deep recursion |
| #48033 | Conditional type prevents assignability | Structural |
| #32066 | Incorrect assignability from distributive types | Distributive |
| #46975 | Narrowing doesn't cascade through generics | Narrowing |

**Common Patterns That Cause Issues:**

1. **Recursive conditionals:**
   ```typescript
   type Deep<T> = T extends object ? { [K in keyof T]: Deep<T[K]> } : T;
   ```

2. **Self-referential with infer:**
   ```typescript
   type Unpacked<T> = T extends Promise<infer U> ? Unpacked<U> : T;
   ```

3. **Nested distributions:**
   ```typescript
   type Nested<T> = T extends infer U ? (U extends string ? U : never) : never;
   ```

---

## 3. Mapped Types

### 3.1 Syntax and Structure

```typescript
type MappedType = {
    [K in keyof T]: T[K];               // Basic
    [K in keyof T as NewKey<K>]: T[K];  // With key remapping
    readonly [K in keyof T]?: T[K];     // With modifiers
};
```

**Internal Representation:**
```typescript
interface MappedType extends AnonymousType {
    declaration: MappedTypeNode;
    typeParameter?: TypeParameter;      // The 'K' in '[K in ...]'
    constraintType?: Type;              // The constraint (keyof T)
    nameType?: Type;                    // Key remapping with 'as'
    templateType?: Type;                // The property type template
    modifiersType?: Type;               // The 'T' in 'keyof T'
}
```

### 3.2 Resolution Algorithm

**Location:** `checker.ts:14746` - `resolveMappedTypeMembers()`

```
resolveMappedTypeMembers(type):
│
├── Initialize empty members and indexInfos
│
├── Get type parameter, constraint, name type, template type, modifiers type
│
├── If homomorphic (keyof constraint):
│   └── forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType()
│       // Iterate over actual properties + index signatures
│
├── Else:
│   └── forEachType(getLowerBoundOfKeyType(constraintType))
│       // Iterate over union members
│
├── For each key type:
│   │
│   ├── Apply nameType transformation (if present)
│   │
│   ├── If resulting type is usable as property name:
│   │   │
│   │   ├── Check for existing property (merge if duplicate)
│   │   │
│   │   ├── Compute modifiers:
│   │   │   ├── Optional: +? or -? or inherit from source
│   │   │   └── Readonly: +readonly or -readonly or inherit
│   │   │
│   │   └── Create MappedSymbol with:
│   │       ├── nameType (for key)
│   │       ├── keyType (original iteration type)
│   │       ├── mappedType (parent reference)
│   │       └── syntheticOrigin (source property if homomorphic)
│   │
│   └── Else if valid index key type:
│       └── Create IndexInfo
│
└── Return resolved type with members and indexInfos
```

### 3.3 Homomorphic Mapped Types

A mapped type is **homomorphic** when it iterates over `keyof T`:

```typescript
type Homomorphic<T> = { [K in keyof T]: T[K] };      // ✓ Homomorphic
type NonHomomorphic<T> = { [K in "a" | "b"]: T[K] }; // ✗ Not homomorphic
```

**Why it matters:**
- Homomorphic types preserve property modifiers
- They enable reverse inference
- Better error messages (linked declarations)

### 3.4 Key Remapping (`as` clause)

**Location:** `checker.ts:14949` - `getMappedTypeNameTypeKind()`

```typescript
type Getters<T> = {
    [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};

// MappedTypeNameTypeKind enum:
enum MappedTypeNameTypeKind {
    None,       // No 'as' clause
    Filtering,  // 'as' result is subtype of original (e.g., as Exclude<K, 'x'>)
    Remapping   // 'as' transforms the key
}
```

**Filtering Keys:**
```typescript
type OnlyStrings<T> = {
    [K in keyof T as T[K] extends string ? K : never]: T[K];
};
```

### 3.5 Modifier Manipulation

```typescript
// Adding modifiers
type ReadonlyPartial<T> = { readonly [K in keyof T]?: T[K] };

// Removing modifiers
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Required<T> = { [K in keyof T]-?: T[K] };
```

**Implementation:**
```typescript
// From checker.ts:14901
function getMappedTypeModifiers(type: MappedType): MappedTypeModifiers {
    const declaration = type.declaration;
    return (declaration.readonlyToken ?
            declaration.readonlyToken.kind === SyntaxKind.MinusToken ?
                MappedTypeModifiers.ExcludeReadonly :
                MappedTypeModifiers.IncludeReadonly :
            0) |
           (declaration.questionToken ?
            declaration.questionToken.kind === SyntaxKind.MinusToken ?
                MappedTypeModifiers.ExcludeOptional :
                MappedTypeModifiers.IncludeOptional :
            0);
}
```

### 3.6 Critical Edge Cases

| Issue | Description |
|-------|-------------|
| #45560 | Mapped types lose type information |
| #39204 | Polymorphic `this` type lost through mapped type |
| #44475 | Type modifier breaks `any` |
| #57265 | Remapped keys unexpectedly widen type |
| #44325 | Distribution results changed |
| #44217 | `keyof` on array mapped type returns function types |

---

## 4. Template Literal Types

### 4.1 Syntax and Structure

```typescript
type Greeting = `Hello, ${string}!`;
type Route = `/${string}/${number}`;
type EventName<T extends string> = `on${Capitalize<T>}`;
```

**Internal Representation:**
```typescript
interface TemplateLiteralType extends Type {
    texts: readonly string[];  // ['Hello, ', '!']
    types: readonly Type[];    // [stringType]
}
```

### 4.2 Resolution Algorithm

**Location:** `checker.ts:18988` - `getTemplateLiteralType()`

```
getTemplateLiteralType(texts, types):
│
├── Find union in types → distribute recursively
│
├── Handle wildcardType → return wildcardType
│
├── addSpans(): Flatten template literal parts
│   │
│   ├── For each type:
│   │   │
│   │   ├── If literal/null/undefined:
│   │   │   └── Concatenate to current text span
│   │   │
│   │   ├── If TemplateLiteralType:
│   │   │   └── Recursively addSpans for nested template
│   │   │
│   │   ├── If generic index type or placeholder:
│   │   │   └── Add to newTypes, start new text span
│   │   │
│   │   └── Else: Return false (can't resolve to template)
│   │
│   └── Return true if successful
│
├── If no types remain → return StringLiteralType
│
├── Normalize: `${Mapping<xxx>}` → Mapping<xxx>
│
├── Cache and return TemplateLiteralType
│
└── Return stringType if addSpans fails
```

### 4.3 Intrinsic String Manipulation Types

**Location:** `checker.ts:19064` - `getStringMappingType()`

Built-in types that operate on strings at the type level:

| Type | Effect | Example |
|------|--------|---------|
| `Uppercase<S>` | All uppercase | `"hello"` → `"HELLO"` |
| `Lowercase<S>` | All lowercase | `"HELLO"` → `"hello"` |
| `Capitalize<S>` | First letter uppercase | `"hello"` → `"Hello"` |
| `Uncapitalize<S>` | First letter lowercase | `"Hello"` → `"hello"` |

**Implementation:**
```typescript
function applyStringMapping(symbol: Symbol, str: string): string {
    switch (intrinsicTypeKinds.get(symbol.escapedName as string)) {
        case IntrinsicTypeKind.Uppercase: return str.toUpperCase();
        case IntrinsicTypeKind.Lowercase: return str.toLowerCase();
        case IntrinsicTypeKind.Capitalize: return str.charAt(0).toUpperCase() + str.slice(1);
        case IntrinsicTypeKind.Uncapitalize: return str.charAt(0).toLowerCase() + str.slice(1);
    }
    return str;
}
```

### 4.4 Pattern Inference

**Location:** `checker.ts:26613` - `inferTypesFromTemplateLiteralType()`

```typescript
type ParseRoute<T> = T extends `/${infer Segment}/${infer Rest}`
    ? [Segment, ...ParseRoute<`/${Rest}`>]
    : T extends `/${infer Segment}`
    ? [Segment]
    : [];
```

**Algorithm:**
```
inferFromLiteralPartsToTemplateLiteral(sourceTexts, sourceTypes, target):
│
├── Match prefix: source must start with target's first text
├── Match suffix: source must end with target's last text
│
├── For each target placeholder:
│   │
│   ├── Find delimiter (target text) in source
│   │
│   ├── If delimiter not found → return undefined
│   │
│   ├── Extract segment between current position and delimiter
│   │
│   └── Create inferred type from segment:
│       ├── String literal if simple text
│       └── Template literal if contains types
│
└── Return array of inferred types
```

### 4.5 Critical Edge Cases

| Issue | Description |
|-------|-------------|
| #62937 | Stack overflow with deeply recursive template literals |
| #62933 | Maximum call stack with recursive templates in generics |
| #49839 | Lazy placeholder matching causes inference failure |
| #40731 | Unnormalized template literal types |
| #44792 | Circular references not allowed |

**Performance Concern:** Template literal inference can be exponential. The implementation uses caching and limits recursion depth.

---

## 5. Type Inference

### 5.1 Overview

TypeScript uses **bidirectional type inference**:
- **Synthesis (→):** Infer type from expression structure
- **Checking (←):** Verify expression against expected type

### 5.2 Core Inference Algorithm

**Location:** `checker.ts:26710` - `inferTypes()`

```
inferTypes(inferences, originalSource, originalTarget, priority, contravariant):
│
├── State:
│   ├── bivariant: boolean
│   ├── propagationType: Type (for 'any' propagation)
│   ├── inferencePriority: number
│   ├── visited: Map<string, number>
│   ├── sourceStack, targetStack: Type[]
│   └── expandingFlags: ExpandingFlags
│
├── inferFromTypes(source, target):
│   │
│   ├── Quick exits:
│   │   ├── target has no type variables → return
│   │   ├── target is NoInfer<T> → return
│   │   └── source is wildcard → propagate to target
│   │
│   ├── Same alias symbol → infer from type arguments
│   │
│   ├── Same union/intersection → infer constituent by constituent
│   │
│   ├── Target is Union:
│   │   ├── Match identical types, remove from both
│   │   ├── Match closely related types
│   │   └── Infer from remaining
│   │
│   ├── Target is Intersection:
│   │   └── Infer to each constituent
│   │
│   ├── Target is TypeParameter:
│   │   └── Record inference candidate with priority
│   │
│   ├── Target is IndexedAccessType (T[K]):
│   │   └── Special handling for property inference
│   │
│   ├── Target is ConditionalType:
│   │   └── Infer from check/extends types
│   │
│   ├── Target is single call/construct signature:
│   │   └── Infer parameter and return types
│   │
│   ├── Source and target are object types:
│   │   ├── Infer from matching properties
│   │   ├── Infer from index signatures
│   │   └── Infer from call/construct signatures
│   │
│   └── Source is object, target is mapped type:
│       └── Reverse mapped type inference
```

### 5.3 Inference Priority

```typescript
// From types.ts
const enum InferencePriority {
    None                     = 0,
    NakedTypeVariable        = 1 << 0,  // Naked type variable in union or intersection type
    SpeculativeTuple         = 1 << 1,  // Speculative tuple inference
    SubstituteSource         = 1 << 2,  // Source type is a substitution type
    HomomorphicMappedType    = 1 << 3,  // Reverse inference for homomorphic mapped type
    PartialHomomorphicMappedType = 1 << 4,  // Partial reverse inference
    MappedTypeConstraint     = 1 << 5,  // Constraint of mapped type
    ContravariantConditional = 1 << 6,  // Conditional type in contravariant position
    ReturnType               = 1 << 7,  // From return type of called function
    LiteralKeyof             = 1 << 8,  // From 'keyof' of string literal type
    NoConstraints            = 1 << 9,  // Don't infer from constraints
    AlwaysStrict             = 1 << 10, // Always use strict rules
    MaxValue                 = 1 << 11, // Maximum value (for initialization)

    PriorityImpliesCombination = ReturnType | MappedTypeConstraint | LiteralKeyof,
}
```

### 5.4 Candidate Selection

After collecting candidates, the final type is selected:

```
getInferredType(inference):
│
├── If contravariant candidates exist and covariant don't:
│   └── Return intersection of contravariant candidates
│
├── If covariant candidates exist:
│   └── Return union of covariant candidates (with widening)
│
├── If no candidates:
│   ├── Use default type if available
│   └── Otherwise use constraint or unknown
│
└── Apply constraint checking
```

### 5.5 Contextual Typing

```typescript
// Type flows from context to expression
const handler: (e: MouseEvent) => void = (e) => {
    console.log(e.button);  // e inferred as MouseEvent
};

// Array element contextual typing
const arr: number[] = [1, 2, 3].map(x => x * 2);
```

---

## 6. Variance

### 6.1 Variance Concepts

| Variance | Definition | Example |
|----------|------------|---------|
| **Covariant** (+) | A ≤ B ⟹ F<A> ≤ F<B> | Return types, readonly properties |
| **Contravariant** (-) | A ≤ B ⟹ F<B> ≤ F<A> | Parameter types |
| **Invariant** (0) | No relationship | Mutable properties |
| **Bivariant** (±) | Both directions | Function parameters (unsound) |
| **Independent** | Not witnessed | Unused type parameters |

### 6.2 Variance Computation

**Location:** `checker.ts:24942` - `getVariances()`

```
getVariancesWorker(symbol, typeParameters):
│
├── For each type parameter:
│   │
│   ├── Check explicit variance modifiers (in/out):
│   │   ├── out → Covariant
│   │   ├── in → Contravariant
│   │   └── in out → Invariant
│   │
│   ├── If no explicit modifier, compute:
│   │   │
│   │   ├── Create marker types:
│   │   │   ├── typeWithSuper: T instantiated with markerSuperType
│   │   │   └── typeWithSub: T instantiated with markerSubType
│   │   │
│   │   ├── Check relationships:
│   │   │   ├── typeWithSub ≤ typeWithSuper → Covariant
│   │   │   └── typeWithSuper ≤ typeWithSub → Contravariant
│   │   │
│   │   ├── If bivariant:
│   │   │   └── Check for independence using unrelated markers
│   │   │
│   │   └── Track unmeasurable/unreliable flags
│   │
│   └── Record variance for this parameter
│
└── Cache and return variances
```

### 6.3 Variance Annotations (TypeScript 4.7+)

```typescript
type Getter<out T> = () => T;           // Covariant
type Setter<in T> = (value: T) => void; // Contravariant
type Property<in out T> = {              // Invariant
    get(): T;
    set(value: T): void;
};
```

### 6.4 Known Variance Issues

| Issue | Description |
|-------|-------------|
| #62798 | Conditional type variance breaks referential transparency |
| #53210 | Bugs with variance checking |
| #48265 | Inaccurate variance in generic functions |
| #29698 | Key types not considered in variance |
| #52083 | `T[keyof T]` treated as invariant instead of covariant |

---

## 7. Type Relationships

### 7.1 Relationship Types

```typescript
// From types.ts
const enum RelationComparisonResult {
    Succeeded           = 1 << 0,
    Failed              = 1 << 1,
    Reported            = 1 << 2,
    ReportsUnmeasurable = 1 << 3,
    ReportsUnreliable   = 1 << 4,
    ReportsMask         = ReportsUnmeasurable | ReportsUnreliable,
}
```

### 7.2 Assignability Algorithm

**Location:** `checker.ts:22222` - `isTypeRelatedTo()`

```
isTypeRelatedTo(source, target, relation):
│
├── Normalize fresh literal types to regular
│
├── Quick check: source === target → true
│
├── Simple type checks (for non-identity):
│   ├── Comparable relation with reversed check
│   └── Simple structural check
│
├── Identity relation: flags must match
│
├── Check cache for previous result
│
├── If structured types:
│   └── checkTypeRelatedTo() for full structural check
│
└── Return result
```

### 7.3 Structural Comparison

```
structuredTypeRelatedTo(source, target):
│
├── Object types:
│   ├── Compare all target properties exist in source
│   ├── Compare property types (covariant for reads)
│   ├── Compare call signatures
│   ├── Compare construct signatures
│   └── Compare index signatures
│
├── Union source:
│   └── ALL source members must relate to target
│
├── Union target:
│   └── Source must relate to SOME target member
│
├── Intersection source:
│   └── SOME source member must relate to target
│
└── Intersection target:
    └── Source must relate to ALL target members
```

---

## 8. Advanced Patterns from Type Libraries

### 8.1 ts-toolbelt Patterns

**Repository:** [github.com/millsp/ts-toolbelt](https://github.com/millsp/ts-toolbelt)

**200+ utilities organized by category:**

#### Object Operations
```typescript
// Deep Partial
type PartialDeep<T> = T extends object
    ? { [K in keyof T]?: PartialDeep<T[K]> }
    : T;

// Object Merge (with overwrite semantics)
type Merge<O1, O2> = {
    [K in keyof O1 | keyof O2]: K extends keyof O2
        ? O2[K]
        : K extends keyof O1
        ? O1[K]
        : never;
};

// Path-based access
type Path<T, P extends readonly string[]> = P extends [infer H, ...infer R]
    ? H extends keyof T
        ? R extends string[]
            ? Path<T[H], R>
            : never
        : never
    : T;
```

#### List/Tuple Operations
```typescript
// Tuple Length
type Length<T extends readonly any[]> = T['length'];

// Head/Tail
type Head<T extends readonly any[]> = T extends [infer H, ...any[]] ? H : never;
type Tail<T extends readonly any[]> = T extends [any, ...infer R] ? R : never;

// Reverse
type Reverse<T extends readonly any[], Acc extends readonly any[] = []> =
    T extends [infer H, ...infer R]
        ? Reverse<R, [H, ...Acc]>
        : Acc;

// Flatten (one level)
type Flatten<T extends readonly any[]> = T extends [infer H, ...infer R]
    ? H extends readonly any[]
        ? [...H, ...Flatten<R>]
        : [H, ...Flatten<R>]
    : [];
```

#### Function Operations
```typescript
// Curry type
type Curry<F> = F extends (...args: infer A) => infer R
    ? A extends [infer H, ...infer T]
        ? (arg: H) => Curry<(...args: T) => R>
        : R
    : never;

// Compose type
type Compose<Fns extends readonly ((...args: any[]) => any)[]> =
    Fns extends [infer F extends (...args: any[]) => any]
        ? F
        : Fns extends [...infer Rest extends ((...args: any[]) => any)[], infer Last extends (...args: any[]) => any]
            ? (...args: Parameters<Last>) => ReturnType<Compose<Rest>>
            : never;
```

### 8.2 type-fest Patterns

**Repository:** [github.com/sindresorhus/type-fest](https://github.com/sindresorhus/type-fest)

**Key Utilities:**

```typescript
// Simplify - Flatten intersection types for readability
type Simplify<T> = { [K in keyof T]: T[K] } & {};

// Exact - Prevent excess properties
type Exact<T, Shape> = T & {
    [K in Exclude<keyof T, keyof Shape>]: never;
};

// Get - Path-based property access
type Get<T, Path extends string> = Path extends `${infer K}.${infer Rest}`
    ? K extends keyof T
        ? Get<T[K], Rest>
        : never
    : Path extends keyof T
    ? T[Path]
    : never;

// Schema - Convert object to validation schema
type Schema<T> = {
    [K in keyof T]-?: T[K] extends object
        ? Schema<T[K]>
        : (value: unknown) => value is T[K];
};

// Jsonify - Type-safe JSON serialization
type Jsonify<T> = T extends Date
    ? string
    : T extends ((...args: any[]) => any) | undefined
    ? never
    : T extends object
    ? { [K in keyof T]: Jsonify<T[K]> }
    : T;
```

#### Type Guards
```typescript
// IsEqual
type IsEqual<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2)
    ? true
    : false;

// IsNever
type IsNever<T> = [T] extends [never] ? true : false;

// IsAny
type IsAny<T> = 0 extends (1 & T) ? true : false;

// IsUnknown
type IsUnknown<T> = IsAny<T> extends true
    ? false
    : unknown extends T
    ? true
    : false;
```

### 8.3 String Manipulation Patterns

```typescript
// Split string by delimiter
type Split<S extends string, D extends string> =
    S extends `${infer Head}${D}${infer Tail}`
        ? [Head, ...Split<Tail, D>]
        : [S];

// Join array to string
type Join<T extends readonly string[], D extends string> =
    T extends [infer H extends string, ...infer R extends string[]]
        ? R['length'] extends 0
            ? H
            : `${H}${D}${Join<R, D>}`
        : '';

// CamelCase
type CamelCase<S extends string> = S extends `${infer H}_${infer T}`
    ? `${Lowercase<H>}${Capitalize<CamelCase<T>>}`
    : Lowercase<S>;

// PascalCase
type PascalCase<S extends string> = Capitalize<CamelCase<S>>;
```

### 8.4 Numeric Type Patterns

```typescript
// Build tuple of length N
type BuildTuple<N extends number, T extends any[] = []> =
    T['length'] extends N ? T : BuildTuple<N, [...T, unknown]>;

// Add (using tuple lengths)
type Add<A extends number, B extends number> =
    [...BuildTuple<A>, ...BuildTuple<B>]['length'] & number;

// Subtract
type Subtract<A extends number, B extends number> =
    BuildTuple<A> extends [...BuildTuple<B>, ...infer R]
        ? R['length']
        : never;

// Range
type Range<Start extends number, End extends number, Acc extends number[] = []> =
    Start extends End
        ? [...Acc, Start]
        : Range<Add<Start, 1>, End, [...Acc, Start]>;
```

---

## 9. Known Issues and Edge Cases

### 9.1 Conditional Type Issues

| Issue # | Title | Status | Notes |
|---------|-------|--------|-------|
| 55733 | Unsound assignments with conditional types | Cursed | Fundamental trade-off |
| 62798 | Invariant breaks referential transparency | Cursed | Eager variance calculation |
| 60237 | Infinite instantiation with recursive types | Open | Tail recursion limit (1000) |
| 48033 | Conditional prevents assignability | Open | Deferred evaluation issue |
| 32066 | Incorrect assignability from distributive | Open | Distribution semantics |
| 27118 | Assignability needs identical distributivity | Open | Design limitation |

### 9.2 Mapped Type Issues

| Issue # | Title | Status | Notes |
|---------|-------|--------|-------|
| 45560 | Mapped types lose type information | Open | Fix available |
| 39204 | Polymorphic `this` lost | Open | |
| 44475 | Type modifier breaks `any` | Open | |
| 57265 | Remapped keys widen unexpectedly | Open | |

### 9.3 Template Literal Issues

| Issue # | Title | Status | Notes |
|---------|-------|--------|-------|
| 62937 | Stack overflow with deep recursion | Open | Performance |
| 62933 | Call stack exceeded in generics | Open | Recursion limit |
| 49839 | Lazy placeholder matching failure | Cursed | Inference limitation |

### 9.4 Inference Issues

| Issue # | Title | Status | Notes |
|---------|-------|--------|-------|
| 62824 | silentNeverType leak | Open | Internal type escapes |
| 26242 | Partial type argument inference | Open | Proposal |
| 51108 | Inferred constraints | Experimental | |

### 9.5 Turing Completeness Implications

TypeScript's type system is **Turing complete**, which means:

1. **No guaranteed termination** - Type checking can hang
2. **Complexity limits** - Recursion depth (1000), instantiation count (5M)
3. **Performance variability** - Some patterns exponentially expensive

**Mitigation Strategies:**
- Tail call optimization for consecutive conditionals
- Caching of instantiated types
- Early termination with error types
- Distribution limits on cross-product unions

---

## 10. Implementation Recommendations

### 10.1 Data Structure Design

**Type Representation:**
```rust
enum Type {
    // Primitives
    Any, Unknown, String, Number, Boolean, BigInt, Symbol,
    Void, Undefined, Null, Never,

    // Literals
    StringLiteral { value: String },
    NumberLiteral { value: f64 },
    BigIntLiteral { value: BigInt },
    BooleanLiteral { value: bool },

    // Compound
    Union { types: Vec<TypeId> },
    Intersection { types: Vec<TypeId> },
    Tuple { elements: Vec<TupleElement> },

    // Object-like
    Object { ... },
    Interface { ... },
    Mapped { ... },

    // Advanced
    Conditional { root: ConditionalRoot },
    TemplateLiteral { texts: Vec<String>, types: Vec<TypeId> },
    TypeParameter { constraint: Option<TypeId>, default: Option<TypeId> },
    IndexedAccess { object: TypeId, index: TypeId },
}
```

### 10.2 Critical Algorithms

1. **Type Instantiation** - Must handle recursive types with caching
2. **Assignability Checking** - Structural comparison with variance
3. **Type Inference** - Bidirectional with priority-based candidate selection
4. **Conditional Evaluation** - Distribution and deferred resolution

### 10.3 Performance Considerations

1. **Caching is Critical:**
   - Type instantiation cache
   - Assignability relation cache
   - Variance computation cache

2. **Limits to Enforce:**
   - Recursion depth: 100 for instantiation, 1000 for conditionals
   - Instantiation count: 5,000,000 per expression
   - Cross-product union size

3. **Lazy Evaluation:**
   - Defer conditional types when generic
   - Defer mapped type member resolution
   - Defer template literal expansion

### 10.4 Testing Strategy

1. **Unit Tests:** Individual type operations
2. **Conformance Tests:** TypeScript's test suite
3. **Edge Case Tests:** From GitHub issues
4. **Performance Tests:** Recursive/complex types
5. **Fuzzing:** Random type combinations

### 10.5 Migration Priority

**Phase 1: Core Types**
- Primitives and literals
- Unions and intersections
- Basic object types

**Phase 2: Generics**
- Type parameters
- Type instantiation
- Basic inference

**Phase 3: Advanced Types**
- Conditional types (non-distributive first)
- Mapped types (homomorphic first)
- Template literal types

**Phase 4: Edge Cases**
- Distributive conditionals
- Key remapping
- Complex inference scenarios

---

## Appendix: Reference Implementation Snippets

### A.1 Conditional Type Resolution

```typescript
// Simplified from checker.ts
function getConditionalType(root: ConditionalRoot, mapper?: TypeMapper): Type {
    const checkType = instantiateType(root.checkType, mapper);
    const extendsType = instantiateType(root.extendsType, mapper);

    // Defer if generic
    if (isGeneric(checkType) || isGeneric(extendsType)) {
        return createConditionalType(root, mapper);
    }

    // Definitely false
    if (!isAssignableTo(checkType, extendsType)) {
        return instantiateType(root.falseType, mapper);
    }

    // Definitely true
    if (isAssignableTo(restrictive(checkType), restrictive(extendsType))) {
        return instantiateType(root.trueType, mapper);
    }

    // Indeterminate - return union
    return union(
        instantiateType(root.trueType, mapper),
        instantiateType(root.falseType, mapper)
    );
}
```

### A.2 Mapped Type Resolution

```typescript
// Simplified from checker.ts
function resolveMappedTypeMembers(type: MappedType): ResolvedType {
    const members = new Map<string, Symbol>();
    const constraint = getConstraintType(type);
    const template = getTemplateType(type);

    for (const keyType of iterateKeyTypes(constraint)) {
        const propName = getPropertyName(keyType);
        const propType = instantiateType(template, { [type.typeParam]: keyType });

        const modifiers = computeModifiers(type, keyType);
        members.set(propName, createProperty(propName, propType, modifiers));
    }

    return createResolvedType(members);
}
```

### A.3 Template Literal Construction

```typescript
// Simplified from checker.ts
function getTemplateLiteralType(texts: string[], types: Type[]): Type {
    // Distribute over unions
    const unionIdx = types.findIndex(t => isUnion(t));
    if (unionIdx >= 0) {
        return mapUnion(types[unionIdx], t =>
            getTemplateLiteralType(texts, replaceAt(types, unionIdx, t))
        );
    }

    // Flatten and concatenate
    let result = texts[0];
    const newTypes: Type[] = [];
    const newTexts: string[] = [];

    for (let i = 0; i < types.length; i++) {
        if (isLiteral(types[i])) {
            result += toString(types[i]) + texts[i + 1];
        } else {
            newTexts.push(result);
            newTypes.push(types[i]);
            result = texts[i + 1];
        }
    }

    if (newTypes.length === 0) {
        return createStringLiteral(result);
    }

    newTexts.push(result);
    return createTemplateLiteralType(newTexts, newTypes);
}
```

---

## Sources

### Official Documentation
- [TypeScript Handbook - Conditional Types](https://www.typescriptlang.org/docs/handbook/2/conditional-types.html)
- [TypeScript Handbook - Mapped Types](https://www.typescriptlang.org/docs/handbook/2/mapped-types.html)
- [TypeScript Handbook - Template Literal Types](https://www.typescriptlang.org/docs/handbook/2/template-literal-types.html)

### Source Code
- `/Users/mohsenazimi/code/typescript/src/compiler/checker.ts`
- `/Users/mohsenazimi/code/typescript/src/compiler/types.ts`
- `/Users/mohsenazimi/code/typescript/docs/TYPE_CHECKER_DESIGN.md`

### Type Libraries
- [ts-toolbelt](https://github.com/millsp/ts-toolbelt) - 200+ type utilities
- [type-fest](https://github.com/sindresorhus/type-fest) - Essential type utilities

### GitHub Issues
- microsoft/TypeScript issues labeled: Domain: Conditional Types, Domain: Mapped Types, Domain: check: Type Inference, Domain: check: Variance Relationships

---

*This document is intended as a reference for implementing TypeScript's advanced type system in Rust. The algorithms and edge cases documented here reflect the current behavior of the TypeScript compiler.*
