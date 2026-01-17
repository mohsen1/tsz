# Design Document: Semantic Structural Solver for TypeScript

## 1. Theoretical Foundations

**Goal:** Establish the mathematical ground truth for the type system.

To build a correct solver, we must move away from viewing types as "AST nodes to be compared" and view them as **Sets of Values**. This "Semantic Subtyping" approach provides the rigorous framework necessary to handle TypeScript's complex features like unions, intersections, and narrowing.

### 1.1 Types as Sets (The Universe $\mathcal{U}$)

A type in TypeScript is fundamentally a predicate that defines a subset of all possible JavaScript values. Let $\mathcal{U}$ be the universe of all valid runtime values. A type $T$ is strictly defined as:

$$T = \{ v \in \mathcal{U} \mid \text{predicate}_T(v) \text{ is true} \}$$

#### The Hierarchy
*   **The Top Type ($\top$):** The set containing *all* values.
    *   TS Syntax: `unknown` (and loosely `any`, though `any` implies unsoundness).
    *   Math: $\top = \mathcal{U}$
*   **The Bottom Type ($\bot$):** The empty set. No value satisfies this.
    *   TS Syntax: `never`
    *   Math: $\bot = \emptyset$
*   **Unit Types (Singletons):** A set containing exactly one specific value.
    *   TS Syntax: `"hello"`, `42`, `true`
    *   Math: $S = \{ \text{"hello"} \}$

#### TypeScript Example
```typescript
// The set of all strings
type S = string;

// The set containing exactly one string
type Hello = "hello";

// The empty set (intersection of disjoint sets)
type Empty = string & number; // never
```

> **Implication for Rust:** Our `TypeKey` enum must include `Intrinsic(Unknown)` and `Intrinsic(Never)` as distinct variants that act as the identity/absorbing elements in set operations.

---

### 1.2 The Subtyping Relation ($<:$)

Subtyping is defined strictly as **Set Inclusion**. Type $S$ is a subtype of $T$ ($S <: T$) if and only if every value in the set $S$ is also in the set $T$.

$$S <: T \iff S \subseteq T \iff \forall v, (v \in S \implies v \in T)$$

#### The Inverse Relationship: Properties vs. Values
In structural typing, this creates an inverse relationship between the "size" of the interface (number of properties) and the "size" of the set (number of allowed values).

*   **More constraints (properties) = Smaller Set.**
*   **Fewer constraints (properties) = Larger Set.**

#### TypeScript Example
```typescript
interface Animal {
    name: string;
}
// Set A: All objects with AT LEAST a 'name' string.

interface Dog {
    name: string;
    breed: string;
}
// Set D: All objects with AT LEAST 'name' AND 'breed'.

// SUBTYPING CHECK:
// Dog <: Animal
// Because every object that has (name, breed) AUTOMATICALLY has (name).
// Therefore, the set of Dogs is a SUBSET of Animals.
```

> **Implication for Rust:** When implementing `solve_subtype(sub, sup)`, if both are Objects, we must verify that `sub` contains **all keys** present in `sup`. It is acceptable for `sub` to have *extra* keys (width subtyping).

---

### 1.3 Set-Theoretic Primitives

TypeScript types form a lattice ordered by subset inclusion. The solver must implement the algebraic operations of this lattice.

#### Union ($\cup$)
The logical **OR**. A value is in $A \mid B$ if it is in $A$ *or* in $B$.
*   **Law:** $A \cup B \equiv B \cup A$ (Commutativity)
*   **Simplification:** $A \cup A \equiv A$ (Idempotence)
*   **Subtyping:** $A <: (A \cup B)$

#### Intersection ($\cap$)
The logical **AND**. A value is in $A \& B$ if it is in $A$ *and* in $B$.
*   **Law:** $A \cap (B \cup C) \equiv (A \cap B) \cup (A \cap C)$ (Distributivity)
*   **Subtyping:** $(A \cap B) <: A$

#### TypeScript Example: Distributivity
Distributivity is critical for correct narrowing.

```typescript
type A = { id: 1 };
type B = { id: 2 };

// Intersection of Union
type T1 = (A | B) & { active: boolean };

// Distributes to:
// ({ id: 1 } & { active: boolean }) | ({ id: 2 } & { active: boolean })
```

> **Implication for Rust:** The solver cannot just store nested `Union(Intersection(Union(...)))` trees. It must aggressively **normalize** types into a canonical form (e.g., Disjunctive Normal Form) during the Interning phase to make comparison efficient.

---

### 1.4 Coinduction (The Greatest Fixed Point)

Standard (Inductive) logic proves statements by breaking them down to base cases.
*   *Inductive Proof:* $0 < 5$ because $0 < 1 < 2 < 3 < 4 < 5$.

Structural types often have no base case (recursion).
*   *Recursive Type:* `type List = { next: List | null }`

If we try to prove `List <: List` inductively, we enter an infinite loop:
`List <: List` $\to$ `List.next <: List.next` $\to$ `List <: List` $\to$ ...

#### The Coinductive Principle
We define equality/subtyping as the **Greatest Fixed Point (GFP)** of the generating function.
Practically, this means:
> "Two recursive types are equivalent unless there is a finite path of property access that reveals a difference (a contradiction)."

If we traverse a cycle and return to a node we are already visiting without finding a contradiction, the result is **Provisionally True**.

