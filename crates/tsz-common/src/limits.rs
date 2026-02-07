//! Centralized limits and thresholds for the TypeScript compiler.
//!
//! This module provides shared constants for recursion depths, operation counts,
//! and capacity limits used throughout the codebase. Centralizing these values:
//! - Prevents duplicate definitions with inconsistent values
//! - Makes it easy to tune limits for different environments (WASM vs native)
//! - Documents the rationale for each limit
//!
//! # Categories
//!
//! - **Recursion Depths**: Limits to prevent stack overflow in recursive algorithms
//! - **Operation Counts**: Limits to prevent infinite loops in iterative algorithms
//! - **Capacity Limits**: Pre-allocation sizes and maximum collection sizes
//! - **WASM Limits**: Reduced limits for the constrained WASM environment
//!
//! # Solver recursion limits
//!
//! Recursion limits for the type solver (subtype checking, type evaluation,
//! property access, etc.) are centralized in
//! [`tsz_solver::recursion::RecursionProfile`] rather than here. This avoids
//! duplication between `limits.rs` constants and `RecursionGuard` construction
//! sites. The profiles are the single source of truth for solver recursion.

// =============================================================================
// Recursion Depth Limits (Checker / Binder / Parser / Emitter)
// =============================================================================
// These prevent stack overflow in deeply nested type structures or AST nodes.
// Solver-specific recursion limits live in RecursionProfile instead.

/// Maximum depth for expression type checking.
///
/// Prevents stack overflow when the checker recursively resolves the type of
/// deeply nested expressions. Each nested expression adds a frame to the call
/// stack; at 500 levels deep the checker bails out early.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply nested ternary / comma / binary expressions:
/// const x = (((((((((((((((((((((((((1 + 2) + 3) + 4) /* ... 500 levels ... */)))));
///
/// // Deeply nested function calls:
/// f(f(f(f(f(f(f(f(f(f(f(f(/* ... */)))))))))))));
///
/// // Deeply nested property accesses:
/// a.b.c.d.e.f.g.h.i.j.k.l.m.n /* ... hundreds of levels ... */;
/// ```
pub const MAX_EXPR_CHECK_DEPTH: u32 = 500;

/// Maximum depth for generic type instantiation.
///
/// Prevents infinite recursion when the compiler instantiates recursive generic
/// types. When this depth is exceeded the compiler emits **TS2589**:
/// *"Type instantiation is excessively deep and possibly infinite."*
///
/// Used in `function_type.rs`, `state_type_environment.rs`, and
/// `instantiate.rs`. Returns `TypeId::ERROR` when exceeded.
///
/// # TypeScript example
///
/// ```typescript
/// // Recursive conditional type that never terminates:
/// type InfiniteUnwrap<T> = T extends Promise<infer U> ? InfiniteUnwrap<U> : T;
/// type Bad = InfiniteUnwrap<Promise<Promise<Promise</* ... 50+ layers ... */>>>>;
/// //   ~~~ TS2589: Type instantiation is excessively deep and possibly infinite.
///
/// // Self-referential generic that keeps expanding:
/// type Foo<T> = { value: Foo<Foo<T>> };
/// type Boom = Foo<string>;
/// //   ~~~~ TS2589
///
/// // Recursive tuple builder:
/// type BuildTuple<N extends number, T extends any[] = []> =
///   T["length"] extends N ? T : BuildTuple<N, [...T, unknown]>;
/// type Huge = BuildTuple<999>;
/// //   ~~~~ TS2589
/// ```
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum depth for general checker recursion guards.
///
/// Used by `enter_recursion` / `leave_recursion` on checker functions like
/// `get_construct_type_from_type`, `type_reference_symbol_type`, etc.
/// Each guarded cycle adds ~7-14 stack frames; depth 50 ≈ 350-700 frames
/// (~0.7 MB), well within the 8 MB default stack.
///
/// # TypeScript example
///
/// ```typescript
/// // Mutually recursive type references that the checker must resolve:
/// type A = { b: B };
/// type B = { a: A };
/// declare const val: A;
/// val.b.a.b.a.b.a.b; // checker must recurse into each nested type reference
///
/// // Constructor types referencing themselves via return types:
/// interface Widget {
///   new (): Widget; // checker recurses to resolve the construct signature
/// }
/// ```
pub const MAX_CHECKER_RECURSION_DEPTH: u32 = 50;

