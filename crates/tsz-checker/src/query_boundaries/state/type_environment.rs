use crate::state::CheckerState;
use tsz_solver::{MappedTypeId, TypeDatabase, TypeId};

pub(crate) use super::super::common::{
    collect_enum_def_ids, collect_referenced_types, is_generic_type, lazy_def_id,
    object_shape_for_type as object_shape,
};
pub(crate) use tsz_solver::type_queries::{
    MappedConstraintKind, PropertyAccessResolutionKind, TypeResolutionKind,
};

/// Thin wrapper around `tsz_solver::TypeEvaluator`.
///
/// Evaluates a complex type (conditional, mapped, index access, etc.) using
/// the provided `TypeResolver` to resolve lazy references. This delegates to
/// `TypeEvaluator::with_resolver` + `evaluate` in a single call.
pub(crate) fn evaluate_type_with_resolver<R: tsz_solver::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator = tsz_solver::TypeEvaluator::with_resolver(db, resolver);
    evaluator.evaluate(type_id)
}

pub(crate) fn application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

pub(crate) fn mapped_type_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<MappedTypeId> {
    tsz_solver::type_queries::get_mapped_type_id(db, type_id)
}

pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

pub(crate) fn classify_mapped_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> MappedConstraintKind {
    tsz_solver::type_queries::classify_mapped_constraint(db, type_id)
}

pub(crate) fn classify_for_type_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeResolutionKind {
    tsz_solver::type_queries::classify_for_type_resolution(db, type_id)
}

pub(crate) fn classify_for_property_access_resolution(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyAccessResolutionKind {
    tsz_solver::type_queries::classify_for_property_access_resolution(db, type_id)
}

pub(crate) fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ConditionalType>> {
    tsz_solver::type_queries::get_conditional_type(db, type_id)
}

pub(crate) fn is_union_or_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_union_type(db, type_id)
        || tsz_solver::type_queries::is_intersection_type(db, type_id)
}

pub(crate) use tsz_solver::type_queries::MappedSourceKind;

/// Classify a mapped-type source for array/tuple preservation decisions.
///
/// The checker uses this to decide whether to delegate to the solver's
/// tuple/array mapped evaluation or expand as a plain object.
pub(crate) fn classify_mapped_source(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    tsz_solver::type_queries::classify_mapped_source(db, source)
}

/// Check if a mapped type's `as` clause is identity-preserving.
pub(crate) fn is_identity_name_mapping(
    db: &dyn TypeDatabase,
    mapped: &tsz_solver::MappedType,
) -> bool {
    tsz_solver::type_queries::is_identity_name_mapping(db, mapped)
}

/// Compute modifier values for a mapped-type property.
pub(crate) fn compute_mapped_modifiers(
    mapped: &tsz_solver::MappedType,
    is_homomorphic: bool,
    source_optional: bool,
    source_readonly: bool,
) -> (bool, bool) {
    tsz_solver::type_queries::compute_mapped_modifiers(
        mapped,
        is_homomorphic,
        source_optional,
        source_readonly,
    )
}

/// Collect source property info for a homomorphic mapped type.
pub(crate) fn collect_homomorphic_source_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> rustc_hash::FxHashMap<tsz_common::Atom, (bool, bool, TypeId)> {
    tsz_solver::type_queries::collect_homomorphic_source_properties(db, source)
}

/// Expand a mapped type with resolved finite keys into PropertyInfo list.
pub(crate) fn expand_mapped_type_to_properties(
    db: &dyn TypeDatabase,
    mapped: &tsz_solver::MappedType,
    string_keys: &[tsz_common::Atom],
    source_props: &rustc_hash::FxHashMap<tsz_common::Atom, (bool, bool, TypeId)>,
    is_homomorphic: bool,
) -> Vec<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::expand_mapped_type_to_properties(
        db,
        mapped,
        string_keys,
        source_props,
        is_homomorphic,
    )
}

/// Re-export identity mapped type info from solver.
pub(crate) use tsz_solver::type_queries::IdentityMappedInfo;

