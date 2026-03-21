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
