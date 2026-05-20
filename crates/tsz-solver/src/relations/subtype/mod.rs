//! Structural subtype checking.
//!
//! This module implements TypeScript's structural subtyping system:
//!
//! - `core`: Main subtype checker, coinductive cycle detection, type resolution
//! - `cache`: Relation caching with cycle-aware invalidation
//! - `explain`: Detailed failure reason generation for diagnostics
//! - `helpers`: Utility functions for common subtype patterns
//! - `overlap`: Discriminant overlap and disjointness checks
//! - `visitor`: Visitor-based type traversal for subtype relations
//! - `rules`: Category-specific subtype rules (objects, functions, literals, etc.)

pub(crate) mod cache;
pub(crate) mod core;
pub(crate) mod explain;
pub(crate) mod helpers;
pub(crate) mod overlap;
pub(crate) mod rules;
pub(crate) mod visitor;

// Re-export core items at the same visibility they were originally declared with.
// Items declared `pub` in core.rs are re-exported as `pub` so that lib.rs can
// publicly re-export them (e.g., SubtypeChecker, is_subtype_of).
pub use self::cache::reset_subtype_thread_local_state;
pub use self::core::*;

// Re-export SubtypeFailureReason so rules/ submodules can use `super::super::SubtypeFailureReason`
pub(crate) use crate::diagnostics::SubtypeFailureReason;