#### TypeScript Example
```typescript
interface A {
    value: string;
    child: A;
}

interface B {
    value: string;
    child: B;
}

// Q: Is A <: B?
// 1. Check value: string <: string (OK)
// 2. Check child: A <: B ...
//    CYCLE DETECTED: We are already asking "Is A <: B?"
//    Coinductive Rule: Return TRUE.
// Result: A <: B is TRUE.
```

#### Counter-Example (The Contradiction)
```typescript
interface A { x: A }
interface B { x: B; y: number }

// Q: Is A <: B?
// 1. Check x: A <: B (Cycle -> assume True)
// 2. Check y: A has no 'y'. FAIL.
// Result: A is NOT subtype of B.
```

> **Implication for Rust:** The `Salsa` query system or our custom `SolverEngine` must maintain a `CycleStack`. When `solve_subtype(A, B)` is called, we first check if `(A, B)` is in the stack.
> *   If **Yes**: Return `true` (Stop recursion).
> *   If **No**: Push `(A, B)`, compute children, pop `(A, B)`.

## 2. The Type Universe (Representation)

**Goal:** Define how mathematical sets are encoded in Rust memory to ensure $O(1)$ equality checks and cache-friendly traversal.

In the legacy architecture, types were heap-allocated structs scattered across memory. In the **Semantic Solver**, types are **Interned Data**. We separate the *Identity* of a type (an Integer) from its *Structure* (the Data).

### 2.1 The Canonical ID (`TypeId`)

The `TypeId` is the atomic currency of the solver. It is a 32-bit integer that represents a unique, immutable set of values.

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub u32);
```

*   **Size:** 4 bytes.
*   **Semantics:** If `id_a == id_b`, then Type A and Type B are mathematically identical. No deep comparison is ever needed for equality.
*   **Performance:** Fits in registers; extremely cheap to copy and pass into queries.

---

### 2.2 The Type Key (`TypeKey`)

The `TypeKey` is the "Definition" of the type. It is the input to the interner. Once interned into a `TypeId`, the Key is stored in a contiguous central storage (Arena/Jar) and rarely accessed unless we are actively solving a query for that specific ID.

```rust
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum TypeKey {
    // 1. The Basics
    Intrinsic(IntrinsicKind),     // string, number, any, never...
    Literal(LiteralValue),        // "foo", 42, true

    // 2. Structural Shapes
    // Defined by a set of properties.
    // INVARIANT: Must be sorted by Atom to ensure canonical uniqueness.
    Object(Vec<(Atom, TypeId)>),

    // 3. Algebraic Sets
    // INVARIANT: Must be flattened and sorted by TypeId.
    Union(Vec<TypeId>),           // A | B
    Intersection(Vec<TypeId>),    // A & B

    // 4. The Knot (Recursion)
    // A reference to a named declaration (Interface/Class).
    // The solver resolves this lazily to handle cycles.
    Ref(SymbolId), 
    
    // 5. Meta-Types (Unresolved)
    // Conditional types, Mapped types, etc. waiting for instantiation.
    Conditional(Box<ConditionalType>),
}
```

#### TypeScript Mapping Example

**1. Object Shapes (Normalization)**
To ensure `TypeId` uniqueness, object properties must be **canonicalized** (sorted).

```typescript
// File A
type A = { x: number; y: string };

