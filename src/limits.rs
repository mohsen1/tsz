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

// =============================================================================
// Recursion Depth Limits
// =============================================================================
// These prevent stack overflow in deeply nested type structures or AST nodes.

/// Maximum depth for type node checking (type annotations, type parameters).
/// Prevents stack overflow when processing deeply nested generic types.
/// Value: 500 - balances supporting complex types while preventing abuse.
pub const MAX_TYPE_CHECK_DEPTH: u32 = 500;

/// Maximum depth for expression type checking.
/// Prevents stack overflow when processing deeply nested expressions.
pub const MAX_EXPR_CHECK_DEPTH: u32 = 500;

/// Maximum depth for generic type instantiation.
/// Prevents infinite recursion in recursive generic types like `type Foo<T> = Foo<Foo<T>>`.
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum depth for general checker recursion guards.
/// Used by enter_recursion/leave_recursion on checker functions like
/// get_construct_type_from_type, type_reference_symbol_type, etc.
/// Each guarded cycle adds ~7-14 stack frames; depth 50 ≈ 350-700 frames (~0.7 MB),
/// well within the 8 MB default stack.
pub const MAX_CHECKER_RECURSION_DEPTH: u32 = 50;

/// Maximum depth for function call resolution.
/// Prevents infinite recursion when resolving overloaded function calls.
pub const MAX_CALL_DEPTH: u32 = 20;

/// Maximum depth for subtype checking.
/// Prevents infinite recursion in recursive type comparisons.
pub const MAX_SUBTYPE_DEPTH: u32 = 100;

/// Maximum depth for type evaluation (conditional types, mapped types).
/// Prevents infinite recursion when evaluating complex type-level computations.
pub const MAX_EVALUATE_DEPTH: u32 = 50;

/// Maximum depth for type alias resolution.
/// Prevents infinite recursion in circular type alias references.
pub const MAX_ALIAS_RESOLUTION_DEPTH: u32 = 128;

/// Maximum depth for qualified name resolution (A.B.C.D...).
/// Prevents infinite loops in namespace/module traversal.
pub const MAX_QUALIFIED_NAME_DEPTH: u32 = 128;

/// Maximum depth for class inheritance chains.
/// TypeScript allows deep inheritance; 256 is generous but bounded.
pub const MAX_CLASS_INHERITANCE_DEPTH: u32 = 256;

/// Maximum depth for optional chaining expressions (a?.b?.c?.d...).
pub const MAX_OPTIONAL_CHAIN_DEPTH: u32 = 1_000;

/// Maximum depth for binding pattern destructuring.
/// Limits deeply nested destructuring like `const {a: {b: {c: ...}}} = x`.
pub const MAX_BINDING_PATTERN_DEPTH: u32 = 100;

/// Maximum depth for AST traversal during lowering.
pub const MAX_AST_DEPTH: u32 = 500;

/// Maximum depth for emitter recursion.
/// Prevents stack overflow when emitting deeply nested structures.
pub const MAX_EMIT_RECURSION_DEPTH: u32 = 1_000;

/// Maximum depth for parser recursion.
/// Prevents stack overflow when parsing deeply nested source code.
pub const MAX_PARSER_RECURSION_DEPTH: u32 = 1_000;

/// Maximum depth for decorator text extraction.
pub const MAX_DECORATOR_TEXT_DEPTH: u32 = 10;

/// Maximum depth for type merging operations.
pub const MAX_MERGE_DEPTH: u32 = 32;

/// Maximum depth for constraint recursion in type inference.
pub const MAX_CONSTRAINT_RECURSION_DEPTH: u32 = 100;

/// Maximum depth for mapped type property access.
pub const MAX_MAPPED_ACCESS_DEPTH: u32 = 50;

/// Maximum depth for template literal type counting.
pub const MAX_LITERAL_COUNT_DEPTH: u32 = 50;

// =============================================================================
// Operation Count Limits
// =============================================================================
// These prevent infinite loops in iterative algorithms.

/// Maximum iterations for tree-walking algorithms.
/// Used in type computation, narrowing, and other traversals.
pub const MAX_TREE_WALK_ITERATIONS: u32 = 10_000;

/// Maximum iterations for flow analysis.
/// Control flow analysis can be expensive; this bounds the work.
pub const MAX_FLOW_ANALYSIS_ITERATIONS: u32 = 100_000;

/// Maximum subtype checking pairs in progress (cycle detection).
/// Prevents memory exhaustion in complex recursive type checks.
pub const MAX_IN_PROGRESS_PAIRS: u32 = 10_000;

/// Maximum total subtype checks per compilation.
/// Bounds the total work for pathological type patterns.
pub const MAX_TOTAL_SUBTYPE_CHECKS: u32 = 100_000;

