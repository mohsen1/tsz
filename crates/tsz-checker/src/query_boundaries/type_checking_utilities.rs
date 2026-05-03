use tsz_solver::{TupleListId, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::UnionMembersKind;
pub(crate) use tsz_solver::type_queries::{
    ArrayLikeKind, ElementIndexableKind, IndexKeyKind, LiteralKeyKind, LiteralTypeKind,
    TypeQueryKind,
};

pub(crate) fn tuple_list_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TupleListId> {
    tsz_solver::type_queries::get_tuple_list_id(db, type_id)
}

pub(crate) fn application_base(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_application_base(db, type_id)
}

pub(crate) fn literal_key_kind(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralKeyKind {
    tsz_solver::type_queries::classify_literal_key(db, type_id)
}

pub(crate) fn classify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralTypeKind {
    tsz_solver::type_queries::classify_literal_type(db, type_id)
}

pub(crate) fn classify_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> ArrayLikeKind {
    tsz_solver::type_queries::classify_array_like(db, type_id)
}

pub(crate) use super::common::unwrap_readonly as unwrap_readonly_for_lookup;

pub(crate) fn classify_index_key(db: &dyn TypeDatabase, type_id: TypeId) -> IndexKeyKind {
    tsz_solver::type_queries::classify_index_key(db, type_id)
}

pub(crate) fn classify_element_indexable(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ElementIndexableKind {
    tsz_solver::type_queries::classify_element_indexable(db, type_id)
}

pub(crate) fn classify_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> TypeQueryKind {
    tsz_solver::type_queries::classify_type_query(db, type_id)
}

pub(crate) fn get_invalid_index_type_member(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_invalid_index_type_member(db, type_id)
}

pub(crate) fn get_invalid_index_type_member_strict(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_invalid_index_type_member_strict(db, type_id)
}

pub(crate) fn classify_for_union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> UnionMembersKind {
    tsz_solver::type_queries::classify_for_union_members(db, type_id)
}

pub(crate) fn get_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) use super::common::{
    EvaluationNeeded, classify_for_evaluation, lazy_def_id, type_application,
};

/// Whether the AST node at `idx` is a bare type-parameter reference whose
/// name resolves to a TypeParameter symbol in the current lexical scope.
/// Used to suppress the "any cannot be used as an index type" check when
/// our type resolution collapsed the parameter to `any` — tsc keeps the
/// index syntactically generic and defers rejection to instantiation time.
pub(crate) fn ast_index_node_is_in_scope_type_parameter(
    arena: &tsz_parser::parser::node::NodeArena,
    binder: &tsz_binder::BinderState,
    type_parameter_scope: &rustc_hash::FxHashMap<String, TypeId>,
    idx: tsz_parser::parser::NodeIndex,
) -> bool {
    use tsz_binder::symbol_flags;
    use tsz_parser::parser::syntax_kind_ext;
    let Some(node) = arena.get(idx) else {
        return false;
    };
    if node.kind != syntax_kind_ext::TYPE_REFERENCE {
        return false;
    }
    let Some(type_ref) = arena.get_type_ref(node) else {
        return false;
    };
    if type_ref
        .type_arguments
        .as_ref()
        .is_some_and(|args| !args.nodes.is_empty())
    {
        return false;
    }
    let name_idx = type_ref.type_name;
    let Some(name_node) = arena.get(name_idx) else {
        return false;
    };
    if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
        return false;
    }
    let Some(ident) = arena.get_identifier(name_node) else {
        return false;
    };
    if type_parameter_scope.contains_key(ident.escaped_text.as_str()) {
        return true;
    }
    if let Some(sym_id) = binder.resolve_identifier(arena, name_idx)
        && let Some(symbol) = binder.get_symbol(sym_id)
        && symbol.has_any_flags(symbol_flags::TYPE_PARAMETER)
    {
        return true;
    }
    false
}