// File B (Same structure, different order)
type B = { y: string; x: number };
```

*   **Representation:**
    Both `A` and `B` lower to the **exact same** `TypeKey`:
    ```rust
    TypeKey::Object(vec![
        (atom("x"), type_number),
        (atom("y"), type_string),
    ])
    ```
*   **Result:** `TypeId(A) == TypeId(B)`. The solver sees them as the same integer.

**2. Algebraic Sets (Flattening)**
Nested unions must be **flattened** to satisfy set theory associativity rules ($A \cup (B \cup C) \equiv A \cup B \cup C$).

```typescript
type X = string | number;
type Y = X | boolean; // (string | number) | boolean
```

*   **Representation:**
    `Y` is stored as a flat list:
    ```rust
    TypeKey::Union(vec![type_boolean, type_number, type_string]) // Sorted by ID
    ```

---

### 2.3 The Interning Strategy

The Interner acts as the "Hash Consing" engine. It ensures that we never store duplicate shapes.

**The Algorithm:**
1.  **Lowering:** Convert AST node to `TypeKey`.
2.  **Normalization:**
    *   Sort object keys.
    *   Flatten unions/intersections.
    *   Prune identities (e.g., remove `never` from Union, remove `unknown` from Intersection).
3.  **Lookup:** Check `HashMap<TypeKey, TypeId>`.
    *   **Hit:** Return existing `TypeId`.
    *   **Miss:** Allocate new slot in `Vec<TypeKey>`, insert into Map, return new `TypeId`.

#### Why This Matters for Recursion
Recursive types in TS are typically "Named" (Interfaces or Classes). We represent these as `TypeKey::Ref(SymbolId)`.

```typescript
// interface List { next: List | null }
// SymbolId(100) = "List"
```

1.  **Parsing `List`:** We generate `TypeId(50)` which maps to `TypeKey::Ref(SymbolId(100))`.
2.  **Solving:** When the solver hits `TypeId(50)`, it asks: "What is the shape of `SymbolId(100)`?"
3.  **Laziness:** The database looks up the AST for `SymbolId(100)`, lowers it, and returns the structural shape:
    `Object { next: Union(Ref(100), Null) }`.

This indirection (`Ref`) allows us to represent infinite trees using finite memory.

---

### 2.4 Performance Primitives

To make this architecture performant in Rust/WASM, we rely on specific low-level optimizations:

1.  **`Atom` (u32):** String interning for property names. Comparing `"propertyDescription"` vs `"propertyDescriptor"` becomes a single integer comparison.
2.  **`SmallVec`:** Most objects have few properties; most unions have few variants. Using `SmallVec<[TypeId; 4]>` avoids heap allocation for the 90% case.
3.  **`BitFlags`:** We store metadata on the `TypeId` (in a parallel array) to allow fast rejection.
    *   `flags[id].is_truthy()`
    *   `flags[id].has_object_structure()`
    *   This allows `check_truthiness(id)` to run without even looking up the `TypeKey`.

---

## 3. The Logic Engine (Subtyping Algorithms)

**Goal:** Formalize the algorithmic rules that prove $S <: T$ (S is a subtype of T).

The Logic Engine is a recursive state machine that takes two `TypeId`s and returns a Boolean. Unlike the legacy compiler, which relies on ad-hoc heuristics, this engine strictly applies **Semantic Subtyping** rules derived from set theory and coinduction.

The central query is:
```rust
fn solve_subtype(db: &dyn Db, sub: TypeId, sup: TypeId) -> bool
```

---

### 3.1 The Coinductive Loop (Cycle Handling)

As established in Section 1.4, recursive types require **Greatest Fixed Point (GFP)** semantics. We cannot simply recurse, or we will stack overflow on trees like `interface Node { next: Node }`.

**The Algorithm:**
1.  **Memoization Check:** Has `solve_subtype(sub, sup)` already been computed? If yes, return cached result.
2.  **Cycle Detection:** Is the pair `(sub, sup)` currently on the active query stack?
    *   **Yes:** We have encountered a cycle. Under GFP semantics (assuming no contradictions found yet), we return **`true`** (The "Provisional Yes").
    *   **No:** Proceed.
3.  **Expansion:** Lower `sub` and `sup` to their `TypeKey` structures.
4.  **Verification:** Run the specific logic rule (Object, Union, Primitive) defined below.
5.  **Caching:** Store the result. *Note: Results depending on a "Provisional Yes" are cached contextually to the cycle.*

#### TypeScript Example
```typescript
interface A { data: string; next: A }
interface B { data: string | number; next: B }

// Query: Is A <: B?
// 1. Check A.data <: B.data ("string" <: "string | number") -> TRUE
// 2. Check A.next <: B.next (A <: B)
//    -> CYCLE DETECTED! (A, B) is on stack.
//    -> Return TRUE (Provisional).
// 3. Result: TRUE && TRUE = TRUE.
```

---

### 3.2 Structural Object Logic

The core of TypeScript is structural compatibility.
Rule: **$S$ is a subtype of $T$ if $S$ has all properties required by $T$, and their types are compatible.**

$$S <: T \iff \forall (k, \tau_{sup}) \in T_{\text{props}}, \exists (k, \tau_{sub}) \in S_{\text{props}} : \tau_{sub} <: \tau_{sup}$$

#### The Implementation Strategy
Since our `TypeKey::Object` stores properties as a **Sorted Vector**, we can perform this check efficiently (linear scan).

1.  Iterate through properties of `Sup` (the requirement).
2.  For each property `P` in `Sup`:
    *   Binary search for `P` in `Sub`.
    *   **Missing?** Return `false` (Structure mismatch).
    *   **Found?** Recursively call `solve_subtype(sub_prop_type, sup_prop_type)`.
        *   If `false`, return `false`.
3.  If loop finishes, return `true`.

#### Width Subtyping (Extra Properties)
This algorithm naturally supports "Width Subtyping." If `Sub` has extra properties that `Sup` does not mention, the loop simply ignores them.

```typescript
type Sup = { x: number };
type Sub = { x: 1; y: string }; // 'y' is ignored
// Sub <: Sup is Valid.
```

---

### 3.3 Algebraic Logic (Unions & Intersections)

Handling $A | B$ and $A \& B$ requires applying set-theoretic distribution rules.

#### Case 1: Sub is a Union ($A | B <: T$)
For a Union to be a subtype of $T$, **every** variant in the union must be compatible with $T$.
$$ (A \cup B) \subseteq T \iff (A \subseteq T) \land (B \subseteq T) $$

*   **Logic:** `solve_subtype(A, T) && solve_subtype(B, T)`

#### Case 2: Sup is a Union ($S <: A | B$)
This is more complex. Simplistically, $S$ must match *one* of the variants.
$$ S \subseteq (A \cup B) \leftarrow (S \subseteq A) \lor (S \subseteq B) $$
*   **Logic:** `solve_subtype(S, A) || solve_subtype(S, B)`
*   *Note:* Real TS handles "Discriminated Unions" where $S$ might partially overlap A and partially overlap B, but this is the baseline rule.

#### Case 3: Sub is an Intersection ($A \& B <: T$)
If you have *more* constraints ($A$ AND $B$), you are a smaller set. You are a subtype if *either* part satisfies $T$ (simplification) or if the composite satisfies $T$.
Usually: `solve_subtype(A, T) || solve_subtype(B, T)` (if structural).
*   *Refinement:* If $T$ is `{x: number}`, and $A=\{x: \dots\}$, $B=\{y: \dots\}$, we effectively merge $A$ and $B$ before checking.

---

### 3.4 Function Variance

Function types behave counter-intuitively regarding their parameters.
Given `type Fn = (param: P) => R`:

$$ Fn_{sub} <: Fn_{sup} \iff (R_{sub} <: R_{sup}) \land (P_{sup} <: P_{sub}) $$

1.  **Return Type (Covariant):** The return value of Sub must be *safer* (subset) than Sup.
    *   `() => Cat` is a subtype of `() => Animal`.
2.  **Parameter Type (Contravariant):** The argument of Sub must accept *at least* what Sup accepts (superset).
    *   `(x: Animal) => void` is a subtype of `(x: Cat) => void`.
    *   *Reasoning:* If I expect a function that handles `Cat`, and you give me one that handles *any* `Animal`, that is safe. If you give me one that only handles `PersianCat`, it might crash when I pass a generic `Cat`.

#### TypeScript Example
```typescript
type Handler = (e: Event) => void;
type ClickHandler = (e: MouseEvent) => void;