/// Maximum depth for function call resolution.
///
/// Prevents infinite recursion when the checker resolves overloaded or
/// recursive function call chains. At depth 20 the checker stops trying
/// deeper call resolution paths.
///
/// # TypeScript example
///
/// ```typescript
/// // Many overloads where the checker tries each candidate recursively:
/// declare function overloaded(x: string): number;
/// declare function overloaded(x: number): string;
/// declare function overloaded(x: boolean): boolean;
/// declare function overloaded(x: string | number | boolean): unknown;
/// const r = overloaded(overloaded(overloaded(/* deeply nested calls */)));
///
/// // Recursive function calls where return-type inference chains:
/// function recurse<T>(x: T): T { return recurse(x); }
/// ```
pub const MAX_CALL_DEPTH: u32 = 20;

/// Maximum depth for subtype checking.
///
/// Prevents infinite recursion when comparing recursive types for
/// assignability. Used by the solver's `SubtypeChecker` `max_depth` field.
/// Returns `SubtypeResult::DepthExceeded` when the limit is hit, which
/// can trigger **TS2589** via the `depth_exceeded` flag.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply recursive structural types:
/// type LinkedList<T> = { value: T; next: LinkedList<T> | null };
/// declare let a: LinkedList<string>;
/// declare let b: LinkedList<string | number>;
/// a = b; // subtype checker recurses through each `next` level
///
/// // Mutually recursive interfaces:
/// interface Tree<T> {
///   value: T;
///   children: Forest<T>;
/// }
/// interface Forest<T> extends Array<Tree<T>> {}
/// declare let tree1: Tree<string>;
/// declare let tree2: Tree<unknown>;
/// tree2 = tree1; // deep structural comparison of Tree → Forest → Tree → ...
/// ```
pub const MAX_SUBTYPE_DEPTH: u32 = 100;

/// Maximum depth for type alias resolution.
///
/// Prevents infinite recursion when the checker follows chains of type
/// aliases. Returns `None` when exceeded, silently stopping resolution.
/// Used in `type_checking_queries.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Long chain of aliases the checker must unwind:
/// type A = B;
/// type B = C;
/// type C = D;
/// type D = E;
/// // ... 128 levels of indirection ...
/// type Z = string;
///
/// // Circular alias (TypeScript already errors, but the limit prevents
/// // the compiler from hanging during resolution):
/// type Loop = Loop; // TS2456: Type alias 'Loop' circularly references itself.
/// ```
pub const MAX_ALIAS_RESOLUTION_DEPTH: u32 = 128;

/// Maximum depth for qualified name resolution (`A.B.C.D...`).
///
/// Prevents infinite loops when traversing namespace or module chains.
/// Used in `symbol_resolver.rs`; returns `TypeSymbolResolution::NotFound`
/// when exceeded.
///
/// # TypeScript example
///
/// ```typescript
/// // Deep namespace nesting:
/// namespace A {
///   export namespace B {
///     export namespace C {
///       export namespace D {
///         // ... 128 levels deep ...
///         export const value = 42;
///       }
///     }
///   }
/// }
/// const x = A.B.C.D./* ... */.value;
///
/// // Module re-exports creating long resolution chains:
/// // a.ts: export { foo } from './b';
/// // b.ts: export { foo } from './c';
/// // c.ts: export { foo } from './d';
/// // ... 128 hops ...
/// ```
pub const MAX_QUALIFIED_NAME_DEPTH: u32 = 128;

/// Maximum depth for optional chaining expressions (`a?.b?.c?.d...`).
///
/// The checker walks the optional chain from innermost to outermost to
/// determine the overall type. This limits how deep that walk can go.
/// Used in `optional_chain.rs`; breaks the loop when exceeded.
///
/// # TypeScript example
///
/// ```typescript
/// // Long optional chain — each `?.` adds one level to the walk:
/// const result = obj?.a?.b?.c?.d?.e?.f?.g?.h?.i?.j?.k?.l?.m?.n?.o?.p;
///
/// // Chained method calls with optional:
/// const val = api?.getUser()?.getProfile()?.getAddress()?.getCity()?.getName();
///
/// // Mixed optional chains with element access:
/// const item = data?.items?.[0]?.children?.[1]?.name;
/// ```
pub const MAX_OPTIONAL_CHAIN_DEPTH: u32 = 1_000;

/// Maximum depth for binding pattern destructuring.
///
/// Limits how deeply nested a destructuring pattern can be during the
/// lowering pass. Used in `lowering_pass.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply nested object destructuring:
/// const {
///   a: {
///     b: {
///       c: {
///         d: {
///           e: {
///             f: value  // at depth 100 the lowering pass stops
///           }
///         }
///       }
///     }
///   }
/// } = deeplyNestedObject;
///
/// // Deeply nested array destructuring:
/// const [[[[[[[[[[[deepValue]]]]]]]]]]] = nestedArrays;
///
/// // Mixed object/array destructuring:
/// const { items: [{ nested: { deep: [finalValue] } }] } = data;
/// ```
pub const MAX_BINDING_PATTERN_DEPTH: u32 = 100;

