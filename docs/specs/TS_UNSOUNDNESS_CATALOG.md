# TypeScript Unsoundness Catalog

This document catalogs known, intentional deviations from sound set-theoretic typing in TypeScript. It serves as the requirement specification for the **Compatibility Layer** (The "Lawyer") defined in `docs/SOLVER.md`. The Core Solver (The "Judge") should remain mathematically sound; the rules below are applied by the wrapper to mimic TypeScript's pragmatic design choices.

## 1. The "Any" Type
**Behavior:** Acts as both Top (`unknown`) and Bottom (`never`). It is assignable to everything and everything is assignable to it.
**Example:** `tests/cases/compiler/anyAssignability.ts`
**Solver Rule:** Compat Layer short-circuits: if `sub` is `any` OR `sup` is `any`, return `true`. This prevents `any` from polluting the transitive logic of the set-theory engine.

## 2. Function Bivariance
**Behavior:**
*   **Methods:** Always bivariant (accepts narrower OR wider arguments).
*   **Functions:** Contravariant by default, but Bivariant if `strictFunctionTypes` is false.
**Example:** `tests/cases/compiler/methodBivariance.ts`
**Solver Rule:**
*   Check `SolverConfig.strict_function_types`.
*   If checking a **Method** (property in object), force `Variance::Bivariant`.
*   If checking a **Function** (standalone) and config is loose, force `Variance::Bivariant`.
*   Otherwise, use `Variance::Contravariant` (Sound).

## 3. Covariant Mutable Arrays
**Behavior:** Arrays (and standard mutable collections) are treated covariantly, despite being mutable. `Dog[]` is assignable to `Animal[]`, even though writing a `Cat` into the latter is unsafe.
**Example:** `tests/cases/conformance/types/typeRelationships/assignmentCompatibility/assignmentCompatibilityForArrays.ts`
**Solver Rule:** When checking `Array<T>` vs `Array<U>`, apply `Covariant` check on generic parameters ($T \subseteq U$), ignoring the write-side safety requirement.

## 4. Freshness / Excess Property Checks
**Behavior:** Object literals ("fresh" objects) are subject to excess property checks; variables with the same shape are not.
**Example:** `tests/cases/compiler/excessPropertyCheck.ts`
**Solver Rule:**
*   The Lowering phase must flag Object Literal types with `TypeFlags::IS_FRESH`.
*   If `sub` is Fresh, the Solver enables the **Exactness Check** (rejects width subtyping: `sub` cannot have keys that `sup` lacks).

## 5. Nominal Classes (Private Members)
**Behavior:** Classes are structural *unless* they contain `private` or `protected` members. If present, the class becomes **Nominal**—it is only compatible with itself or its subclasses, not with a structurally identical class/object.
**Example:** `tests/cases/conformance/classes/members/privateTypes/privateStructuralComparison.ts`
**Solver Rule:**
*   In `solve_object`, check if `sup` has private/protected members.
*   If yes, switch from Structural Logic (matching properties) to **Nominal Logic** (checking `sub.declaration_id` descends from `sup.declaration_id`).

## 6. Void Return Exception
**Behavior:** Functions returning `void` (`() => void`) are allowed to accept functions with non-void returns (`() => string`). The return value is simply considered "ignored."
**Example:** `tests/cases/compiler/voidReturnCompatibility.ts`
**Solver Rule:** If `sup.return_type` is `Intrinsic::Void`, treat `sub.return_type` as compatible regardless of its actual type.

## 7. Open Numeric Enums
**Behavior:** Numeric Enums are not opaque sets. `number` is assignable to `Enum`, and `Enum` is assignable to `number`. Values outside the defined enum constants are valid.
**Example:** `tests/cases/compiler/enumAssignability.ts`
**Solver Rule:**
*   If `sub` is `Number` and `sup` is `Enum` (numeric) -> `true`.
*   If `sub` is `Enum` (numeric) and `sup` is `Number` -> `true`.

## 8. Unchecked Indexed Access
**Behavior:** Accessing `T[K]` where `T` is an array/index-signature returns `T`, ignoring the possibility of `undefined` (unless `noUncheckedIndexedAccess` is on).
**Example:** `const x: number[] = []; const y = x[100]; // y is number`
**Solver Rule:**
*   In `TypeKey::IndexAccess` logic: do **not** add `Undefined` to the result union by default.
*   Only add `Undefined` if `SolverConfig.no_unchecked_indexed_access` is true.