// ERROR: Handler is NOT a subtype of ClickHandler?
// NO, Wait:
// Target: (e: MouseEvent) => void
// Assign: (e: Event) => void
// Check Args: MouseEvent <: Event? YES.
// Result: Safe.
```

> **Implication for Rust:** The `TypeKey::Function` variant must store parameters and return types. The solver explicitly flips the `(sub, sup)` arguments when checking parameters: `solve_subtype(sup_param, sub_param)`.

## 4. Meta-Type Resolution (Advanced Features)

**Goal:** Formalize how the solver handles types that *compute* other types (Meta-Types).

Unlike Java or C#, TypeScript types are a Turing-complete functional programming language. Features like **Conditional Types**, **Mapped Types**, and **Index Access Types** act as functions that take input types and produce output types. The solver must evaluate these "programs" during the subtyping check.

---

### 4.1 Conditional Types ($T \text{ extends } U ? X : Y$)

Conditional types are logic gates. They branch based on a subtyping check.

**Representation:**
```rust
TypeKey::Conditional {
    check_type: TypeId,   // T
    extends_type: TypeId, // U
    true_branch: TypeId,  // X
    false_branch: TypeId  // Y
}
```

**Evaluation Algorithm:**
When the solver encounters a `Conditional` type key, it performs a **Speculative Subtyping Check**:

1.  **Check:** Run `solve_subtype(check_type, extends_type)`.
2.  **Branching:**
    *   **True ($T \subseteq U$):** The type resolves to `true_branch`.
    *   **False ($T \cap U = \emptyset$):** If $T$ and $U$ are disjoint, the type resolves to `false_branch`.
    *   **Ambiguous ($T$ overlaps $U$):** This happens with generic parameters (e.g., $T$ is unresolved). The type remains **Deferred**.

#### The Deferral Problem
If `check_type` is a generic variable `T` (not yet inferred), we cannot decide the branch. We must keep the type in its symbolic form: `TypeKey::Conditional(...)`.
Subtyping rules for Deferred Conditionals are complex:
*   $Cond(T) <: S$ usually requires proving that *both* branches ($X$ and $Y$) are subtypes of $S$ (conservative).

---

### 4.2 Distributivity (The Map Operation)

TypeScript Conditional Types have a special behavior: **Distributivity**.
If the checked type $T$ is a naked type parameter, and it is instantiated with a **Union**, the conditional applies to *each member* of the union independently.

$$ (A \mid B) \text{ extends } U ? X : Y \equiv (A \text{ extends } U ? X : Y) \mid (B \text{ extends } U ? X : Y) $$

**Algorithm:**
1.  Is `check_type` a Union?
2.  **Yes:**
    *   Iterate over each variant $V$ in the union.
    *   Evaluate `Conditional(V, extends_type, true, false)`.
    *   Collect results into a new `TypeKey::Union`.
    *   Intern and return the new ID.
3.  **No:** Proceed with standard evaluation (4.1).

#### TypeScript Example
```typescript
type ToArray<T> = T extends any ? T[] : never;
type Result = ToArray<string | number>;
// 1. Distribute: ToArray<string> | ToArray<number>
// 2. Evaluate: string[] | number[]
```

---

### 4.3 Mapped Types (Homomorphisms)

Mapped types transformation object shapes: `{ [K in Keys]: Transform<K> }`.

**Representation:**
```rust
TypeKey::Mapped {
    source: TypeId,       // The type being iterated (usually keyof T)
    template: TypeId,     // The value transformation
    modifiers: Modifiers, // readonly, optional (-?)
    name_map: Option<TypeId> // 'as' clause for key remapping
}
```

**Evaluation Algorithm (Instantiation):**
When a Mapped Type is applied to a concrete Object Type:
1.  **Extract Keys:** Calculate `K = keyof SourceObject`.
2.  **Iterate:** For each literal key $k \in K$:
    *   **Instantiate Template:** Replace the generic inference variable in `template` with $k$.
    *   **Instantiate Name:** If `name_map` exists, compute the new key name.
    *   **Apply Modifiers:** Add/remove `readonly` or `?` flags.
3.  **Synthesize:** Construct a new `TypeKey::Object` with the resulting properties.
4.  **Intern:** Return the new `TypeId`.

---

### 4.4 Index Access Types ($T[K]$)

Accessing a property via a type: `Person["age"]`.

**Representation:**
```rust
TypeKey::IndexAccess {
    object_type: TypeId, // T
    index_type: TypeId   // K
}
```

**Evaluation Algorithm:**
1.  **Resolve T:** If $T$ is an Object, find property $P$ matching $K$.
    *   If $K$ is a single string literal `"age"`, return type of `age`.
    *   If $K$ is a Union `"age" | "name"`, return Union `T["age"] | T["name"]`.
2.  **Distribute T:** If $T$ is a Union ($A \mid B$), distribute: $A[K] \mid B[K]$.
3.  **Array Access:** If $T$ is `Array<E>` and $K$ is `number`, return $E$.
4.  **Tuple Access:** If $T$ is `[string, number]`:
    *   $K=0 \to$ `string`
    *   $K=1 \to$ `number`
    *   $K=\text{number} \to$ `string | number`

#### TypeScript Example
```typescript
type Tuple = [string, number];
type Val = Tuple[number]; // string | number
```

---

### 4.5 The "defer" Strategy vs. "eager" Strategy

A critical design decision in Phase 7.5 is **Laziness**.

*   **Eager Evaluation:** When we define `type X = ...`, do we compute the final structure immediately?
    *   *Pros:* Fast lookups later.
    *   *Cons:* Infinite loops in recursive type definitions; wasted work.
*   **Lazy (Deferred) Evaluation:** We store the *recipe* (`TypeKey::Conditional`). We only compute the result when `solve_subtype` actually reaches it.

**Design Decision:** We adopt **Lazy Evaluation with Memoization**.
The `TypeKey` enum retains the high-level constructs (`Conditional`, `Mapped`). The `solve_subtype` function calls a `normalize(id)` helper that attempts to reduce these Meta-Types to concrete Primitives/Objects only when strictly necessary. This prevents stack overflows in recursive type definitions that are never actually instantiated.

---
## 5. Constraint Solving (Inference)

**Goal:** Formalize how the solver infers missing types (Generics) by solving a system of constraints.

Unlike the static verification of `solve_subtype(S, T)`, inference is dynamic. We start with unknown variables (e.g., `<T>`) and "narrow" them down until they fit all observed usage patterns.

---

### 5.1 Unification Variables ($\alpha$)

In a function call like `identity<T>(x)`, the type `T` is unknown. We represent this as an **Inference Variable** (often denoted $\alpha, \beta$ or `?0, ?1`).

**Representation:**
The `ena` crate (Union-Find) manages these variables.
```rust
// In the InferenceContext
struct InferenceVar(u32);

