//! Domain-specific checker modules.
//!
//! Each module implements type-checking logic for a particular language feature,
//! delegating type-semantic queries to the solver via `query_boundaries`.

pub mod accessor_checker;
pub mod call_checker;
pub mod enum_checker;
pub mod generic_checker;
pub mod iterable_checker;
pub mod jsx_checker;
pub mod jsx_checker_attrs;
mod jsx_checker_helpers;
mod jsx_checker_runtime;
pub mod parameter_checker;
pub mod promise_checker;
pub mod property_checker;
pub mod signature_builder;

use tsz_parser::parser::base::NodeIndex;
use tsz_solver::TypeId;

/// Explicit context for synthesized JSX children, threaded from dispatch
/// into the JSX checking path instead of stored as ambient mutable state
/// on `CheckerContext`.
#[derive(Clone)]
pub struct JsxChildrenContext {
    /// Number of children in the JSX body.
    pub child_count: usize,
    /// Whether any `JsxText` children exist.
    pub has_text_child: bool,
    /// The type to use as the `children` prop value.
    pub synthesized_type: TypeId,
    /// Node indices of `JsxText` children (for TS2747 location reporting).
    pub text_child_indices: Vec<NodeIndex>,
}
