use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_MEMBER_NAME, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS,
    CONTEXT_FLAG_FUNCTION_BODY, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_GENERATOR_MEMBER_NAME,
    CONTEXT_FLAG_STATIC_BLOCK, ParserState,
};

use crate::parser::{
    NodeIndex, NodeList,
    node::{self},
    syntax_kind_ext,
};

use tsz_common::Atom;

use tsz_common::diagnostics::diagnostic_codes;

use tsz_scanner::SyntaxKind;

/// Pre-classified modifier flags for a single class member, computed in one
/// pass through the combined decorator + keyword-modifier list.
///
/// Constructed once by `scan_class_member_modifier_phase` so that all
/// downstream dispatch in `parse_class_member` reads named boolean fields
/// instead of performing repeated linear scans over the modifier node list.
pub(crate) struct ClassMemberModifierSet {
    /// Combined decorators + keyword modifiers in source order.
    /// Retained for AST construction and diagnostic-position lookups.
    pub(crate) modifiers: Option<NodeList>,
    /// `true` when at least one decorator was present.
    pub(crate) has_decorators: bool,
    /// `var` or `let` appeared as a modifier (invalid; triggers specific recovery).
    pub(crate) has_var_let: bool,
    pub(crate) has_static: bool,
    pub(crate) has_export: bool,
    pub(crate) has_declare: bool,
    pub(crate) has_accessor: bool,
    pub(crate) has_async: bool,
    /// Diagnostic-list length captured just before modifier parsing, used to
    /// selectively roll back modifier-ordering diagnostics when a static block
    /// is discovered after modifiers were already parsed.
    pub(crate) diag_len_before_modifiers: usize,
}

include!("state_statements_class_members_parts/part1.rs");
include!("state_statements_class_members_parts/part2.rs");