// In the TypeKey universe
TypeKey::InferenceVar(InferenceVar)
```

**State:**
Each variable starts as **Unbound**.
During solving, it can become:
1.  **Bound to a Concrete Type:** $\alpha \to \text{string}$
2.  **Bound to another Variable:** $\alpha \to \beta$ (Aliasing)
3.  **Constrained:** Lower Bound $L <: \alpha$ and Upper Bound $\alpha <: U$.

---

### 5.2 Constraint Collection

Inference works by walking the AST and generating mathematical constraints.

**Scenario:**
```typescript
function foo<T>(a: T, b: T): T { ... }
foo("hello", 42);
```

**Step 1: Instantiation**
Replace `T` with a fresh variable `?0`.
Signature becomes: `(a: ?0, b: ?0) => ?0`.

**Step 2: Argument Matching**
*   Arg 1: `"hello"` assigned to `a: ?0`.
    *   **Constraint 1:** `"hello" <: ?0` (Lower Bound).
*   Arg 2: `42` assigned to `b: ?0`.
    *   **Constraint 2:** `42 <: ?0` (Lower Bound).

**Step 3: Resolution**
We need a type $T$ such that:
$$ \{ \text{"hello"}, 42 \} \subseteq T $$
The most precise type (Best Common Type) is the Union: `"hello" | 42`.
Thus, `?0` resolves to `"hello" | 42`.

---

### 5.3 Bounds Checking ($L <: \alpha <: U$)

Simple unification ($\alpha = T$) isn't enough for TypeScript. We often have ranges.

*   **Upper Bound ($<: U$):** From `extends` clauses.
    `function f<T extends Animal>(x: T)` $\implies \alpha <: \text{Animal}$.
*   **Lower Bound ($L <:$):** From arguments passed *into* the function.
    `f(myCat)` $\implies \text{Cat} <: \alpha$.

**The Solving Algorithm:**
1.  Collect all Upper Bounds into a set $U_{set}$.
2.  Collect all Lower Bounds into a set $L_{set}$.
3.  **Validation:** Verify that for every $l \in L_{set}$ and $u \in U_{set}$, $l <: u$.
4.  **Selection:**
    *   Usually, we pick the **Union of Lower Bounds** ($\bigcup L_{set}$) as the inferred type (widening).
    *   If no Lower Bounds, we default to the **Constraint** (Upper Bound).
    *   If no Constraint, we default to `unknown` (or `inference fail`).

---

### 5.4 Contextual Typing (Reverse Inference)

TypeScript is unique because type information flows **both ways**.
Usually: `Expression` $\to$ `Type`.
Contextual: `Type` $\to$ `Expression`.

**Scenario:**
```typescript
type Handler = (e: string) => void;
const h: Handler = (x) => { ... }; 
// 'x' is inferred as 'string' because of 'Handler'
```

**Algorithm:**
1.  **Identify Context:** The assignment target is `Handler`.
2.  **Push Context:** When checking the arrow function `(x) => ...`, we pass `Handler` as the *Contextual Type*.
3.  **Extraction:** The arrow function analyzer looks at `Handler`.
    *   "I am being assigned to a function type."
    *   "The first parameter of that target is `string`."
4.  **Inference:** Therefore, parameter `x` has type `string`.

**Implementation in Solver:**
The `check_expression` query accepts an optional `expected_type: Option<TypeId>`.
*   If `Some(T)`, the checker uses $T$ to hint inference variables inside the expression logic.
*   This creates a "meet" operation where the expression's intrinsic type and the contextual type meet to determine the final type.

---

### 5.5 Generic Instantiation

Once inference is complete, we must **Substitute** the concrete types back into the signature.

**Input:** `(a: ?0, b: ?0) => ?0`
**Solution:** `?0 = "hello" | 42`
**Output (Substitution):** `(a: "hello"|42, b: "hello"|42) => "hello"|42`

**Deep Substitution:**
This process must be recursive. If `?0` was used inside a nested object or function return type, we must traverse that structure (lazily or eagerly) and replace the variable with the solution.

## 6. Operational Architecture

**Goal:** Concrete mapping of the theoretical framework to the Rust ecosystem (`salsa`, `ena`) and the `ThinNode` architecture.

This section defines the **Physical Layout** of the solver. We move from "Mathematical Sets" to "Rust Structs" and "Salsa Queries."

---

### 6.1 The Query Graph (Salsa Integration)

The type checker is structured as a **Salsa Database**. The database is a collection of "Jars" (modules) that contain **Inputs** (Source Code) and **Derived Queries** (Types).

#### The Type Jar
This module defines the universe of types.

```rust
#[salsa::jar(db = Db)]
pub struct TypeJar(
    // The "Atom" of the system (Interned Type)
    Type,
    
    // The Input: The Semantic Model of the Program
    ProgramSource,

    // Core Queries
    lower_type,         // ThinNode -> Type
    solve_subtype,      // (Type, Type) -> bool
    check_expression,   // (ExpressionNode) -> Type
    resolve_symbol,     // (SymbolId) -> Type
);