/// Check if a mapped type is an identity homomorphic mapped type.
///
/// Returns info about the source type parameter if the mapped type has the
/// form `{ [K in keyof T]: T[K] }`. Used by application type evaluation to
/// decide primitive passthrough behavior.
pub(crate) fn classify_identity_mapped(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<IdentityMappedInfo> {
    tsz_solver::type_queries::classify_identity_mapped(db, mapped_id)
}

/// Evaluate identity mapped type passthrough for a given type argument.
///
/// For an identity homomorphic mapped type `{ [K in keyof T]: T[K] }`:
/// - Primitives pass through directly.
/// - `any` with array constraint passes through.
/// - `any` without array constraint → `{ [x: string]: any; [x: number]: any }`.
/// - `unknown`/`never`/`error` without array constraint → no passthrough.
/// - Non-identity → no passthrough.
///
/// Delegates to solver's centralized passthrough logic.
pub(crate) fn evaluate_identity_mapped_passthrough(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
    arg: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::evaluate_identity_mapped_passthrough(db, mapped_id, arg)
}

/// Get the inner type of a `keyof T` type.
///
/// Returns `Some(T)` if the type is `KeyOf(T)`, `None` otherwise.
pub(crate) fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::keyof_inner_type(db, type_id)
}

/// Get the constraint of a type parameter.
///
/// Returns `Some(constraint)` if the type is a `TypeParameter` or `Infer`
/// with a constraint, `None` otherwise. Used by the checker to discover
/// types reachable through type parameter constraints for pre-resolution
/// into the TypeEnvironment, without accessing TypeData directly.
pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

/// Check if a type is an array or tuple type.
pub(crate) fn is_array_or_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_or_tuple_type(db, type_id)
}

/// Reconstruct a mapped type with a new constraint, preserving all other fields.
///
/// Used when the checker evaluates a mapped type's constraint to concrete keys
/// and needs to create a new mapped type with the resolved constraint.
pub(crate) fn reconstruct_mapped_with_constraint(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
    new_constraint: TypeId,
) -> tsz_solver::MappedTypeId {
    tsz_solver::type_queries::reconstruct_mapped_with_constraint(db, mapped_id, new_constraint)
}

/// Collect finite property names from a mapped type's resolved constraint.
///
/// Returns `Some(names)` if the constraint resolves to a finite set of string
/// literal keys, `None` if the constraint is open-ended (e.g., `string`).
pub(crate) fn collect_finite_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<rustc_hash::FxHashSet<tsz_common::Atom>> {
    tsz_solver::type_queries::collect_finite_mapped_property_names(db, mapped_id)
}

/// Extract string literal keys from a type (union of string literals).
pub(crate) fn extract_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::extract_string_literal_keys(db, type_id)
}

/// Get the name of a type parameter (TypeParameter or Infer).
///
/// Returns `Some(name)` if the type is a type parameter, `None` otherwise.
/// Used by the checker to match type parameters against declared parameter
/// lists without accessing `TypeData` directly.
pub(crate) fn type_param_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<tsz_common::Atom> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id).map(|info| info.name)
}

/// Re-export the body arg preservation classification for application evaluation.
pub(crate) use tsz_solver::type_queries::BodyArgPreservation;

/// Classify a type body to decide how args should be handled during application evaluation.
///
/// Delegates to the solver's structural analysis of conditional-infer patterns.
pub(crate) fn classify_body_for_arg_preservation(
    db: &dyn TypeDatabase,
    body_type: TypeId,
) -> BodyArgPreservation {
    tsz_solver::type_queries::classify_body_for_arg_preservation(db, body_type)
}

/// Check if a type is a primitive (string, number, boolean, bigint, etc.).
pub(crate) fn is_primitive_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_primitive_type(db, type_id)
}

/// Check if a type contains `this` type references.
pub(crate) fn contains_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_this_type(db, type_id)
}

/// Substitute `this` type references in a type with a concrete type.
pub(crate) fn substitute_this_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    tsz_solver::substitute_this_type(db, type_id, this_type)
}

