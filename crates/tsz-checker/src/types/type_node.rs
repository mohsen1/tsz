use super::queries::lib_resolution::keyword_syntax_to_type_id;

use super::type_node_helpers::{
    check_duplicate_parameters_in_type, check_parameter_initializers_in_type,
    type_node_includes_explicit_undefined,
};

use crate::context::CheckerContext;

use tsz_binder::SymbolId;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::recursion::{DepthCounter, RecursionProfile};

use tsz_solver::{TypeId, Visibility};

/// Type node checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type resolution for type nodes goes through this checker.
pub struct TypeNodeChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
    /// Recursion depth counter for stack overflow protection.
    depth: DepthCounter,
}

pub(super) type TypeLiteralSignatureScopeUpdates = Vec<(String, Option<TypeId>)>;

include!("type_node_parts/part1.rs");
include!("type_node_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/type_node.rs"]
mod tests;
