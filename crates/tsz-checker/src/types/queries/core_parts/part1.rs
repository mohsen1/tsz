use crate::query_boundaries::common::{
    TypeQueryKind, classify_type_query, contains_error_type, contains_type_parameters,
    split_nullish_type,
};

use crate::state::{CheckerState, MemberAccessLevel};

use tsz_binder::symbol_flags;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::{SyntaxKind, keyword_to_text_static};

use tsz_solver::TypeId;

/// Extract a property name from a non-computed property name node.
///
/// Handles identifiers, string literals, no-substitution template literals,
/// numeric literals (canonicalized via `canonicalize_numeric_name`), and
/// signed numeric literals (`+1`, `-1`) matching TSC's `isSignedNumericLiteral`.
/// Does NOT handle computed property names — callers must handle those separately
/// when symbol resolution or special formatting is needed.
pub(crate) fn get_literal_property_name(arena: &NodeArena, name_idx: NodeIndex) -> Option<String> {
    let name_node = arena.get(name_idx)?;

    if let Some(keyword) = SyntaxKind::try_from_u16(name_node.kind).and_then(keyword_to_text_static)
    {
        return Some(keyword.to_string());
    }

    // Identifier
    if let Some(ident) = arena.get_identifier(name_node) {
        return Some(ident.escaped_text.clone());
    }

    // String literal, no-substitution template literal, or numeric literal
    if matches!(
        name_node.kind,
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == SyntaxKind::NumericLiteral as u16
    ) && let Some(lit) = arena.get_literal(name_node)
    {
        // Canonicalize numeric property names (e.g. "1.", "1.0" -> "1")
        if name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(canonical) = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
        {
            return Some(canonical);
        }
        return Some(lit.text.clone());
    }

    // Signed numeric literal: prefix +/- with numeric literal operand.
    // TSC's isSignedNumericLiteral handles `[+1]` → "1" and `[-1]` → "-1".
    if name_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        && let Some(unary) = arena.get_unary_expr(name_node)
        && (unary.operator == SyntaxKind::PlusToken as u16
            || unary.operator == SyntaxKind::MinusToken as u16)
        && let Some(operand_node) = arena.get(unary.operand)
        && operand_node.kind == SyntaxKind::NumericLiteral as u16
        && let Some(lit) = arena.get_literal(operand_node)
    {
        let num_text = tsz_solver::utils::canonicalize_numeric_name(&lit.text)
            .unwrap_or_else(|| lit.text.clone());
        if unary.operator == SyntaxKind::MinusToken as u16 {
            return Some(format!("-{num_text}"));
        }
        return Some(num_text);
    }

    None
}

/// Like [`get_literal_property_name`] but also maps `[Symbol.<name>]`
/// computed property names to the canonical `[Symbol.<name>]` key so
/// TS2320/TS2430 heritage checks match well-known-symbol members across
/// bases. User-defined `unique symbol` bindings still need symbol
/// resolution and are out of scope here.
pub(crate) fn get_literal_or_well_known_property_name(
    arena: &NodeArena,
    name_idx: NodeIndex,
) -> Option<String> {
    if let Some(name) = get_literal_property_name(arena, name_idx) {
        return Some(name);
    }
    let name_node = arena.get(name_idx)?;
    if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
        return None;
    }
    let mut expr = arena.get_computed_property(name_node)?.expression;
    // Peel parentheses.
    while let Some(node) = arena.get(expr)
        && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
    {
        expr = arena.get_parenthesized(node)?.expression;
    }
    let node = arena.get(expr)?;
    if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    {
        return None;
    }
    let access = arena.get_access_expr(node)?;
    if arena
        .get_identifier(arena.get(access.expression)?)?
        .escaped_text
        != "Symbol"
    {
        return None;
    }
    let name_node = arena.get(access.name_or_argument)?;
    if let Some(ident) = arena.get_identifier(name_node) {
        return Some(format!("[Symbol.{}]", ident.escaped_text));
    }
    if matches!(
        name_node.kind,
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
    ) && let Some(lit) = arena.get_literal(name_node)
        && !lit.text.is_empty()
    {
        return Some(format!("[Symbol.{}]", lit.text));
    }
    None
}