## 9. Legacy Null/Undefined
**Behavior:** If `strictNullChecks` is OFF, `null` and `undefined` behave like `never` (Bottom)—they are assignable to everything.
**Example:** `tests/cases/compiler/nullAssigability.ts`
**Solver Rule:**
*   Check `SolverConfig.strict_null_checks`.
*   If `false`: Treat `null` and `undefined` as subtypes of all types (except `never`).

## 10. Literal Widening
**Behavior:** Literal types (`"hello"`, `42`) widen to primitives (`string`, `number`) when assigned to mutable storage (`let`, object properties), but stay narrow for `const`.
**Example:** `tests/cases/conformance/types/literal/literalTypeWidening.ts`
**Solver Rule:**
*   This is primarily an **Inference/Lowering** rule.
*   When inferring the type of a mutable binding (`let`, `var`), call `widen_literal(type)` which converts `Literal(V)` -> `Intrinsic(Primitive)`.

## 11. Error Poisoning
**Behavior:** The `error` type (sentinel for compiler failures) participates in every relation to avoid cascading diagnostics.
**Solver Rule:**
*   `Error` is Top and Bottom in the Compat Layer.
*   Result of `Union(Error, T)` is `Error` (suppression).

## 12. Apparent Members of Primitives
**Behavior:** Primitives (`string`, `number`) appear to have methods (`.toFixed()`, `.toUpperCase()`) via their global interface wrappers (`String`, `Number`).
**Solver Rule:**
*   If `solve_object` encounters a primitive `sub`, lower it to its **Apparent Type** (the corresponding Global Interface) before checking properties.







## 13. Weak Type Detection
**Behavior:** In a purely structural system, an interface with *only* optional properties (e.g., `interface Config { port?: number }`) accepts *any* object (except `null`/`undefined`), because every object satisfies "zero required properties."
TypeScript intentionally breaks this. If a type is "Weak" (only optional properties), it rejects assignments that have **no overlapping properties** at all.
**Example:** `tests/cases/compiler/weakTypes.ts`
```typescript
interface Weak { a?: number }
const x = { b: "unrelated" };
const y: Weak = x; // Error in TS (Safety check), but valid structurally!
```
**Solver Rule:**
*   **Detection:** Check if `sup` (Target) contains *only* optional properties.
*   **Enforcement:** If `sup` is Weak, scan `sub`. If `sub` shares **zero** property keys with `sup`, return `false` (even if structurally valid).

## 14. Optionality vs. Undefined
**Behavior:** Historically, an optional property `x?: number` implied `x` could be `number | undefined`.
With `exactOptionalPropertyTypes: true`, these are distinct: `x?: number` means the key can be missing, but *if present*, it must be a number (not `undefined`).
**Example:** `tests/cases/conformance/types/typeRelationships/assignmentCompatibility/optionalPropertyTypeCompatibility.ts`
**Solver Rule:**
*   Check `SolverConfig.exact_optional_property_types`.
*   **If False (Default/Legacy):** When lowering/checking an optional property `{ k?: T }`, treat it effectively as `{ k: T | undefined }`.
*   **If True:** Treat `{ k?: T }` strictly as "Key `k` may be missing OR `k` has type `T`". `undefined` is not automatically added to the domain.

## 15. Tuple-Array Assignment
**Behavior:**
*   **Tuple -> Array:** `[string, number]` is assignable to `(string | number)[]`. (Sound-ish).
*   **Array -> Tuple:** `(string | number)[]` is **not** assignable to `[string, number]` (length unknown).
*   **Exceptions:** Empty arrays `[]` are sometimes assignable to Tuples with optional elements.
**Example:** `tests/cases/conformance/types/tuple/tupleAssignment.ts`
**Solver Rule:**
*   If `sub` is Tuple and `sup` is Array: Check if union of `sub` elements is subtype of `sup` element.
*   If `sub` is Array and `sup` is Tuple: **Reject** (unless `sub` is explicitly empty tuple/array literal and `sup` allows it).

