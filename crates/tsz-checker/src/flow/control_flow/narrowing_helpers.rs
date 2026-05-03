//! Stateless helpers for `narrowing.rs`.
//!
//! Maps AST kinds and primitive identifier names to `TypeId` intrinsics, plus
//! resolution of a const variable's type annotation to a primitive intrinsic.
//! Lifted out of `narrowing.rs` to keep that file under the 2000-LOC ceiling.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

const fn primitive_keyword_intrinsic(kind: u16) -> Option<TypeId> {
    match kind {
        k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
        k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
        k if k == SyntaxKind::StringKeyword as u16 => Some(TypeId::STRING),
        k if k == SyntaxKind::NumberKeyword as u16 => Some(TypeId::NUMBER),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some(TypeId::BOOLEAN),
        k if k == SyntaxKind::BigIntKeyword as u16 => Some(TypeId::BIGINT),
        k if k == SyntaxKind::SymbolKeyword as u16 => Some(TypeId::SYMBOL),
        k if k == SyntaxKind::ObjectKeyword as u16 => Some(TypeId::OBJECT),
        _ => None,
    }
}

fn primitive_name_intrinsic(name: &str) -> Option<TypeId> {
    match name {
        "string" => Some(TypeId::STRING),
        "number" => Some(TypeId::NUMBER),
        "boolean" => Some(TypeId::BOOLEAN),
        "bigint" => Some(TypeId::BIGINT),
        "symbol" => Some(TypeId::SYMBOL),
        "object" => Some(TypeId::OBJECT),
        _ => None,
    }
}

/// Resolve a const variable's `type_annotation` node to a primitive `TypeId`
/// when the annotation is a primitive keyword (e.g. `string`) or a no-arg
/// type reference whose name is a primitive identifier.
pub(super) fn const_annotation_intrinsic_type(
    arena: &NodeArena,
    ann_idx: NodeIndex,
) -> Option<TypeId> {
    let ann_node = arena.get(ann_idx)?;
    if let Some(intrinsic) = primitive_keyword_intrinsic(ann_node.kind) {
        return Some(intrinsic);
    }
    if ann_node.kind != tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
        return None;
    }
    let ty_ref = arena.get_type_ref(ann_node)?;
    if ty_ref
        .type_arguments
        .as_ref()
        .is_some_and(|args| !args.nodes.is_empty())
    {
        return None;
    }
    let name_node = arena.get(ty_ref.type_name)?;
    let ident = arena.get_identifier(name_node)?;
    primitive_name_intrinsic(ident.escaped_text.as_str())
}