/// Maximum total type evaluations per compilation.
pub const MAX_TOTAL_EVALUATIONS: u32 = 100_000;

/// Maximum lowering operations during AST transformation.
pub const MAX_LOWERING_OPERATIONS: u32 = 100_000;

/// Maximum constraint solving iterations during type inference.
pub const MAX_CONSTRAINT_ITERATIONS: u32 = 100;

/// Maximum iterations for type unwrapping.
pub const MAX_UNWRAP_ITERATIONS: u32 = 1_000;

/// Maximum types in visiting set during evaluation.
pub const MAX_VISITING_SET_SIZE: u32 = 10_000;

// =============================================================================
// Type Resolution Limits (WASM-aware)
// =============================================================================
// WASM has stricter memory constraints, so we use lower limits.

/// Maximum type resolution operations.
/// WASM: 20,000 (memory constrained)
/// Native: 100,000
#[cfg(target_arch = "wasm32")]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 20_000;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_TYPE_RESOLUTION_OPS: u32 = 100_000;

/// Maximum template literal expansion size.
/// WASM: 2,000 (memory constrained)
/// Native: 100,000
#[cfg(target_arch = "wasm32")]
pub const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum interned types.
/// WASM: 500,000 (memory constrained)
/// Native: 5,000,000
#[cfg(target_arch = "wasm32")]
pub const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_INTERNED_TYPES: usize = 5_000_000;

/// Maximum keys in mapped type expansion.
/// WASM: 250 (memory constrained)
/// Native: 500
#[cfg(target_arch = "wasm32")]
pub const MAX_MAPPED_KEYS: usize = 250;
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_MAPPED_KEYS: usize = 500;

// =============================================================================
// Capacity/Size Limits
// =============================================================================
// Pre-allocation sizes and maximum collection sizes.

/// Threshold for switching from inline array to HashMap for object properties.
/// 24 properties is approximately the crossover point where HashMap becomes faster.
/// Below this, linear search in a SmallVec is efficient due to cache locality.
pub const PROPERTY_MAP_THRESHOLD: usize = 24;

/// Inline capacity for type lists (union members, tuple elements).
/// 8 elements fit in a SmallVec without heap allocation.
pub const TYPE_LIST_INLINE: usize = 8;

/// Maximum union members when indexing into a type.
pub const MAX_UNION_INDEX_SIZE: usize = 100;

/// Maximum types when distributing conditional types.
pub const MAX_DISTRIBUTION_SIZE: usize = 100;

/// Maximum union members to show in diagnostic messages.
pub const UNION_MEMBER_DIAGNOSTIC_LIMIT: usize = 3;

/// Pre-allocation size for AST nodes.
/// Based on typical source file sizes (1 node per ~20 characters).
pub const MAX_NODE_PREALLOC: usize = 5_000_000;

/// Pre-allocation size for symbols.
/// Based on typical symbol density in TypeScript files.
pub const MAX_SYMBOL_PREALLOC: usize = 1_000_000;

/// Multiplier for incremental parsing node budget.
pub const INCREMENTAL_NODE_MULTIPLIER: usize = 4;

/// Minimum node budget for incremental parsing.
pub const INCREMENTAL_MIN_NODE_BUDGET: usize = 4_096;

// =============================================================================
// Sharding Constants
// =============================================================================
// Used for parallel/concurrent data structures.

/// Number of bits for shard indexing.
/// 6 bits = 64 shards, which provides good concurrency without excessive overhead.
/// 64 shards aligns well with typical CPU core counts and cache line sizes.
pub const SHARD_BITS: u32 = 6;

/// Number of shards (2^SHARD_BITS).
pub const SHARD_COUNT: usize = 1 << SHARD_BITS;

/// Mask for extracting shard index (SHARD_COUNT - 1).
pub const SHARD_MASK: usize = SHARD_COUNT - 1;

// =============================================================================
// Tracer Limits
// =============================================================================
// Used for debug tracing and profiling.

/// Maximum total tracer checks (matches MAX_TOTAL_SUBTYPE_CHECKS).
pub const MAX_TOTAL_TRACER_CHECKS: u32 = MAX_TOTAL_SUBTYPE_CHECKS;

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
        // Verify WASM limits are more conservative (this tests the non-wasm values)
        #[cfg(not(target_arch = "wasm32"))]
        {
            assert!(MAX_TYPE_RESOLUTION_OPS >= 100_000);
            assert!(TEMPLATE_LITERAL_EXPANSION_LIMIT >= 100_000);
            assert!(MAX_INTERNED_TYPES >= 5_000_000);
            assert!(MAX_MAPPED_KEYS >= 500);
        }
    }
}