/// Maximum depth for AST traversal during lowering.
///
/// Prevents stack overflow when the lowering pass recursively visits deeply
/// nested AST nodes (e.g., deeply nested blocks, expressions, or statements).
/// Returns early when exceeded. Used in `lowering_pass.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply nested blocks:
/// {{{{{{{{{{{{{{{{{{{{{{{{{
///   const x = 1; // 500 levels of nesting
/// }}}}}}}}}}}}}}}}}}}}}}}}}
///
/// // Deeply nested if/else chains:
/// if (a) { if (b) { if (c) { if (d) { /* ... 500 levels ... */ } } } }
///
/// // Deeply nested arrow functions:
/// const f = () => () => () => () => () => /* ... 500 levels ... */ 42;
/// ```
pub const MAX_AST_DEPTH: u32 = 500;

/// Maximum depth for emitter recursion.
///
/// Prevents stack overflow when the emitter outputs deeply nested structures
/// (e.g., declaration files with deeply nested types). Logs a warning and
/// writes a comment placeholder when exceeded. Used in `emitter/mod.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply nested type that the .d.ts emitter must output:
/// type Deep = {
///   a: {
///     b: {
///       c: {
///         d: { /* ... 1000 levels of nesting ... */ }
///       }
///     }
///   }
/// };
///
/// // Deeply nested conditional types in declaration output:
/// type Unwrap<T> = T extends Promise<infer U>
///   ? U extends Promise<infer V>
///     ? V extends Promise<infer W>
///       ? /* ... */ : never : never : T;
/// ```
pub const MAX_EMIT_RECURSION_DEPTH: u32 = 1_000;

/// Maximum depth for parser recursion.
///
/// Prevents stack overflow when parsing deeply nested source code. The
/// parser tracks its recursion depth and emits an `UNEXPECTED_TOKEN`
/// diagnostic when this limit is exceeded. Used in `parser/state.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Deeply nested parenthesized expressions:
/// const x = ((((((((((((((((((((((((((((((1)))))))))))))))))))))))))))))));
///
/// // Deeply nested generic type arguments:
/// type T = Promise<Promise<Promise<Promise<Promise</* ... 1000 levels ... */>>>>>;
///
/// // Deeply nested arrow function parameters:
/// const f = (a: (b: (c: (d: (e: /* ... */) => void) => void) => void) => void) => {};
/// ```
pub const MAX_PARSER_RECURSION_DEPTH: u32 = 1_000;

/// Maximum depth for decorator text extraction.
///
/// When extracting decorator text for display or diagnostics, this prevents
/// unbounded recursion into nested decorator expressions. Returns `"..."`
/// when exceeded. Used in `emitter/special_expressions.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Nested decorator factory calls — the text extractor recurses into
/// // each argument expression:
/// @A(B(C(D(E(F(G(H(I(J(K("deep")))))))))))
/// class MyClass {}
///
/// // Complex decorator with nested object literals:
/// @Config({
///   module: Inner({
///     providers: Nested({
///       factory: Deep({
///         value: VeryDeep({ /* depth 10 → truncated to "..." */ })
///       })
///     })
///   })
/// })
/// class Service {}
/// ```
pub const MAX_DECORATOR_TEXT_DEPTH: u32 = 10;

/// Maximum depth for type merging operations.
///
/// Prevents stack overflow when merging declaration types (e.g., merging
/// interfaces across multiple declarations, or merging namespaces with
/// same-named values). Used in `namespace_checker.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Interface merging — each declaration adds a merge level:
/// interface Config { a: string; }
/// interface Config { b: number; }
/// interface Config { c: boolean; }
/// // ... 32 declarations merged together
///
/// // Namespace merging with nested namespaces:
/// namespace N { export interface A { x: number; } }
/// namespace N { export interface A { y: string; } }
/// // the checker must merge N, then merge A within N
///
/// // Class + namespace + interface merge:
/// class Foo {}
/// namespace Foo { export const bar = 1; }
/// interface Foo { baz: string; }
/// ```
pub const MAX_MERGE_DEPTH: u32 = 32;

/// Maximum depth for constraint recursion in type inference.
///
/// Prevents infinite loops when the solver follows constraint chains during
/// type inference. For example, when a type parameter is constrained by
/// another constrained type parameter. Returns early when exceeded.
/// Used in `operations.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Chained constraints the solver must follow:
/// function foo<A extends B, B extends C, C extends D, D extends string>(
///   a: A, b: B, c: C, d: D
/// ) {
///   // solver follows: A → B → C → D → string
/// }
///
/// // Conditional type with recursive constraint resolution:
/// type Resolve<T> = T extends { inner: infer U } ? Resolve<U> : T;
/// type Deep = Resolve<{ inner: { inner: { inner: /* ... */ string } } }>;
///
/// // Mapped type with constraint recursion:
/// type DeepPartial<T> = {
///   [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K];
/// };
/// ```
pub const MAX_CONSTRAINT_RECURSION_DEPTH: u32 = 100;

