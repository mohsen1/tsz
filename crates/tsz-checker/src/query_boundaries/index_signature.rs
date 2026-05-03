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
                composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| is_valid_index_sig_param_type_ast(arena, binder, m))
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
