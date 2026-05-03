//! AST-level index signature parameter type validity checks.
//!
//! Mirrors tsc's `isValidIndexKeyType` for the AST surface. Used at TS1268
//! emission sites as a fallback when the resolved key `TypeId` is a composite
//! (e.g. a `string | number` union, or a `string & Brand` intersection) that
//! doesn't match the primitive equality check but the AST shape is structurally
//! valid.

use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Structural AST check for index-signature parameter type validity.
///
/// Accepts `string`/`number`/`symbol` keywords, template literal types, type
/// aliases that resolve to one of the above, unions whose members are all
/// valid, and non-generic intersections where some member is valid (e.g.
/// `string & Brand`, or two pattern-literal templates intersected).
pub(crate) fn is_valid_index_sig_param_type_ast(
    arena: &NodeArena,
    binder: &tsz_binder::BinderState,
    type_annotation_idx: NodeIndex,
) -> bool {
    let Some(type_node) = arena.get(type_annotation_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == SyntaxKind::StringKeyword as u16 => true,
        k if k == SyntaxKind::NumberKeyword as u16 => true,
        k if k == SyntaxKind::SymbolKeyword as u16 => true,
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => true,
        k if k == syntax_kind_ext::UNION_TYPE => {
            arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    !composite.types.nodes.is_empty()
                        && composite
                            .types
                            .nodes
                            .iter()
                            .all(|&m| is_valid_index_sig_param_type_ast(arena, binder, m))
                })
        }
        k if k == syntax_kind_ext::INTERSECTION_TYPE => arena
            .get_composite_type(type_node)
            .is_some_and(|composite| {
                // Accept the intersection only when at least one member is
                // a structurally valid index-sig type AND no member contains
                // a generic type parameter or literal. This prevents
                // `T & string` from being treated as valid (which would
                // suppress the more specific TS1337 diagnostic).
                let any_valid = composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| is_valid_index_sig_param_type_ast(arena, binder, m));
                let any_generic_or_literal = composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| contains_type_param_or_literal_ast(arena, binder, m));
                any_valid && !any_generic_or_literal
            }),
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            let Some(type_ref) = arena.get_type_ref(type_node) else {
                return false;
            };
            if let Some(name_node) = arena.get(type_ref.type_name)
                && let Some(ident) = arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();
                if matches!(name, "string" | "number" | "symbol") {
                    return true;
                }
            }
            if let Some(sym_id) = binder.resolve_identifier(arena, type_ref.type_name)
                && let Some(symbol) = binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                && let Some(&decl_idx) = symbol.declarations.first()
                && let Some(decl_node) = arena.get(decl_idx)
                && let Some(type_alias) = arena.get_type_alias(decl_node)
            {
                return is_valid_index_sig_param_type_ast(arena, binder, type_alias.type_node);
            }
            false
        }
        _ => false,
    }
}

/// AST-level check: does `type_annotation_idx` contain (recursively) a
/// generic type parameter reference or a literal type? Used to gate the
/// intersection arm of `is_valid_index_sig_param_type_ast` so that
/// `T & string` is rejected (and the more specific TS1337 diagnostic
/// can fire instead of being suppressed).
pub(crate) fn contains_type_param_or_literal_ast(
    arena: &NodeArena,
    binder: &tsz_binder::BinderState,
    type_annotation_idx: NodeIndex,
) -> bool {
    let Some(type_node) = arena.get(type_annotation_idx) else {
        return false;
    };

    if type_node.kind == syntax_kind_ext::LITERAL_TYPE
        || type_node.kind == SyntaxKind::StringLiteral as u16
        || type_node.kind == SyntaxKind::NumericLiteral as u16
        || type_node.kind == SyntaxKind::TrueKeyword as u16
        || type_node.kind == SyntaxKind::FalseKeyword as u16
    {
        return true;
    }

    if type_node.kind == syntax_kind_ext::UNION_TYPE
        || type_node.kind == syntax_kind_ext::INTERSECTION_TYPE
    {
        if let Some(composite) = arena.get_composite_type(type_node) {
            return composite
                .types
                .nodes
                .iter()
                .any(|&m| contains_type_param_or_literal_ast(arena, binder, m));
        }
        return false;
    }

    if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(type_node)
        && let Some(sym_id) = binder.resolve_identifier(arena, type_ref.type_name)
        && let Some(symbol) = binder.get_symbol(sym_id)
    {
        if (symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER) != 0 {
            return true;
        }
        // Recurse into a type alias body so `type S = T & string` is also caught.
        if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
            && let Some(&decl_idx) = symbol.declarations.first()
            && let Some(decl_node) = arena.get(decl_idx)
            && let Some(type_alias) = arena.get_type_alias(decl_node)
        {
            return contains_type_param_or_literal_ast(arena, binder, type_alias.type_node);
        }
    }

    false
}
