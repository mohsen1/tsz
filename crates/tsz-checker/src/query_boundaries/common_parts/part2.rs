pub(crate) fn collect_type_queries(
    db: &dyn TypeDatabase,
    root: TypeId,
) -> Vec<tsz_solver::SymbolRef> {
    tsz_solver::visitor::collect_type_queries(db, root)
}

pub(crate) fn is_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_literal(db, type_id)
}

pub(crate) fn callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::CallableShapeId> {
    tsz_solver::visitor::callable_shape_id(db, type_id)
}

pub(crate) fn enum_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(tsz_solver::def::DefId, TypeId)> {
    tsz_solver::visitor::enum_components(db, type_id)
}

pub(crate) fn union_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeListId> {
    tsz_solver::visitor::union_list_id(db, type_id)
}

/// Factory for `BinaryOpEvaluator` — single construction point through the boundary.
///
/// All checker code that needs binary-op evaluation must construct the evaluator
/// through this function instead of calling `BinaryOpEvaluator::new()` directly.
pub(crate) fn new_binary_op_evaluator(
    db: &dyn tsz_solver::construction::QueryDatabase,
) -> tsz_solver::operations::BinaryOpEvaluator<'_> {
    tsz_solver::operations::BinaryOpEvaluator::new(db)
}

pub(crate) fn intersection_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeListId> {
    tsz_solver::visitor::intersection_list_id(db, type_id)
}

pub(crate) fn tuple_list_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TupleListId> {
    tsz_solver::visitor::tuple_list_id(db, type_id)
}

pub(crate) fn unique_symbol_ref(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::visitor::unique_symbol_ref(db, type_id)
}

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_object_like_type(db, type_id)
}

pub(crate) fn is_enum_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_enum_type(db, type_id)
}

pub(crate) fn is_lazy_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_lazy_type(db, type_id)
}

pub(crate) fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_primitive_type(db, type_id)
}

pub(crate) fn is_literal_type_through_type_constraints(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::visitor::is_literal_type_through_type_constraints(db, type_id)
}

pub(crate) fn has_late_bound_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::has_late_bound_members(db, type_id)
}

pub(crate) fn object_with_index_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::ObjectShapeId> {
    tsz_solver::visitor::object_with_index_shape_id(db, type_id)
}

pub(crate) fn contains_type_parameter_named_shallow(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    tsz_solver::visitor::contains_type_parameter_named_shallow(db, type_id, name)
}

pub(crate) fn no_infer_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::visitor::no_infer_inner_type(db, type_id)
}

/// Alias for `readonly_inner_type` — same semantics, consistent naming.
pub(crate) fn readonly_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::visitor::readonly_inner_type(db, type_id)
}

/// Alias for `type_query_symbol` — extracts the symbol ref from a `typeof T` type.
pub(crate) fn type_query_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::visitor::type_query_symbol(db, type_id)
}

pub(crate) fn walk_referenced_types<F>(db: &dyn TypeDatabase, type_id: TypeId, visitor: F)
where
    F: FnMut(TypeId),
{
    tsz_solver::visitor::walk_referenced_types(db, type_id, visitor)
}

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_function_type(db, type_id)
}

pub(crate) fn remove_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::narrowing::remove_undefined(db, type_id)
}

pub(crate) fn remove_nullish(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::narrowing::remove_nullish(db, type_id)
}

pub(crate) fn contains_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_this_type(db, type_id)
}

pub(crate) fn function_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::FunctionShapeId> {
    tsz_solver::function_shape_id(db, type_id)
}

pub(crate) fn evaluate_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::computation::evaluate_type(db, type_id)
}

pub(crate) fn widen_type_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::widen_type_deep(db, type_id)
}

/// Display-widen a type for TS2403 redeclaration messages.
///
/// Thin boundary wrapper over `tsz_solver::operations::widening::display_widen_for_redeclaration`.
/// See the solver definition for semantics — preserves top-level literal /
/// literal-union types while deep-widening fresh literals nested inside
/// compound shapes.
pub(crate) fn display_widen_for_redeclaration(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::display_widen_for_redeclaration(db, type_id)
}

pub(crate) fn string_intrinsic_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(tsz_solver::StringIntrinsicKind, TypeId)> {
    tsz_solver::string_intrinsic_components(db, type_id)
}

pub(crate) fn is_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_error_type(db, type_id)
}

pub(crate) fn is_module_namespace_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_module_namespace_type(db, type_id)
}

pub(crate) fn is_nullish_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::narrowing::is_nullish_type(db, type_id)
}

pub(crate) fn is_structurally_deferred_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_structurally_deferred_type(db, type_id)
}

pub(crate) fn type_contains_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::narrowing::type_contains_undefined(db, type_id)
}

pub(crate) fn instantiate_function_with_type_args(
    db: &dyn TypeDatabase,
    function_type: TypeId,
    type_args: &[TypeId],
) -> Option<TypeId> {
    c::instantiate_function_with_type_args(db, function_type, type_args)
}

pub(crate) fn normalize_object_union_members_for_write_target(
    db: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Vec<TypeId>> {
    tsz_solver::operations::normalize_object_union_members_for_write_target(db, members)
}

pub(crate) fn index_access_parts(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::index_access_parts(db, type_id)
}

pub(crate) fn split_nullish_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<TypeId>, Option<TypeId>) {
    tsz_solver::narrowing::split_nullish_type(db, type_id)
}

pub(crate) fn instantiate_type_preserving_meta(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    c::instantiate_type_preserving_meta_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        substitution,
    )
}

pub(crate) fn get_base_type_for_comparison(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    widening::get_base_type_for_comparison(db, type_id)
}

pub(crate) fn apply_contextual_type(
    db: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    tsz_solver::computation::apply_contextual_type(db, expr_type, contextual_type)
}

pub(crate) fn resolve_default_type_args(
    db: &dyn TypeDatabase,
    type_params: &[tsz_solver::TypeParamInfo],
) -> Vec<TypeId> {
    tsz_solver::resolve_default_type_args(db, type_params)
}

pub(crate) fn constraint_references_type_param_in_resolution_path(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: tsz_common::interner::Atom,
) -> bool {
    tsz_solver::constraint_references_type_param_in_resolution_path(db, type_id, param_name)
}

pub(crate) fn has_deferred_conditional_member(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::has_deferred_conditional_member(db, type_id)
}

pub(crate) use super::operator_wrappers::{
    is_assignment_operator, is_compound_assignment_operator,
    is_logical_compound_assignment_operator, map_compound_assignment_to_binary,
};
