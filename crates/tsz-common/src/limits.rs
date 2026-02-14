//! Cross-crate limits and thresholds for the TypeScript compiler.
//!
//! # What belongs here
//!
//! A constant belongs in this file **only** if it is imported by more than one
//! crate. Single-crate constants should live next to the code that uses them.
//!
//! # What does NOT belong here
//!
//! - **Solver recursion limits** → [`tsz_solver::recursion::RecursionProfile`]
//! - **Checker recursion limits** → `RecursionProfile` or `DepthCounter::with_profile`
//! - **Data structure tuning** (e.g., `SmallVec` inline capacity) → the crate's own module
//! - **Single-crate iteration limits** → the file that uses them
//!
//! # History
//!
//! This file previously contained ~40 constants attempting to centralize every
//! limit in the codebase. In practice most were duplicated locally and the
//! `limits.rs` copies were never imported — changes here had no effect.
//! It was trimmed to only the constants that are genuinely cross-crate.

// =============================================================================
// Type System Limits
// =============================================================================

/// Maximum depth for generic type instantiation.
///
/// Prevents infinite recursion in recursive generic types. When exceeded,
/// the compiler emits **TS2589**:
/// *"Type instantiation is excessively deep and possibly infinite."*
///
/// Used by `tsz-checker` (function_type.rs, state_type_environment.rs)
/// and `tsz-solver` (instantiate.rs).
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

/// Maximum depth for function call resolution.
///
/// Prevents infinite recursion when resolving overloaded or recursive call
/// chains. Used by `tsz-checker` (type_computation_complex.rs).
///
/// # TypeScript example
///
/// ```typescript
/// // Many overloads where the checker tries each candidate recursively:
/// declare function overloaded(x: string): number;
/// declare function overloaded(x: number): string;
/// declare function overloaded(x: boolean): boolean;
/// const r = overloaded(overloaded(overloaded(/* deeply nested calls */)));
///
/// // Recursive function calls where return-type inference chains:
/// function recurse<T>(x: T): T { return recurse(x); }
/// ```
pub const MAX_CALL_DEPTH: u32 = 20;

/// Maximum depth for subtype checking.
///
/// Prevents infinite recursion in recursive structural type comparisons.
/// Used by `tsz-solver` (SubtypeChecker.max_depth, evaluate.rs union
/// simplification).
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

/// Maximum iterations for tree-walking algorithms.
///
/// A general-purpose safety valve for loops that walk scope chains, parent
/// nodes, or other tree structures. Used across many `tsz-checker` modules.
///
/// # TypeScript example
///
/// ```typescript
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

/// Maximum subtype checking pairs tracked simultaneously (cycle detection).
///
/// When checking if type A is a subtype of type B, the checker records the
/// (A, B) pair to detect cycles (A <: B <: C <: A). This limits how many
/// such pairs can be in-flight at once to prevent memory exhaustion. Used
/// by `tsz-solver` (subtype.rs).
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

// =============================================================================
// Parser Limits
// =============================================================================

/// Maximum depth for parser recursion.
///
/// Prevents stack overflow when parsing deeply nested source code. The parser
/// tracks its recursion depth and emits a diagnostic when exceeded. Used by
/// `tsz-parser` (parser/state.rs).
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

// =============================================================================
// Type Resolution Limits (WASM-aware)
// =============================================================================

/// Maximum type resolution operations (fuel counter).
///
/// Each resolution operation decrements a counter; at zero the checker stops.
/// Prevents unbounded CPU time on a single file. Used by `tsz-checker`
/// (context.rs).
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

#[cfg(test)]
#[path = "../tests/limits.rs"]
mod tests;
