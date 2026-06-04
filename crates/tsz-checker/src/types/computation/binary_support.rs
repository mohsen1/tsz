use crate::context::TypingRequest;

use crate::query_boundaries::type_computation::core::{
    WriteTargetLogicalOperator, WriteTargetLogicalResult,
};

use crate::state::CheckerState;

use tsz_binder::symbol_flags;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

/// Result of syntactic nullishness analysis, mirroring tsc's `PredicateSemantics`.
/// This is a purely syntactic check -- it does NOT look at types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SyntacticNullishness {
    /// The expression is always nullish (e.g., `null`, `undefined`).
    #[allow(dead_code)]
    Always,
    /// The expression may or may not be nullish (e.g., identifiers, calls, property accesses).
    Sometimes,
    /// The expression is never nullish (e.g., literals, arithmetic results, `??` results).
    Never,
}

include!("binary_support_parts/part1.rs");
include!("binary_support_parts/part2.rs");