## 16. Rest Parameter Bivariance
**Behavior:** A function `(...args: any[]) => void` is treated as a universal supertype for functions, ignoring the actual parameter types of the target.
**Example:**
```typescript
type Logger = (...args: any[]) => void;
const log: Logger = (id: number) => {}; // Allowed
```
**Solver Rule:**
*   If `sup` has a rest parameter of type `any` (or `unknown`), allow assignment even if `sub` has specific required parameters. This facilitates "monkey-patching" or generic wrapping patterns.

## 17. The Instantiation Depth Limit
**Behavior:** TypeScript imposes a hard depth limit (usually 50 or 100) on generic instantiations to prevent infinite compiler hangs on complex recursive types (often found in deep JSON or React types).
**Example:** `tests/cases/compiler/excessiveDeepInstantiation.ts`
**Solver Rule:**
*   The Solver Context must maintain a `recursion_depth` counter.
*   If `depth > MAX_DEPTH` (configurable, default ~50), return `TypeKey::Error` (or `any`) to fail gracefully rather than crashing the WASM stack.

## 18. Class "Static Side" Rules
**Behavior:**
*   Classes are compared structurally for their instance side.
*   **However**, the *Static* side (the constructor function) has special rules. Standard properties (`name`, `length`, `prototype`) are often ignored or treated specially to avoid false positives with standard `Function` interface.
*   `protected` static members behave nominally (like private instance members).
**Example:** `tests/cases/conformance/classes/classStaticSide.ts`
**Solver Rule:**
*   When checking `typeof ClassA <: typeof ClassB`:
    *   Ignore standard `Function` properties (`name`, `caller`, `bind`, etc.) unless explicitly declared in the class body.
    *   Apply Nominal Logic if `protected static` members are present.




## 19. Covariant `this` Types
**Behavior:** In TypeScript classes, the polymorphic `this` type is treated efficiently as **Covariant**, even in places where it should be Contravariant (like method arguments).
**Example:**
```typescript
class Box {
    content: any;
    // 'other' is type 'this'.
    // In strict theory, 'this' appearing in an argument makes the class Invariant.
    // TS allows B <: A even if B overrides this method.
    compare(other: this) { }
}
class StringBox extends Box {
    compare(other: StringBox) { } // Method is tighter
}
// Unsoundness:
const b: Box = new StringBox();
b.compare(new Box()); // Runtime error inside StringBox.compare, but TS allows assignment.
```
**Solver Rule:**
*   When checking Class subtyping, if `this` appears in method parameters, **ignore** the standard Contravariance rule. Treat `this` as if it simply transforms to the current class type covariantly.

## 20. The `Object` vs `object` vs `{}` Trifecta
**Behavior:** These three types look similar but behave very differently regarding primitives.
*   `Object` (Global Interface): Matches everything except `null`/`undefined` (including primitives like `123`, because `(123).toString()` exists).
*   `{}` (Empty Object Type): Matches everything except `null`/`undefined` (structurally, everything has "at least zero properties").
*   `object` (Lower-case Intrinsic): Matches **only non-primitives** (objects, arrays, functions).
**Example:** `tests/cases/compiler/objectTypesIdentity.ts`
**Solver Rule:**
*   `sub: Primitive` is assignable to `sup: Object` -> **True**.
*   `sub: Primitive` is assignable to `sup: {}` -> **True**.
*   `sub: Primitive` is assignable to `sup: object` -> **False**.
*   *Note:* The Solver must treat `{}` not as "Object with 0 props" but as a distinct Intrinsic that acts as a supertype of primitives.

## 21. Intersection Reduction (Reduction to `never`)
**Behavior:** TypeScript aggressively reduces intersections of disjoint primitives to `never`.
*   `string & number` -> `never`.
*   **However**, it does *not* reduce disjoint objects structurally immediately.
*   `{ type: "a" } & { type: "b" }` -> Technically `never` (impossible value), but TS might track it as an intersection until property access occurs.
**Example:** `tests/cases/conformance/types/intersection/intersectionReduction.ts`
**Solver Rule:**
*   In `TypeKey::Intersection` normalization:
    *   If intrinsics are disjoint (e.g. `String` and `Number`), reduce immediately to `TypeKey::Intrinsic(Never)`.
    *   If object literals have disjoint discriminant properties (literal fields), reduce to `Never`.
    *   Otherwise, keep as Intersection.