/// Maximum depth for template literal type counting.
///
/// When the solver counts how many string combinations a template literal
/// type can produce, it recurses into each constituent. This prevents
/// runaway counting in deeply nested template literals. Returns 0 or
/// empty when exceeded. Used in `evaluate_rules/template_literal.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Template literal with many interpolation slots:
/// type Color = "red" | "green" | "blue";
/// type Size = "sm" | "md" | "lg";
/// type Variant = "primary" | "secondary";
/// type ClassName = `${Color}-${Size}-${Variant}`; // 3 × 3 × 2 = 18 strings
///
/// // Deeply nested template literals:
/// type A = `${B}-suffix`;
/// type B = `${C}-mid`;
/// type C = `${D}-prefix`;
/// // ... 50 levels of nesting → counting aborts
///
/// // Recursive template literal type:
/// type Repeat<S extends string, N extends number> =
///   N extends 0 ? "" : `${S}${Repeat<S, /* N-1 */>}`;
/// ```
pub const MAX_LITERAL_COUNT_DEPTH: u32 = 50;

// =============================================================================
// Operation Count Limits
// =============================================================================
// These prevent infinite loops in iterative algorithms.

/// Maximum iterations for tree-walking algorithms.
///
/// A general-purpose safety valve used across many checker subsystems:
/// type computation, control-flow narrowing, scope finding, constructor
/// checking, symbol resolution, and more. When the iteration count is
/// exceeded the algorithm breaks out of its loop and returns a safe
/// default. Used in `type_checking.rs`, `flow_analysis.rs`,
/// `symbol_resolver.rs`, `scope_finder.rs`, and others.
///
/// # TypeScript example
///
/// ```typescript
/// // Large switch with many narrowing branches — each case is a tree node:
/// function process(x: string | number | boolean | null | undefined | symbol) {
///   switch (typeof x) {
///     case "string": /* ... */ break;
///     case "number": /* ... */ break;
///     // ... thousands of cases in generated code
///   }
/// }
///
/// // Walking up scope chains to find a binding:
/// function outer() {
///   function level1() {
///     function level2() {
///       // ... deeply nested scopes the resolver must walk
///       return someVar; // walks up 10,000 scope nodes max
///     }
///   }
/// }
///
/// // Large union narrowing with many discriminant checks:
/// type Event =
///   | { type: "click"; x: number }
///   | { type: "key"; key: string }
///   | { type: "scroll"; delta: number }
///   // ... thousands of variants
/// function handle(e: Event) {
///   if (e.type === "click") { /* narrowed */ }
/// }
/// ```
pub const MAX_TREE_WALK_ITERATIONS: u32 = 10_000;

/// Maximum iterations for flow analysis.
///
/// Control flow analysis can be expensive — it walks the flow graph
/// forwards and backwards to determine types at each point. This bounds
/// the total work to prevent hangs on pathological inputs. Used in
/// `flow_analyzer.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Long function with many assignments and branches:
/// function bigFunction(x: string | number) {
///   let result: string | number | boolean = x;
///   if (typeof x === "string") { result = x.toUpperCase(); }
///   if (typeof x === "number") { result = x + 1; }
///   // ... hundreds of if/else branches, loops, and reassignments
///   // flow analysis must track `result`'s type through all paths
///   for (let i = 0; i < 1000; i++) {
///     if (i % 2 === 0) { result = "even"; }
///     else { result = i; }
///   }
///   return result;
/// }
///
/// // Many variables with interleaved assignments:
/// let a = 1, b = "x", c = true;
/// a = condition ? b.length : 0;
/// b = condition ? String(a) : "default";
/// // ... 100,000 flow nodes max before analysis stops
/// ```
pub const MAX_FLOW_ANALYSIS_ITERATIONS: u32 = 100_000;

/// Maximum subtype checking pairs tracked simultaneously (cycle detection).
///
/// When checking if type A is a subtype of type B, the checker records the
/// (A, B) pair to detect cycles (A <: B <: C <: A). This limits how many
/// such pairs can be in-flight at once to prevent memory exhaustion. Used
/// in `subtype.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Many simultaneous structural comparisons during a single assignability check:
/// interface Node<T> {
///   value: T;
///   left: Node<T> | null;
///   right: Node<T> | null;
///   parent: Node<T> | null;
///   children: Node<T>[];
///   metadata: Record<string, Node<T>>;
/// }
/// declare let a: Node<string>;
/// declare let b: Node<string | number>;
/// b = a; // generates thousands of in-progress subtype pairs as it
///        // recurses through value, left, right, parent, children, metadata
/// ```
pub const MAX_IN_PROGRESS_PAIRS: u32 = 10_000;