pub trait Db: salsa::DbWithJar<TypeJar> {}
```

#### The Type Data (`Type`)
In Salsa, interned types are opaque wrappers around a `u32` integer. This matches our theoretical `TypeId`.

```rust
#[salsa::interned(jar = TypeJar)]
pub struct Type {
    #[return_ref]
    pub key: TypeKey, // The Enum defined in Section 2.2
}
```

#### The Query Functions
We define pure functions that the database manages.

1.  **`lower_type(db, node)`:** Converts an AST node to a Type ID.
2.  **`solve_subtype(db, sub, sup)`:** The caching logic engine.
3.  **`type_of_symbol(db, symbol)`:** Lazily computes the type of a variable/function from its declaration.

---

### 6.2 The Lowering Bridge (Synthesis)

The **Bridge** connects the raw `ThinNode` AST (Phase 0.1) to the Semantic Solver. This is a **Just-In-Time (JIT) Compiler** for types.

**Input:** `ThinNode` (Syntax)
**Output:** `Type` (Semantics)

**Algorithm:**
```rust
fn lower_type(db: &dyn Db, node: ThinNodeId) -> Type {
    let arena = db.program_source().arena();
    let node_ref = arena.get(node);

    match node_ref.kind() {
        // Primitives are instant
        Kind::StringKeyword => Type::new(db, TypeKey::Intrinsic(Intrinsic::String)),
        
        // Literals (e.g., "hello")
        Kind::StringLiteral => {
            let text = arena.get_token_value(node);
            Type::new(db, TypeKey::Literal(Literal::String(text)))
        }

        // Interfaces (The heavy lifting)
        Kind::InterfaceDeclaration => {
            let interface = arena.get_interface(node);
            let mut props = Vec::new();
            
            for member in interface.members {
                let name = arena.get_name(member);
                // RECURSIVE QUERY:
                // We don't manually recurse; we ask the DB.
                // If this cycle loops, Salsa/Solver handles it.
                let prop_type = lower_type(db, member.type_annotation);
                props.push((name, prop_type));
            }
            
            // Sort for canonical identity
            props.sort_by(|a, b| a.0.cmp(&b.0));
            
            Type::new(db, TypeKey::Object(props))
        }
        
        // ... handle Unions, Functions, etc.
    }
}
```

---

### 6.3 The Solver Context (Inference State)

Salsa queries are **Stateless** (Pure Functions). However, **Inference** (Section 5) requires mutable state (the Unification Table).

**Solution:** The `SolverContext` is a transient object created *inside* a query.

```rust
pub struct SolverContext<'db> {
    db: &'db dyn Db,
    
    // The "ena" Unification Table for generic variables (?0, ?1)
    inference_table: InPlaceUnificationTable<InferenceVar>,
    
    // Bounds for variables
    constraints: HashMap<InferenceVar, ConstraintSet>,
}