/// Get the intersection members of a type (if it is an intersection).
pub(crate) fn get_intersection_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

/// Check if a type is a discriminated object intersection.
pub(crate) fn is_discriminated_object_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_discriminated_object_intersection(db, type_id)
}

/// Check if a type contains infer types.
pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_infer_types(db, type_id)
}

/// Check if a type contains infer types (TypeDatabase-taking variant).
pub(crate) fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

/// Get the callable shape id from a type.
pub(crate) fn callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::CallableShapeId> {
    tsz_solver::callable_shape_id(db, type_id)
}

/// Check if a type is a type query symbol reference.
pub(crate) fn type_query_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::SymbolRef> {
    tsz_solver::type_query_symbol(db, type_id)
}

/// Result from a cached type evaluation, including side-effects needed by the checker.
pub(crate) struct EvalWithCacheResult {
    /// The evaluated type.
    pub result: TypeId,
    /// Whether the evaluator's recursion depth was exceeded.
    pub depth_exceeded: bool,
    /// Cache entries produced by the evaluator (key → evaluated value).
    pub cache_entries: Vec<(TypeId, TypeId)>,
}

/// Evaluate a type with a resolver, optionally seeding the evaluator cache.
///
/// Returns the result plus side-effects (depth exceeded, cache drain).
/// This is the canonical boundary for TypeEvaluator construction with cache
/// management — checker code must not construct TypeEvaluator directly.
pub(crate) fn evaluate_type_with_cache<R: tsz_solver::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    seed: impl Iterator<Item = (TypeId, TypeId)>,
    has_seed: bool,
) -> EvalWithCacheResult {
    let mut evaluator = tsz_solver::TypeEvaluator::with_resolver(db, resolver);
    if has_seed {
        evaluator.seed_cache(seed);
    }
    let result = evaluator.evaluate(type_id);
    EvalWithCacheResult {
        result,
        depth_exceeded: evaluator.is_depth_exceeded(),
        cache_entries: evaluator.drain_cache().collect(),
    }
}

/// Evaluate a type while suppressing `this` binding.
///
/// Used during heritage merging where `this` must remain unbound until the
/// final derived interface is constructed.
pub(crate) fn evaluate_type_suppressing_this<R: tsz_solver::TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator =
        tsz_solver::TypeEvaluator::with_resolver(db, resolver).with_suppress_this_binding();
    evaluator.evaluate(type_id)
}

/// Check if a type is a generic type application (`TypeData::Application`).
///
/// Thin wrapper to avoid direct `TypeData` pattern matching in checker code.
pub(crate) fn is_application_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}

/// Check if a type contains type query references (TypeDatabase-taking variant).
pub(crate) fn contains_type_query_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_query_db(db, type_id)
}

struct CheckerDeclarationCycleHost<'a, 'b> {
    state: &'a mut CheckerState<'b>,
}

impl tsz_solver::TypeResolver for CheckerDeclarationCycleHost<'_, '_> {
    fn resolve_ref(
        &self,
        symbol: tsz_solver::SymbolRef,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.state.ctx.resolve_ref(symbol, interner)
    }

    fn resolve_lazy(
        &self,
        def_id: tsz_solver::DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.state.ctx.resolve_lazy(def_id, interner)
    }
}

impl tsz_solver::type_queries::DeclarationTypeCycleHost for CheckerDeclarationCycleHost<'_, '_> {
    fn evaluate_application_for_serialization(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_application_type(type_id)
    }
}

pub(crate) fn declaration_type_references_cyclic_structure(
    state: &mut CheckerState<'_>,
    type_id: TypeId,
) -> bool {
    let db = state.ctx.types;
    let mut host = CheckerDeclarationCycleHost { state };
    tsz_solver::type_queries::declaration_type_references_cyclic_structure(db, &mut host, type_id)
}

#[cfg(test)]
#[path = "../../../tests/state_type_environment.rs"]
mod tests;