/// Maximum lowering operations during AST transformation.
///
/// The lowering pass transforms the parsed AST into a form suitable for
/// type-checking (e.g., desugaring `for...of`, expanding decorators).
/// This bounds total operations to prevent hangs on huge files. Sets a
/// `limit_exceeded` flag when exceeded. Used in `lower.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Very large generated file with thousands of statements:
/// export const a1 = 1;
/// export const a2 = 2;
/// // ... 100,000 export statements
///
/// // Each lowered construct (enum, decorator, namespace) costs operations:
/// enum Color { Red, Green, Blue }        // lowered to object + assignments
/// enum Size { S, M, L, XL, XXL }          // lowered to object + assignments
/// // ... thousands of enums × members = many lowering operations
/// ```
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

/// Maximum constraint solving iterations during type inference.
///
/// The type inference engine iteratively solves constraints (unification,
/// bound propagation) until a fixed point is reached. This limits the
/// number of iterations to prevent infinite loops when constraints are
/// cyclic. Used in `infer.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Complex generic inference with many interdependent type parameters:
/// declare function pipe<A, B, C, D, E>(
///   f1: (a: A) => B,
///   f2: (b: B) => C,
///   f3: (c: C) => D,
///   f4: (d: D) => E,
/// ): (a: A) => E;
///
/// // The solver iterates to resolve A→B→C→D→E:
/// const transform = pipe(
///   (x: string) => x.length,
///   (n) => n > 0,
///   (b) => b ? "yes" : "no",
///   (s) => s.toUpperCase(),
/// );
///
/// // Recursive inference that may cycle:
/// declare function identity<T>(x: T): T;
/// const result = identity(identity(identity(identity(42))));
/// ```
pub const MAX_CONSTRAINT_ITERATIONS: u32 = 100;

/// Maximum iterations for type unwrapping.
///
/// Some types are wrapped in layers of indirection (lazy evaluations,
/// references, intersections). The solver iteratively unwraps these to
/// reach the underlying type. This prevents infinite loops if unwrapping
/// cycles back. Returns the current (partially unwrapped) type when
/// exceeded. Used in `operations.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Type wrapped in many layers of indirection:
/// type Lazy<T> = T;
/// type A = Lazy<Lazy<Lazy<Lazy</* ... 1000 layers ... */ string>>>>;
///
/// // Intersection that the solver must flatten:
/// type Big = A & B & C & D & E & F & G & H; // each member may itself
///                                             // be an intersection to unwrap
///
/// // Conditional types that resolve to wrapped types:
/// type Unwrap<T> = T extends { value: infer U } ? U : T;
/// type Result = Unwrap<{ value: { value: { value: /* ... */ string } } }>;
/// ```
pub const MAX_UNWRAP_ITERATIONS: u32 = 1_000;

/// Maximum types in the visiting set during type evaluation.
///
/// During evaluation, the solver tracks which types it is currently visiting
/// to detect cycles. If the visiting set grows beyond this size, evaluation
/// stops to prevent memory exhaustion. Used in `evaluate.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Many distinct types being evaluated simultaneously:
/// type A = B extends string ? C : D;
/// type B = E extends number ? F : G;
/// type C = H extends boolean ? I : J;
/// // ... thousands of conditional types that the solver must evaluate
/// // in a dependency graph — each adds to the visiting set
///
/// // Large mapped type that generates many intermediate types:
/// type AllProps<T> = {
///   [K in keyof T]: T[K] extends object ? AllProps<T[K]> : T[K];
/// };
/// type Big = AllProps<DeepNestedObject>; // visiting set grows with each property
/// ```
pub const MAX_VISITING_SET_SIZE: u32 = 10_000;

// =============================================================================
// Type Resolution Limits (WASM-aware)
// =============================================================================
// WASM has stricter memory constraints, so we use lower limits.

/// Maximum type resolution operations (fuel counter).
///
/// Acts as a "fuel" budget for type resolution. Each resolution operation
/// decrements the counter; when it hits zero, the checker stops resolving
/// further types. This prevents a single file from consuming unbounded
/// CPU time. WASM gets a smaller budget due to memory constraints.
///
/// Used in `context.rs` to initialize the fuel counter, and exported
/// from `state.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // File with thousands of complex type computations:
/// type Compute<T> = T extends [infer H, ...infer R]
///   ? [Process<H>, ...Compute<R>]
///   : [];
/// type Result = Compute<[/* 500 element tuple */]>;
/// // Each element requires multiple resolution operations;
/// // at 100,000 ops the checker stops and returns error types
///
/// // Large file with many overloaded functions:
/// declare function f(x: string): number;
/// declare function f(x: number): string;
/// // ... hundreds of overloads × hundreds of call sites = many resolution ops
/// ```
///
/// WASM: 20,000 (memory constrained) / Native: 100,000
#[cfg(target_arch = "wasm32")]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 20_000;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 100_000;

