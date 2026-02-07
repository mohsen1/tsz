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
/// Prevents infinite recursion in recursive generic types like
/// `type Foo<T> = Foo<Foo<T>>`. When exceeded, the compiler emits **TS2589**:
/// *"Type instantiation is excessively deep and possibly infinite."*
///
/// Used by `tsz-checker` (function_type.rs, state_type_environment.rs)
/// and `tsz-solver` (instantiate.rs).
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum depth for function call resolution.
///
/// Prevents infinite recursion when resolving overloaded or recursive call
/// chains. Used by `tsz-checker` (type_computation_complex.rs).
pub const MAX_CALL_DEPTH: u32 = 20;

/// Maximum depth for subtype checking.
///
/// Prevents infinite recursion in recursive structural type comparisons.
/// Used by `tsz-solver` (SubtypeChecker.max_depth, evaluate.rs union
/// simplification).
pub const MAX_SUBTYPE_DEPTH: u32 = 100;

/// Maximum iterations for tree-walking algorithms.
///
/// A general-purpose safety valve for loops that walk scope chains, parent
/// nodes, or other tree structures. Used across many `tsz-checker` modules.
pub const MAX_TREE_WALK_ITERATIONS: u32 = 10_000;

/// Maximum subtype checking pairs tracked simultaneously (cycle detection).
///
/// Limits how many (source, target) pairs can be in-flight during a single
/// subtype check to prevent memory exhaustion. Used by `tsz-solver`
/// (subtype.rs).
pub const MAX_IN_PROGRESS_PAIRS: u32 = 10_000;

// =============================================================================
// Parser Limits
// =============================================================================

/// Maximum depth for parser recursion.
///
/// Prevents stack overflow when parsing deeply nested source code (e.g.,
/// deeply nested parenthesized expressions or generic type arguments).
/// Used by `tsz-parser` (parser/state.rs).
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
/// WASM: 20,000 (memory constrained) / Native: 100,000
#[cfg(target_arch = "wasm32")]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 20_000;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 100_000;

#[cfg(test)]
mod tests {
    #[test]
    fn test_limits_are_reasonable() {
        use super::*;
        assert!(MAX_INSTANTIATION_DEPTH >= 20 && MAX_INSTANTIATION_DEPTH <= 200);
        assert!(MAX_CALL_DEPTH >= 10 && MAX_CALL_DEPTH <= 100);
        assert!(MAX_SUBTYPE_DEPTH >= 50 && MAX_SUBTYPE_DEPTH <= 500);
        assert!(MAX_TREE_WALK_ITERATIONS >= 1_000 && MAX_TREE_WALK_ITERATIONS <= 100_000);
        assert!(MAX_IN_PROGRESS_PAIRS >= 1_000 && MAX_IN_PROGRESS_PAIRS <= 100_000);
        assert!(MAX_PARSER_RECURSION_DEPTH >= 100 && MAX_PARSER_RECURSION_DEPTH <= 10_000);
        assert!(MAX_TYPE_RESOLUTION_OPS >= 10_000);
    }
}
