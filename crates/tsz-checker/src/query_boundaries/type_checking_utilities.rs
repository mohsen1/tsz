use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{TupleListId, TypeId};

/// Returns `true` when `type_id`'s outer shape performs fresh tuple synthesis
/// on evaluation. Used to attribute the `tuple_too_large` flag to the alias
/// whose body owns the synthesis rather than to a transitive referrer.
pub(crate) fn is_fresh_tuple_synthesis_site(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_fresh_tuple_synthesis_site(db, type_id)
}

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

/// Outcome-shaped relation for rest tuple element array-like probes.
///
/// `TypeNodeChecker` does not own a full checker state, so it cannot call the
/// checker-state rest-parameter relation helper directly. Keep the database
/// relation inside this boundary while exposing the same [`RelationOutcome`]
/// shape that checker-state relation helpers use.
///
/// [`RelationOutcome`]: super::assignability::RelationOutcome
pub(crate) fn rest_element_array_like_relation_outcome(
    db: &dyn QueryDatabase,
    source: TypeId,
    target: TypeId,
) -> super::assignability::RelationOutcome {
    let result = tsz_solver::relations::relation_queries::query_relation(
        db.as_type_database(),
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Assignable,
        tsz_solver::relations::relation_queries::RelationPolicy::unflagged_compatibility(),
        tsz_solver::relations::relation_queries::RelationContext::default(),
    );

    super::assignability::RelationOutcome {
        related: result.related,
        depth_exceeded: result.depth_exceeded,
        iteration_exceeded: result.iteration_exceeded,
        failure: None,
        weak_union_violation: false,
        property_classification: None,
    }
}

pub(crate) use super::common::unwrap_readonly as unwrap_readonly_for_lookup;

pub(crate) fn classify_index_key(db: &dyn TypeDatabase, type_id: TypeId) -> IndexKeyKind {
    tsz_solver::type_queries::classify_index_key(db, type_id)
}

/// Resolver-aware element-indexability classification.
///
/// The checker always calls this variant. It threads a [`TypeResolver`] —
/// typically the [`CheckerContext`] acting as a `TypeEnvironment` — through to
/// the solver so that `Application(Lazy(DefId), args)` shapes (`Record<K, V>`,
/// user mapped aliases, `Partial<T>`, `Readonly<T>`, …) expand to their
/// structural mapped/object form before classification. Without a resolver
/// those wrappers stay opaque, the classifier returns `Other`, and the
/// checker emits false TS7053 diagnostics for type-parameter constraints
/// that mention them (including intersection constraints such as
/// `T extends { a: number } & Record<string, V>`).
///
/// There is intentionally no non-resolver-aware wrapper exposed here.
/// Indexability decisions made without a resolver have produced regressions
/// (see #10726); making this the only entry point keeps that mistake out of
/// the checker tree.
///
/// [`TypeResolver`]: tsz_solver::relations::subtype::TypeResolver
/// [`CheckerContext`]: crate::context::CheckerContext
pub(crate) fn classify_element_indexable_with_resolver<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> ElementIndexableKind {
    tsz_solver::type_queries::classify_element_indexable_with_resolver(db, resolver, type_id)
}

pub(crate) fn classify_type_query(db: &dyn TypeDatabase, type_id: TypeId) -> TypeQueryKind {
    tsz_solver::type_queries::classify_type_query(db, type_id)
}

pub(crate) fn is_object_intrinsic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::OBJECT
        || tsz_solver::intrinsic_kind(db, type_id) == Some(tsz_solver::IntrinsicKind::Object)
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

pub(crate) fn union_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::TypeIdList> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn get_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::TypeIdList> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) use super::common::{
    EvaluationNeeded, classify_for_evaluation, lazy_def_id, type_application,
};

/// Whether the AST node at `idx` is a bare type-parameter reference whose
/// name resolves to a `TypeParameter` symbol in the current lexical scope.
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