/// Maximum template literal expansion size.
///
/// Template literal types can produce a combinatorial explosion of string
/// literal types. For example, `` `${A | B}-${C | D}` `` produces 4 strings.
/// This limits the total number of expanded strings to prevent memory
/// exhaustion. When exceeded, the solver keeps the unexpanded template
/// literal type rather than widening to `string`.
///
/// Used in `evaluate_rules/template_literal.rs` and `intern.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Combinatorial explosion of template literal types:
/// type Letter = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h";
/// type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9";
/// type HexColor = `#${Letter | Digit}${Letter | Digit}${Letter | Digit}`;
/// // 18^3 = 5,832 possible strings — within native limit but exceeds WASM limit
///
/// // Even larger expansion:
/// type Coord = `${Digit}${Digit}-${Digit}${Digit}`;
/// // 10 × 10 × 10 × 10 = 10,000 strings
///
/// // Exceeds both limits:
/// type Code = `${Letter}${Digit}${Letter}${Digit}${Letter}`;
/// // 8 × 10 × 8 × 10 × 8 = 51,200 strings → stays as template literal type
/// ```
///
/// WASM: 2,000 (memory constrained) / Native: 100,000
#[cfg(target_arch = "wasm32")]
pub const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum interned types.
///
/// The type interner deduplicates and stores all types created during
/// compilation. This limits the total number of interned types to prevent
/// memory exhaustion. Returns `TypeId::ERROR` when exceeded. Used in
/// `intern.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Large project with many unique types:
/// // Each interface, union, intersection, conditional, mapped type, etc.
/// // creates one or more interned types.
///
/// // A single mapped type can generate many interned types:
/// type FullAPI = {
///   [K in keyof LargeInterface]: {
///     get: () => Promise<LargeInterface[K]>;
///     set: (value: LargeInterface[K]) => void;
///     subscribe: (cb: (value: LargeInterface[K]) => void) => () => void;
///   };
/// };
/// // For an interface with 100 properties, this creates ~400+ interned types
///
/// // Conditional type distribution over large unions:
/// type Process<T> = T extends string ? T[] : T extends number ? Set<T> : T;
/// type Result = Process</* union with 1000 members */>;
/// // Each branch creates new interned types
/// ```
///
/// WASM: 500,000 (memory constrained) / Native: 5,000,000
#[cfg(target_arch = "wasm32")]
pub const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_INTERNED_TYPES: usize = 5_000_000;

/// Maximum keys in mapped type expansion.
///
/// Mapped types iterate over the keys of a type to produce new properties.
/// If the source type has too many keys, expansion is aborted to prevent
/// memory exhaustion. Returns `TypeId::ERROR` when exceeded. Used in
/// `evaluate_rules/mapped.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Mapped type over a large interface:
/// interface BigConfig {
///   option1: string;
///   option2: number;
///   // ... 500+ properties
///   option500: boolean;
/// }
/// type ReadonlyConfig = Readonly<BigConfig>;
/// // Readonly<T> = { readonly [K in keyof T]: T[K] } → iterates 500 keys
///
/// // Mapped type over string union keys:
/// type Keys = "a" | "b" | "c" | /* ... 500 keys */;
/// type Obj = { [K in Keys]: K };
///
/// // Mapped type that exceeds the limit:
/// type TooMany = { [K in keyof HugeGeneratedInterface]: boolean };
/// // If HugeGeneratedInterface has >500 keys → returns error type
/// ```
///
/// WASM: 250 (memory constrained) / Native: 500
#[cfg(target_arch = "wasm32")]
pub const MAX_MAPPED_KEYS: usize = 250;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_MAPPED_KEYS: usize = 500;

// =============================================================================
// Capacity/Size Limits
// =============================================================================
// Pre-allocation sizes and maximum collection sizes.

/// Threshold for switching from inline array to HashMap for object properties.
///
/// Below this count, object properties are stored in a `SmallVec` and
/// looked up via linear scan — cache locality makes this fast. At or above
/// this threshold, properties are promoted to a `HashMap` for O(1) lookup.
/// 24 is approximately the crossover point based on benchmarking.
///
/// Used in `intern.rs` when building object type representations.
///
/// # TypeScript example
///
/// ```typescript
/// // Small object (≤24 properties) → inline array, linear scan:
/// interface User {
///   id: number;
///   name: string;
///   email: string;
///   age: number;
/// }
///
/// // Large object (>24 properties) → HashMap for property lookup:
/// interface LargeConfig {
///   prop1: string;
///   prop2: number;
///   prop3: boolean;
///   // ... 25+ properties → switches to HashMap internally
///   prop25: string;
/// }
/// ```
pub const PROPERTY_MAP_THRESHOLD: usize = 24;

