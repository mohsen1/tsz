//! Stateless helpers for `narrowing.rs`.
//!
//! Maps AST kinds and primitive identifier names to `TypeId` intrinsics, plus
//! resolution of a const variable's type annotation to a primitive/object-like intrinsic.
//! Lifted out of `narrowing.rs` to keep that file under the 2000-LOC ceiling.

use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use crate::query_boundaries::common::{TypeDatabase, TypeEnvironment};
use crate::query_boundaries::flow_analysis as flow_query;

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

/// Returns true when the identifier at `idx` spells `undefined` AND resolves
/// to the global `undefined` (a lib symbol or unresolved). A user-declared
/// local named `undefined` (parameter, variable, etc.) shadows the global
/// and must not be treated as the literal `undefined` sentinel.
pub(super) fn is_global_undefined_identifier(
    arena: &NodeArena,
    binder: &BinderState,
    idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };
    let Some(ident) = arena.get_identifier(node) else {
        return false;
    };
    if ident.escaped_text != "undefined" {
        return false;
    }
    if let Some(sym_id) = binder
        .get_node_symbol(idx)
        .or_else(|| binder.resolve_identifier(arena, idx))
        && !binder.lib_symbol_ids.contains(&sym_id)
    {
        return false;
    }
    true
}

/// Resolve a const variable's `type_annotation` node to a primitive/object-like
/// `TypeId` when the annotation can act as an `unknown` equality comparand.
pub(super) fn const_annotation_intrinsic_type(
    arena: &NodeArena,
    ann_idx: NodeIndex,
) -> Option<TypeId> {
    let ann_node = arena.get(ann_idx)?;
    if let Some(intrinsic) = primitive_keyword_intrinsic(ann_node.kind) {
        return Some(intrinsic);
    }
    if matches!(
        ann_node.kind,
        k if k == tsz_parser::parser::syntax_kind_ext::TYPE_LITERAL
            || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_TYPE
            || k == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE
    ) {
        return Some(TypeId::OBJECT);
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

pub(super) fn evaluate_predicate_instantiation(
    db: &dyn TypeDatabase,
    type_environment: Option<&std::cell::RefCell<TypeEnvironment>>,
    instantiated: TypeId,
) -> TypeId {
    let evaluated = if let Some(env) = type_environment {
        let env_borrow = env.borrow();
        let with_env = flow_query::evaluate_application_type(db, &env_borrow, instantiated);
        if with_env == instantiated {
            flow_query::evaluate_type_structure(db, instantiated)
        } else {
            with_env
        }
    } else {
        flow_query::evaluate_type_structure(db, instantiated)
    };
    if evaluated == TypeId::ANY && instantiated != TypeId::ANY {
        instantiated
    } else {
        evaluated
    }
}