## 22. Template String Expansion Limits
**Behavior:** Template literal types can generate unions: `` `color-${'red' | 'blue'}` `` -> `` "color-red" | "color-blue" ``.
To prevent memory exhaustion, TS imposes a limit (roughly 100,000 items). If a union exceeds this, it de-optimizes to `string` (widening) or errors.
**Example:** `tests/cases/conformance/types/literal/templateLiteralTypesTooComplex.ts`
**Solver Rule:**
*   When synthesis encounters a Template Literal Type, perform a "Cardinality Check" before expansion.
*   If `(A.size * B.size * ...) > LIMIT`, abort expansion and return `Intrinsic::String` (or `Error` depending on context). Do *not* try to inter 100k types.

## 23. Comparison Operator Overlap (Expression Logic)
**Behavior:** This isn't subtyping, but it's a critical solver query. TS forbids `x === y` if the types have **no overlap**.
*   `1 === 2`: Allowed (both numbers).
*   `1 === "a"`: Error (no overlap).
*   **Quirk:** It allows overlap if one side is a generic `T` (because `T` might become anything), *unless* `T` is constrained to a disjoint type.
**Solver Rule:**
*   Implement a separate query: `compute_overlap(A, B)`.
*   Differs from Subtyping: `Intersection(A, B)` is not `Never`.
*   Lawyer Layer: If `compute_overlap` returns false, flag a "This condition will always return false" diagnostic, but the types themselves are valid.