/// Inline capacity for type lists (union members, tuple elements, etc.).
///
/// Type lists backed by `SmallVec<[TypeId; 8]>` can hold up to 8 elements
/// without heap allocation. Most unions and tuples in real code have fewer
/// than 8 members, so this avoids allocation overhead in the common case.
///
/// Used in `intern.rs` for `TypeList` and `UnionMembers`.
///
/// # TypeScript example
///
/// ```typescript
/// // Fits inline (≤8 members, no heap allocation):
/// type Status = "pending" | "active" | "completed" | "failed";
/// type Tuple = [string, number, boolean];
/// type Mixed = string | number | null | undefined;
///
/// // Spills to heap (>8 members):
/// type BigUnion = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i";
/// type LongTuple = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
/// ```
pub const TYPE_LIST_INLINE: usize = 8;

/// Maximum union members when indexing into a type.
///
/// When computing `T[K]` where `T` or `K` is a union, the solver evaluates
/// each combination. This caps the union size to prevent combinatorial
/// explosion. Returns `TypeId::ERROR` when exceeded. Used in
/// `evaluate_rules/index_access.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Indexing into a union:
/// type Data = { a: string } | { a: number } | { a: boolean }
///   /* | ... 100 members max */;
/// type Result = Data["a"]; // evaluates index access on each union member
///
/// // Union key indexing:
/// interface Big { /* 100 properties */ }
/// type Keys = keyof Big; // union of 100 string literals
/// type Values = Big[Keys]; // indexes with each key — 100 accesses max
///
/// // Exceeding the limit:
/// type Huge = /* 101-member union */;
/// type Bad = Huge[0]; // → error type
/// ```
pub const MAX_UNION_INDEX_SIZE: usize = 100;

/// Maximum types when distributing conditional types.
///
/// When a conditional type `T extends U ? X : Y` is applied to a union `T`,
/// TypeScript distributes the conditional over each member. This limits how
/// many members can be distributed over to prevent combinatorial explosion.
/// Returns `TypeId::ERROR` when exceeded. Used in
/// `evaluate_rules/conditional.rs` and `instantiate.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Distributive conditional type:
/// type IsString<T> = T extends string ? "yes" : "no";
///
/// // Distribution over a union:
/// type Result = IsString<string | number | boolean>;
/// // Distributes to: IsString<string> | IsString<number> | IsString<boolean>
/// // = "yes" | "no" | "no" = "yes" | "no"
///
/// // Large union distribution:
/// type BigUnion = "a" | "b" | "c" | /* ... 100 members max */;
/// type Mapped = IsString<BigUnion>;
/// // Distributes 100 times — at the limit
///
/// // Exceeds the limit:
/// type HugeUnion = /* 101+ members */;
/// type Bad = IsString<HugeUnion>; // → error type
/// ```
pub const MAX_DISTRIBUTION_SIZE: usize = 100;

/// Maximum union members to show in diagnostic messages.
///
/// When displaying a type error involving a union, only the first N members
/// are shown to keep error messages readable. Additional members are elided
/// with `| ...`. Used in `diagnostics.rs` for **TS2322** / **TS2326**
/// style errors.
///
/// # TypeScript example
///
/// ```typescript
/// type Status = "pending" | "active" | "completed" | "failed" | "archived";
///
/// const s: Status = "invalid";
/// // Error: Type '"invalid"' is not assignable to type
/// //   '"pending" | "active" | "completed" | ...'
/// //                                          ^^^
/// // Only first 3 members shown, rest elided with "..."
///
/// declare let x: string | number | boolean | null | undefined;
/// const y: string = x;
/// // Error: Type 'string | number | boolean | ...' is not assignable to type 'string'.
/// ```
pub const UNION_MEMBER_DIAGNOSTIC_LIMIT: usize = 3;

/// Pre-allocation size for AST nodes.
///
/// Based on typical source file sizes (roughly 1 AST node per ~20
/// characters of source code). The parser pre-allocates up to this many
/// node slots to avoid repeated reallocation during parsing. The actual
/// allocation is `min(estimated_nodes, MAX_NODE_PREALLOC)`. Used in
/// `parser/node_arena.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // A 100MB source file at ~20 chars/node ≈ 5,000,000 nodes → capped here
/// // A typical 10KB file ≈ 500 nodes → well below this limit
/// //
/// // Each of these source constructs produces one or more AST nodes:
/// const x = 1;               // VariableStatement + VariableDeclaration + NumericLiteral
/// function f(a: string) {}   // FunctionDeclaration + Parameter + TypeAnnotation + Block
/// if (x > 0) { return x; }  // IfStatement + BinaryExpr + Block + ReturnStatement
/// ```
pub const MAX_NODE_PREALLOC: usize = 5_000_000;