impl<'db> SolverContext<'db> {
    pub fn new(db: &'db dyn Db) -> Self { ... }

    pub fn unify(&mut self, a: Type, b: Type) -> Result<(), TypeError> {
        // 1. Resolve 'a' and 'b' (follow pointers in table)
        // 2. If one is a variable, bind it.
        // 3. If both are concrete, call db.solve_subtype(a, b).
    }
}
```

**Lifecycle:**
1.  **Query Start:** `check_function_body(db, func_node)` is called.
2.  **Context Creation:** `let mut ctx = SolverContext::new(db);`
3.  **Execution:** The checker runs over the body, mutating `ctx.inference_table`.
4.  **Finalization:** We "Snapshot" the final types.
5.  **Return:** The query returns the *Concrete Types* (interned in Salsa) and discards the mutable context.

---

### 6.4 Error Propagation (The "Poison Pill")

In a high-performance compiler, we must handle errors gracefully without cascading noise.

**The Error Type:**
```rust
TypeKey::Error
```

**Semantics:**
*   **Subtyping:** `Error` is both a subtype and supertype of everything.
    *   `Error <: String` (True)
    *   `String <: Error` (True)
*   **Propagation:** Any operation involving `Error` yields `Error`.
    *   `Union(Error, String)` -> `Error` (Simplification rule to suppress noise)
    *   `PropertyAccess(Error, "foo")` -> `Error`

**Why:** This prevents "Cascading Errors." If `const x = garbage();` fails, `x` becomes `Error`. Subsequent usage `x.foo()` is strictly valid (because `Error` accepts `foo`), suppressing a second error message for the same root cause.

---

### 6.5 Parallelism Strategy

Since `salsa` manages the dependency graph, parallelism is largely "free," but we must configure the runtime correctly.

1.  **File-Level Parallelism:** The `ProgramSource` input is split by file. Changes to `A.ts` invalidate queries dependent on `A.ts`, but `B.ts` queries remain Green (Cached).
2.  **Thread Pool:** We use `rayon` to drive the Salsa runtime.
3.  **Coinduction Safety:** Salsa's cycle recovery mechanism must be thread-safe. (Salsa 2022 handles this via revision numbers and deterministic fallback).

---

## 8. The Compatibility Layer ("Formalizing the Informal")

**Goal:** Support TypeScript’s intentional unsoundness (quirks) without corrupting the mathematical integrity of the Logic Engine.

TypeScript contains specific behaviors—such as `any` acting as both a subtype and supertype, or bivariant function arguments—that violate strict set theory. To handle this, we adopt a **"Judge vs. Lawyer"** architecture.

### 8.1 Architectural Separation

1.  **The Judge (Core Logic):** The `solve_subtype_pure` query. It implements strict, sound set theory (Covariance, Contravariance, Greatest Fixed Point). It knows nothing about "legacy behavior."
2.  **The Lawyer (Compat Layer):** The `solve_subtype` query (Public API). It intercepts requests, applies TypeScript-specific "Business Logic" (unwrapping `any`, checking legacy flags), and *then* delegates to the Judge with specific instructions.

**Salsa Implementation Strategy:**

```rust
#[salsa::tracked]
pub fn solve_subtype(db: &dyn Db, sub: Type, sup: Type) -> bool {
    // 1. "Any" Short-Circuit (The Black Hole)
    // 'any' violates the partial order of sets. We handle it here
    // so the core logic remains transitive.
    if sub.is_any(db) || sup.is_any(db) {
        return true;
    }

    // 2. Configure the Judge
    // We derive the strictness rules from the compiler options and context.
    let config = SolverConfig {
        // Example: Methods in TS are bivariant, but standalone functions can be strict.
        function_variance: if db.is_strict_function_types() {
            Variance::Contravariant // Sound
        } else {
            Variance::Bivariant     // Unsound (Legacy)
        },
        excess_property_check: sub.is_fresh(),
    };

    // 3. Delegate to Core Math
    solve_subtype_core(db, sub, sup, config)
}
```

### 8.2 Modeled Unsoundness

We do not use ad-hoc hacks. We map TypeScript quirks to explicit **Configuration States**.

#### A. Function Variance
The solver must accept a `Variance` parameter rather than hardcoding Contravariance.

*   **Covariant:** $A \\subseteq B$ (Output positions)
*   **Contravariant:** $B \\subseteq A$ (Input positions - Sound)
*   **Bivariant:** $(A \\subseteq B) \\lor (B \\subseteq A)$ (Input positions - Unsound/Legacy)

#### B. Freshness (Excess Property Checking)
"Freshness" is a transient state of Object Literals. We encode this as a `TypeFlag` on the `TypeKey`.
*   **Rule:** If `sub.flags.contains(IS_FRESH)`, the Core Solver enables the **Exactness Check** (rejects width subtyping).

#### C. The Void Exception
TypeScript allows `() => void` to match `() => string` (ignoring the return value).
*   **Rule:** The Core Solver adds a special case: If `sup.return_type` is `Void` and context allows "ignored return," treat `sub.return_type` as compatible regardless of type.


---

## 7. Conclusion & Migration Path

This design moves the TypeScript Compiler from an **Imperative State Machine** to a **Functional Database**.

*   **Correctness:** Guaranteed by Set Theory (Section 1) and Coinduction (Section 3).
*   **Performance:** Guaranteed by Interning (Section 2) and Salsa Incrementalism (Section 6).
*   **Completeness:** Advanced features (Conditional Types, Inference) are handled via Unification and Logic Lowering (Sections 4 & 5).


You have the full Design Document. Now, to make this actionable, here is the **Implementation Roadmap** to execute Phase 7.5.

You can add this to your `migration_plan.md` or use it as your task tracker.

---

# 📅 Phase 7.5 Execution Plan: The Semantic Solver

**Objective:** Build the `salsa`-backed structural solver in `wasm/src/solver/`.

## Step 1: Infrastructure & Dependencies
**Goal:** Set up the cargo workspace for the new engine.

*   [ ] **Add Dependencies:**
    *   `salsa = "0.17"` (The Query Database)
    *   `ena = "0.14"` (Union-Find / Unification)
    *   `indexmap` (Deterministic iteration for structural keys)
    *   `bitflags` (Efficient TypeFlags)
*   [ ] **Create Module Structure:**
    ```text
    wasm/src/solver/
    ├── mod.rs          // Public API
    ├── db.rs           // Salsa Database definition
    ├── jar.rs          // The TypeJar struct
    ├── type_id.rs      // The TypeId wrapper (Interned)
    ├── type_key.rs     // The TypeKey enum (Data)
    ├── lower.rs        // ThinNode -> TypeKey (Synthesis)
    ├── logic.rs        // solve_subtype (The Algorithm)
    └── infer.rs        // Unification table (Inference)
    ```

## Step 2: The Data Universe (Interning)
**Goal:** Define `TypeId` and `TypeKey` so we can represent types as integers.

*   [ ] **Define `TypeKey` Enum:**
    *   Must derive `Eq`, `Hash`, `Clone`.
    *   Include `Intrinsic`, `Literal`, `Object`, `Union`, `Intersection`, `Ref`.
*   [ ] **Implement Normalization:**
    *   Create `TypeKey::object(vec)` constructor that sorts properties by Atom.
    *   Create `TypeKey::union(vec)` constructor that flattens nested unions and sorts by ID.
*   [ ] **Setup Salsa Interner:**
    *   Define `#[salsa::interned]` struct `Type` wrapping `TypeKey`.

## Step 3: The Bridge (Lowering)
**Goal:** Convert `ThinNode` AST into `Type`s.

*   [ ] **Implement `lower_type` Query:**
    *   Signature: `fn lower_type(db, node: NodeIndex) -> Type`.
    *   Match on `ThinNode.kind`.
    *   **Case Primitive:** Map `SyntaxKind::StringKeyword` -> `Intrinsic::String`.
    *   **Case Literal:** Extract text from scanner -> `Literal::String`.
    *   **Case Interface:**
        *   Iterate members.
        *   Recursively call `lower_type` (Salsa handles cycles!).
        *   Return `TypeKey::Object`.

## Step 4: The Logic Engine (Subtyping)
**Goal:** Implement `solve_subtype` with cycle handling.

*   [ ] **Implement `solve_subtype` Query:**
    *   Signature: `fn solve_subtype(db, sub: Type, sup: Type) -> bool`.
*   [ ] **Handle Primitives:** `String <: String`, `Never <: T`, `T <: Unknown`.
*   [ ] **Handle Structured Objects:**
    *   Iterate `sup` properties.
    *   Binary search in `sub` properties.
    *   Recurse: `solve_subtype(sub_prop, sup_prop)`.
*   [ ] **Handle Unions:**
    *   `Sub` is Union: ALL parts must match `Sup`.
    *   `Sup` is Union: `Sub` must match ANY part (simplistic).
*   [ ] **Verify Cycle Detection:**
    *   Write a test case: `interface A { x: A }; interface B { x: B }; A <: B?`
    *   Ensure Salsa returns `true` (via cycle recovery strategy) instead of stack overflow.

## Step 5: Constraint Solving (Inference)
**Goal:** Implement `check_call_expression` with generics.

*   [ ] **Define `InferenceContext`:**
    *   Wrapper around `ena::InPlaceUnificationTable`.
*   [ ] **Implement `instantiate`:**
    *   Replace `TypeKey::Generic(T)` with `InferenceVar(?0)`.
*   [ ] **Implement `unify(a, b)`:**
    *   If `a` is var, point to `b`.
    *   If both concrete, call `solve_subtype`.
*   [ ] **Implement `check_call`:**
    *   Create context.
    *   Instantiate signature.
    *   Unify arguments.
    *   Snapshot/Rollback for overloads.
    *   Return result type.

## Step 6: Integration Tests
**Goal:** Verify against `tests/cases/compiler`.

*   [ ] **Unit Tests:** `solver/tests.rs` for basic mechanics.
*   [ ] **Integration:** Hook up `check_file` to the new solver.
*   [ ] **Benchmark:** Compare memory usage vs. Legacy Checker.

---

### Recommended First Code Block
Start with **Step 1 & 2** to establish the data structures.

```rust
// wasm/src/solver/type_key.rs

use crate::parser::thin_node::NodeIndex;
use crate::interner::Atom;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeKey {
    /// The 'any', 'string', 'number' keywords
    Intrinsic(IntrinsicKind),
    
    /// Literal values: "foo", 42
    Literal(LiteralValue),

    /// Structural object: { x: number, y: string }
    /// INVARIANT: Must be sorted by Atom (name)
    Object(Vec<(Atom, TypeId)>),

    /// A | B
    /// INVARIANT: Flattened and sorted by ID
    Union(Vec<TypeId>),

    /// Recursive reference to a declaration
    Ref(NodeIndex),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicKind {
    Any, Unknown, String, Number, Boolean, Never, Void, Undefined
}

// Placeholder for the ID (will be defined by Salsa macro later)
pub type TypeId = u32; 
```