## 24. Cross-Enum Incompatibility (The Nominal Enum Rule)
**Behavior:** While Numeric Enums are "Open" regarding numbers (Item #7), they are **Nominal** regarding *other* enums.
*   `EnumA.Val` (0) is assignable to `number`.
*   `EnumB.Val` (0) is assignable to `number`.
*   **BUT:** `EnumA.Val` is **NOT** assignable to `EnumB`.
**Example:** `tests/cases/conformance/enums/enumAssignabilityIncompatibility.ts`
**Solver Rule:**
*   In `solve_subtype(sub, sup)`:
    *   If *both* are Enums (and not the exact same ID), **Reject**.
    *   This overrides the underlying numeric structural equality. The "Lawyer" checks the Enum Declaration ID brand before checking the value.

## 25. Index Signature Consistency
**Behavior:** If a type declares an Index Signature (`[k: string]: T`), **all** explicit properties in that type must be subtypes of `T`.
*   `{ [k: string]: number; id: number }` -> Valid.
*   `{ [k: string]: number; name: string }` -> **Error** (string is not number).
*   **Quirk:** This applies strictly to *Interface/Literal* declarations, but Intersections can sometimes create "impossible" types that technically violate this, which reduces to `never` or behaves inconsistently depending on property access method.
**Example:** `tests/cases/compiler/indexSignaturePropertyCheck.ts`
**Solver Rule:**
*   **Lowering Phase:** When synthesizing a `TypeKey::Object`, validate that all named members are compatible with the index signature (if present). If not, flag `TypeKey::Error`.
*   **Solving Phase:** If `sup` has an index signature, `sub`'s explicitly known properties must *also* be checked against that signature, not just `sub`'s index signature.

## 26. Split Accessors (Getter/Setter Variance)
**Behavior:** TypeScript allows a property to have different types for reading (Getter) vs. writing (Setter).
*   `get x(): string`
*   `set x(v: string | number)`
*   The property `x` is effectively `string` (covariant) for reads, and `string | number` (contravariant) for writes.
**Example:** `tests/cases/conformance/classes/propertyMemberDeclarations/accessorWithDifferentTypes.ts`
**Solver Rule:**
*   The `TypeKey::Object` property storage must support **Split Types**: `Property { read_type: TypeId, write_type: TypeId }`.
*   **Subtyping:** `Sub.x <: Sup.x` means:
    *   `Sub.read <: Sup.read` (Covariant)
    *   `Sup.write <: Sub.write` (Contravariant)

## 27. Homomorphic Mapped Types over Primitives
**Behavior:** You can apply a Mapped Type to a primitive!
*   `type M = { [K in keyof number]: boolean }`
*   This doesn't map over nothing; it maps over the **Apparent Type** (the `Number` interface), producing an object with keys like `toFixed`, `toExponential`, etc., but with `boolean` values.
**Example:** `tests/cases/conformance/types/mapped/mappedTypesOverPrimitives.ts`
**Solver Rule:**
*   If `TypeKey::Mapped` is applied to a `TypeKey::Intrinsic` (string/number/boolean):
    *   Lower the intrinsic to its Interface equivalent (`String`, `Number`).
    *   Apply the mapping to *that* interface's keys.

## 28. The "Constructor Void" Exception
**Behavior:** Similar to functions (Item #6), a Class Constructor declared to return `void` (in a type definition) allows a concrete class implementation that constructs an object.
*   `type Ctor = new () => void;`
*   `class C { }`
*   `const c: Ctor = C;` // Allowed!
**Example:** `tests/cases/conformance/types/typeRelationships/assignmentCompatibility/constructorVoid.ts`
**Solver Rule:**
*   Apply the same logic as the "Void Return Exception" to `TypeKey::Object` signatures that are `new` signatures (Construct Signatures).




## 29. The Global `Function` Type (The Untyped Callable)
**Behavior:** The global `Function` interface behaves like an untyped supertype for all callables.
*   Any arrow function/method is assignable to `Function`.
*   However, `Function` is **not** safe to call (it effectively has `(...args: any[]) => any`).
*   **Quirk:** It behaves differently from `{}` or `object` because it specifically allows "bind", "call", "apply".
**Example:** `tests/cases/compiler/functionType.ts`
**Solver Rule:**
*   `sub: Any Callable` is assignable to `sup: Intrinsic(Function)` -> **True**.
*   `sub: Intrinsic(Function)` is assignable to `sup: Any Callable` -> **False** (usually, unless `sup` is `any`).
*   Treat `Function` as a distinct Intrinsic that sits between `Object` and specific function signatures.

## 30. `keyof` Contravariance (Set Inversion)
**Behavior:** The `keyof` operator flips set logic.
*   `A | B` is a wider set than `A`.
*   `keyof (A | B)` is a **narrower** set than `keyof A`.
*   **Logic:** A key is only safe to access on `A | B` if it exists on **both** `A` AND `B`. Thus, `keyof (A | B) === (keyof A) & (keyof B)`.
**Example:** `tests/cases/conformance/types/typeRelationships/typeInference/keyofInference.ts`
**Solver Rule:**
*   When evaluating `TypeKey::KeyOf(T)`:
    *   If `T` is `Union(A, B)`, return `Intersection(KeyOf(A), KeyOf(B))`.
    *   *Implementation Note:* This is one of the few places where Unions turn into Intersections.

## 31. Base Constraint Assignability (Generic Erasure)
**Behavior:** Inside a generic function, how do we check `T <: U`?
*   If `T` and `U` are generic, we cannot check their instances.
*   We check their **Constraints**.
*   **Rule:** `T <: U` if `Constraint(T) <: U`.
*   **But:** `T <: Constraint(T)` is always true.
**Example:**
```typescript
function f<T extends string, U>(x: T, y: U) {
    let s: string = x; // OK: T extends string
    let t: T = "hello"; // Error: string is not T (T could be "world")
}
```
**Solver Rule:**
*   If `sub` is a Generic Param (`TypeKey::Param`):
    *   If `sup` is not a Param, switch to checking `Constraint(sub) <: sup`.
    *   If `sup` is also a Param, check if `sub == sup` (Identity). If not, check `Constraint(sub) <: sup`. (Do NOT check `Constraint(sub) <: Constraint(sup)`—that is necessary but insufficient).

## 32. Best Common Type (BCT) Inference
**Behavior:** When inferring an array literal `[1, "a"]`, TS does not create a Tuple `[1, "a"]`. It widens to `(number | string)[]`.
*   **Algorithm:** It gathers all element types and finds a type that is a supertype of all candidates.
*   **Quirk:** If no single candidate is a supertype of all others, it creates a Union of all candidates.
**Example:** `tests/cases/conformance/types/typeRelationships/bestCommonType/bestCommonType.ts`
**Solver Rule:**
*   **Inference Phase:** When lowering an Array Literal:
    *   Collect all element types $E_1, E_2, ...$.
    *   Try to find a $T \in \{E\}$ such that $\forall x, E_x <: T$.
    *   If found, result is `T[]`.
    *   If not found, result is `(E_1 | E_2 | ...)[]`.
    *   *Optimization:* Do not create massive unions if a common base class exists (e.g., `Dog` and `Cat` -> `Animal` if explicitly hinted, otherwise `Dog | Cat`).

## 33. The "Object" vs "Primitive" boxing behavior
**Behavior:**
*   `number` is assignable to `Number` (The Object wrapper).
*   `Number` is **NOT** assignable to `number`.
*   `number` is assignable to `Object` (Global Interface).
*   `number` is assignable to `{}`.
**Example:** `tests/cases/conformance/types/primitives/primitiveTypeAssignment.ts`
**Solver Rule:**
*   Be extremely careful with the distinction between `Intrinsic::Number` (primitive) and `TypeKey::Ref(Symbol::Number)` (the interface).
*   Allow `Primitive -> Interface` (Auto-boxing compatibility).
*   Disallow `Interface -> Primitive` (Unboxing is not automatic in subtyping, only in operations).


## 34. String Enums (Strict Opaque Types)
**Behavior:** Unlike Numeric Enums (Entry #7), String Enums are **not** open.
*   `enum E { A = "a" }`
*   `const x: E = "a";` // **Error!**
*   String Enums are "Opaque"—a string literal cannot be assigned to the Enum type, even if the values match. You must use the Enum member (`E.A`).
**Example:** `tests/cases/conformance/enums/stringEnumAssignability.ts`
**Solver Rule:**
*   If `sup` is a String Enum:
    *   `sub` must be the **exact same** Enum type (Identity/Nominal check).
    *   OR `sub` must be a reference to a specific member of that Enum.
    *   Reject `Literal("a")`.

## 35. The Recursion Depth Limiter ("The Circuit Breaker")
**Behavior:** While Coinduction handles "perfect" cycles, "expanding" cycles (types that grow slightly with every recursive step) can crash the compiler.
*   TS implements a "Runaway Recursion" check. If the instantiation depth of a generic type exceeds a threshold (typically ~50-100), it emits `error` or `any`.
**Example:** `tests/cases/compiler/recursiveTypeFault.ts`
**Solver Rule:**
*   **Safety Valve:** The `Lowering` phase must pass a `depth` counter.
*   If `depth > CONFIG.max_instantiation_depth`:
    *   Log a "Type instantiation is excessively deep" diagnostic.
    *   Return `TypeKey::Error` (or `Intrinsic::Any` for legacy compat) to halt the expansion immediately.

## 36. JSX Intrinsic Lookup (Case Sensitivity)
**Behavior:** JSX tags behave differently based on casing.
*   `<div />` (Lowercase) -> Look up property `"div"` in global interface `JSX.IntrinsicElements`.
*   `<MyComp />` (Uppercase) -> Look up variable `MyComp` in current scope and check its signature.
**Example:** `tests/cases/conformance/jsx/jsxIntrinsicElements.ts`
**Solver Rule:**
*   **Lowering Phase:**
    *   If tag name starts with **Lowercase**: Synthesize a `TypeKey::IndexAccess` lookup on the `JSX.IntrinsicElements` interface.
    *   If tag name starts with **Uppercase**: Resolve symbol normally as a Value.

## 37. `unique symbol` (Nominal Primitives)
**Behavior:** Most primitives are structural. `symbol` is structural. But `unique symbol` is **Nominal**.
*   `const sym1: unique symbol = Symbol();`
*   `const sym2: unique symbol = Symbol();`
*   `sym1` is **NOT** assignable to `sym2`.
*   They are treated as distinct Unit Types (like `"a"` vs `"b"`), but based on *declaration identity* rather than value.
**Example:** `tests/cases/conformance/types/primitives/symbol/uniqueSymbol.ts`
**Solver Rule:**
*   In `TypeKey`: Represent `unique symbol` not as `Intrinsic` but as `Ref(SymbolId)`.
*   **Subtyping:** `Ref(SymA) <: Ref(SymB)` only if `SymA == SymB`.
*   `Ref(SymA) <: Intrinsic(Symbol)` is True.

## 38. Correlated Unions (The Cross-Product limitation)
**Behavior:** When accessing a Union of Objects with a Union of Keys, TS computes the **Cross-Product**, which often results in a wider type than expected (loss of correlation).
*   `type A = { kind: 'a', val: number } | { kind: 'b', val: string }`
*   `obj['val']` -> `number | string` (Correct).
*   *Limitation:* TS generally cannot track that `obj.kind` "a" implies `obj.val` is "number" *during a generic access*.
**Solver Rule:**
*   When evaluating `IndexAccess(Union(ObjA, ObjB), Union(Key1, Key2))`:
    *   Result is `Union(ObjA[Key1], ObjA[Key2], ObjB[Key1], ObjB[Key2])`.
    *   Do **not** try to be smarter than TS here; implementing "Correlated Access" logic would make your solver *incompatible* (too strict/too smart) with existing TS code.

## 39. `import type` Erasure (Value vs Type Space)
**Behavior:** Symbols imported via `import type` do not exist in the Value Space.
*   `import type { A } from './a';`
*   `const x: A = ...;` // OK.
*   `new A();` // **Error: 'A' cannot be used as a value.**
**Solver Rule:**
*   **Resolution Phase:** If a symbol is flagged as `TypeOnlyImport`:
    *   `resolve_type(Symbol)` -> OK.
    *   `resolve_value(Symbol)` -> Return `Error` (Symbol not found in value space).
    *   This prevents the solver from accidentally allowing runtime usage of erased types.



## 40. Distributivity Disabling (`[T] extends [U]`)
**Behavior:** As noted in #4.2 (Design Doc), conditional types distribute over unions. However, developers often *want* to check the union as a whole. The supported "hack" to disable distribution is wrapping types in a tuple.
*   `type Check<T> = T extends any ? true : false;` -> `Check<A | B>` is `boolean`.
*   `type Check<T> = [T] extends [any] ? true : false;` -> `Check<A | B>` is `true`.
**Example:** `tests/cases/conformance/types/conditional/conditionalTypes1.ts`
**Solver Rule:**
*   **Lowering/Normalization:** Do *not* optimize `[T]` into `T`. Keep the Tuple wrapper.
*   **Evaluation:** When the Conditional logic sees `Tuple(T)` extends `Tuple(U)`, it performs a standard subtyping check `Tuple(T) <: Tuple(U)`. Since `T` is now wrapped, it is no longer a "Naked Type Parameter," so the **Distributivity Rule** is skipped.

## 41. Key Remapping & Filtering (`as never`)
**Behavior:** In Mapped Types, if you remap a key to `never`, that key is **removed** from the resulting object type. This is how `Omit` is implemented under the hood in modern TS.
*   `type Omit<T, K> = { [P in keyof T as P extends K ? never : P]: T[P] }`
**Example:** `tests/cases/conformance/types/mapped/mappedTypeKeyRemapping.ts`
**Solver Rule:**
*   **Synthesis Phase:** When evaluating a Mapped Type:
    1.  Compute the new key name via the `as` clause.
    2.  If the result is `Intrinsic::Never`, **skip** this property entirely (do not add it to the resulting `TypeKey::Object`).
    3.  Do not produce a property named `"never"`.

## 42. CFA Invalidation in Closures
**Behavior:** Type Narrowing (Control Flow Analysis) works linearly. However, inside a callback/closure, narrowing is often **reset** or invalidated because the callback might run *after* the variable has changed.
*   `let x: string | number = "hello";`
*   `if (typeof x === "string") {`
    *   `x` is `string` here.
    *   `function callback() { console.log(x.toUpperCase()); }` // **Error!**
    *   TS invalidates the narrowing inside `callback` because `x` is mutable and `callback` is deferred.
*   `}`
**Example:** `tests/cases/conformance/controlFlow/controlFlowInLoop.ts`
**Solver Rule:**
*   **Contextual Typing:** When checking a Function Expression:
    *   Identify all captured variables from parent scopes.
    *   If a variable is mutable (`let`/`var`), **ignore** any narrowing predicates currently active in the outer scope. Reset its type to the declared type.
    *   If `const`, maintain narrowing (safe).

## 43. Abstract Class Instantiation
**Behavior:**
*   `AbstractClass` cannot be instantiated: `new Abstract()` -> Error.
*   **However**, `AbstractClass` *is* a subtype of `Function` (it has a prototype).
*   And you can define a type that accepts abstract constructors: `type Ctor = abstract new () => any;`
**Example:** `tests/cases/conformance/classes/classDeclarations/abstractKeywords.ts`
**Solver Rule:**
*   **Subtyping:**
    *   `ConcreteConstructor <: AbstractConstructor` -> **True**.
    *   `AbstractConstructor <: ConcreteConstructor` -> **False**.
    *   `AbstractConstructor <: Function` -> **True**.
*   **Expression Check:** `NewExpression(Target)`: If `Target` resolves to a Class Symbol marked `ABSTRACT`, emit error "Cannot create an instance of an abstract class."

## 44. Module Augmentation Merging
**Behavior:** Interfaces with the same name in the same scope **merge**. This crosses module boundaries via `declare module "..."`.
*   File A: `interface Window { a: string }`
*   File B: `interface Window { b: number }`
*   Result: `Window` has both `a` and `b`.
**Example:** `tests/cases/conformance/externalModules/moduleAugmentation.ts`
**Solver Rule:**
*   **Binder/Lowering:** When resolving `SymbolId("Window")`:
    *   Do not stop at the first declaration.
    *   Collect **all** declarations associated with that Symbol ID across all files.
    *   **Synthesis:** Merge members from all declarations into a single `TypeKey::Object`.
    *   *Conflict Resolution:* If properties collide, usually the first one wins or they become overloads (for methods), but for value props, it's often an error or union.















## Implementation Priority

Do not attempt to implement all 44 rules at once. Follow this phased approach to maintain a working compiler at each step.

### 🚨 Phase 1: The "Hello World" Barrier (Bootstrapping)
*Goal: Compile `lib.d.ts` and basic variables without crashing or false errors.*

These rules are required because the standard library relies on them heavily. Without these, even `const x: string = "a"` might fail if it interacts with global interfaces.

1.  **#1 The "Any" Type:** The universal lubricant. Nothing works without short-circuiting `any`.
2.  **#20 The `Object` vs `object` vs `{}` Trifecta:** Primitives must be assignable to the global `Object` interface.
3.  **#6 Void Return Exception:** Callbacks in `lib.d.ts` (like `Array.forEach`) rely on this.
4.  **#11 Error Poisoning:** Essential for debugging the compiler itself (prevents one bug from looking like 100).
5.  **#3 Covariant Mutable Arrays:** `ReadonlyArray` and `Array` relationships break without this.

### 🚧 Phase 2: The "Business Logic" Barrier (Common Patterns)
*Goal: Compile standard application code involving functions, classes, and object literals.*

1.  **#2 Function Bivariance:** Methods in classes will reject valid overrides without this.
2.  **#4 Freshness / Excess Properties:** Prevents valid object literals from being flagged as errors.
3.  **#10 Literal Widening:** Essential for `let` and `var` bindings to work intuitively.
4.  **#19 Covariant `this`:** Critical for Fluent APIs (e.g., `builder.add().build()`).
5.  **#14 Optionality vs Undefined:** Standard optional parameters won't match without this logic.

### 🛠 Phase 3: The "Library" Barrier (Complex Types)
*Goal: Compile modern npm packages (Zod, React, tRPC) which use advanced type algebra.*

1.  **#25 Index Signature Consistency:** Validates dictionary types used in libraries.
2.  **#40 Distributivity Disabling:** Used heavily in conditional type logic.
3.  **#30 `keyof` Contravariance:** Essential for `Pick`, `Omit`, and mapped types.
4.  **#21 Intersection Reduction:** Prevents "impossible" types from propagating.
5.  **#41 Key Remapping (`as never`):** The engine behind the `Omit` utility type.

### 🔮 Phase 4: The "Feature" Barrier (Edge Cases)
*Goal: 100% Compliance with the Test Suite.*

*   **Enums:** #7 (Open Numbers), #24 (Nominal), #34 (String Opaque).
*   **Classes:** #5 (Private Nominal), #18 (Static Side), #43 (Abstract).
*   **Module Interop:** #39 (Import Type), #44 (Augmentation).
*   **JSX:** #36 (Intrinsic Lookup).
*   **The Rest:** #13 (Weak Types), #17 (Depth Limits), etc.

---

### Execution Strategy

1.  **Build the "Judge" (Core) first.** Make sure it passes standard set-theory tests.
2.  **Build the "Lawyer" (Compat) wrapper.**
3.  **Implement Phase 1 rules.** Verify by trying to load a minimized `lib.d.ts`.
4.  **Implement Phase 2 rules.** Verify by running simple source files with functions/classes.
5.  **Iterate.** Use the official `tests/cases` suite to drive Phase 3 and 4 implementation.