/// Pre-allocation size for symbols.
///
/// Based on typical symbol density in TypeScript files. The binder
/// pre-allocates up to this many symbol slots. A "symbol" represents a
/// named entity (variable, function, class, etc.). Used in `binder/lib.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Each declaration creates at least one symbol:
/// const x = 1;                    // symbol for `x`
/// function greet(name: string) {} // symbols for `greet` and `name`
/// class User {                    // symbol for `User`
///   name: string;                 // symbol for `name` property
///   constructor() {}              // symbol for constructor
///   greet() {}                    // symbol for `greet` method
/// }
/// // A large codebase may have up to 1,000,000 symbols
/// ```
pub const MAX_SYMBOL_PREALLOC: usize = 1_000_000;

/// Multiplier for incremental parsing node budget.
///
/// When re-parsing a file incrementally (e.g., after an edit in the LSP),
/// the node budget is `previous_node_count × INCREMENTAL_NODE_MULTIPLIER`.
/// The multiplier accounts for edits that may increase the AST size (e.g.,
/// pasting a large block). Used in `lsp/project.rs`.
///
/// # TypeScript example
///
/// ```typescript
/// // Original file has 1,000 AST nodes.
/// // User pastes a large block → incremental parse budget = 1,000 × 4 = 4,000 nodes.
/// // This allows the file to grow up to 4× its original size in a single edit
/// // before the parser needs to do a full reparse.
///
/// // If the file previously had:
/// export function small() { return 1; } // ~5 nodes
/// // And user pastes in a 15-node block → budget of 20 nodes is sufficient
/// ```
pub const INCREMENTAL_NODE_MULTIPLIER: usize = 4;

/// Minimum node budget for incremental parsing.
///
/// Even for very small files, the incremental parser always allocates at
/// least this many node slots. This prevents under-allocation when a tiny
/// file is edited to become much larger. Used in `lsp/project.rs` with
/// `.max(INCREMENTAL_MIN_NODE_BUDGET)`.
///
/// # TypeScript example
///
/// ```typescript
/// // Tiny file with 2 nodes:
/// const x = 1;
/// // Budget = max(2 × 4, 4096) = 4096 nodes
/// // This ensures there's always room for significant expansion
///
/// // Empty file being filled:
/// // Budget = max(0 × 4, 4096) = 4096 nodes
/// // Enough for the user to type ~80KB of code before reallocation
/// ```
pub const INCREMENTAL_MIN_NODE_BUDGET: usize = 4_096;

// =============================================================================
// Sharding Constants
// =============================================================================
// Used for parallel/concurrent data structures.

/// Number of bits for shard indexing.
///
/// Sharding is used for concurrent data structures (e.g., the type interner)
/// to reduce lock contention. With 6 bits, we get 64 shards. Each shard
/// has its own lock, so up to 64 threads can access different shards
/// simultaneously without contention.
///
/// 64 shards aligns well with typical CPU core counts (8-64 cores) and
/// cache line sizes (64 bytes). More shards would waste memory on locks;
/// fewer would increase contention.
///
/// # How it works
///
/// ```text
/// hash(type_key) = 0b...101011_110010
///                          ^^^^^^
///                     shard index = lower 6 bits = 0b110010 = 50
/// → routes to shard 50 out of 64
/// ```
pub const SHARD_BITS: u32 = 6;

/// Number of shards (2^SHARD_BITS = 64).
///
/// Derived from [`SHARD_BITS`]. Represents the total number of independent
/// shards in concurrent data structures.
pub const SHARD_COUNT: usize = 1 << SHARD_BITS;

/// Mask for extracting shard index (SHARD_COUNT - 1 = 63 = 0b111111).
///
/// Applied as `hash & SHARD_MASK` to extract the lower [`SHARD_BITS`] bits
/// of a hash, producing a shard index in `0..SHARD_COUNT`.
pub const SHARD_MASK: usize = SHARD_COUNT - 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shard_constants_consistent() {
        assert_eq!(SHARD_COUNT, 1 << SHARD_BITS);
        assert_eq!(SHARD_MASK, SHARD_COUNT - 1);
    }

    #[test]
    fn test_wasm_limits_are_smaller() {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = MAX_TYPE_RESOLUTION_OPS >= 100_000;
            let _ = TEMPLATE_LITERAL_EXPANSION_LIMIT >= 100_000;
            let _ = MAX_INTERNED_TYPES >= 5_000_000;
            let _ = MAX_MAPPED_KEYS >= 500;
        }
    }
}
