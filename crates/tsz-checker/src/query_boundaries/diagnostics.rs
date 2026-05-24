use super::state::checking as state_checking;
use tsz_solver::TypeId;

pub(crate) use super::common::{
    application_info, array_element_type, callable_shape_for_type, intersection_members,
    lazy_def_id, union_members,
};
pub(crate) use tsz_solver::type_queries::AssignmentNumericDisplayChildren;

pub(crate) fn assignment_numeric_display_children(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> AssignmentNumericDisplayChildren {
    tsz_solver::type_queries::assignment_numeric_display_children(db, type_id)
}

pub(crate) fn object_shape_for_assignment_numeric_display(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    tsz_solver::type_queries::object_shape_for_assignment_numeric_display(db, type_id)
}

pub(crate) fn number_literal_bits(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<u64> {
    tsz_solver::type_queries::number_literal_bits(db, type_id)
}

pub(crate) fn is_number_literal_union(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_number_literal_union(db, type_id)
}

pub(crate) fn numeric_literal_union_origin_preserves_alias(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::numeric_literal_union_origin_preserves_alias(db, def_store, type_id)
}

pub(crate) fn collect_property_name_atoms_for_diagnostics(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    tsz_solver::type_queries::collect_property_name_atoms_for_diagnostics(db, type_id, max_depth)
}

/// Collect property names accessible on a type for spelling suggestions.
///
/// For union types, only properties present in ALL members are returned (intersection).
pub(crate) fn collect_accessible_property_names_for_suggestion(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    max_depth: usize,
) -> Vec<tsz_common::Atom> {
    if state_checking::union_members(db, type_id).is_none() {
        return collect_property_name_atoms_for_diagnostics(db, type_id, max_depth);
    }

    tsz_solver::type_queries::collect_accessible_property_names_for_suggestion(
        db, type_id, max_depth,
    )
}

pub(crate) fn function_shape(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn mapped_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<(
    tsz_solver::MappedTypeId,
    std::sync::Arc<tsz_solver::MappedType>,
)> {
    tsz_solver::type_queries::get_mapped_type_with_id(db, type_id)
}

pub(crate) fn type_application(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

pub(crate) fn same_non_class_nominal_application_surface<R: tsz_solver::resolver::TypeResolver>(
    db: &dyn tsz_solver::construction::TypeDatabase,
    resolver: &R,
    def_store: &tsz_solver::def::DefinitionStore,
    source_candidates: &[TypeId],
    target_candidates: &[TypeId],
) -> bool {
    source_candidates.iter().any(|&source_candidate| {
        let Some(source) = non_class_nominal_application_surface(db, def_store, source_candidate)
        else {
            return false;
        };

        target_candidates
            .iter()
            .filter_map(|&candidate| {
                non_class_nominal_application_surface(db, def_store, candidate)
            })
            .any(|target| nominal_application_surfaces_match(db, resolver, &source, &target))
    })
}

struct NominalApplicationSurface {
    def_id: tsz_solver::DefId,
    args: Vec<TypeId>,
}

fn nominal_application_surfaces_match<R: tsz_solver::resolver::TypeResolver>(
    db: &dyn tsz_solver::construction::TypeDatabase,
    resolver: &R,
    source: &NominalApplicationSurface,
    target: &NominalApplicationSurface,
) -> bool {
    source.def_id == target.def_id
        && source.args.len() == target.args.len()
        && source
            .args
            .iter()
            .zip(&target.args)
            .all(|(&source, &target)| {
                tsz_solver::relations::subtype::are_types_structurally_identical(
                    db, resolver, source, target,
                )
            })
}

fn non_class_nominal_application_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> Option<NominalApplicationSurface> {
    if is_type_query_surface(db, type_id) {
        return None;
    }

    let app = type_application(db, type_id).or_else(|| {
        db.get_display_alias(type_id)
            .filter(|&alias| !is_type_query_surface(db, alias))
            .and_then(|alias| type_application(db, alias))
    })?;
    if app.args.is_empty() || is_type_query_surface(db, app.base) {
        return None;
    }

    let def_id = lazy_def_id(db, app.base)?;
    let def = def_store.get(def_id)?;
    (!matches!(
        def.kind,
        tsz_solver::def::DefKind::Class | tsz_solver::def::DefKind::ClassConstructor
    ))
    .then(|| NominalApplicationSurface {
        def_id,
        args: app.args.clone(),
    })
}

fn is_type_query_surface(db: &dyn tsz_solver::construction::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_type_query_type(db, type_id)
        || db
            .get_display_alias(type_id)
            .is_some_and(|alias| tsz_solver::is_type_query_type(db, alias))
}

pub(crate) fn is_index_access_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, type_id)
}

pub(crate) fn contains_index_access_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}

pub(crate) fn application_base_has_conditional_alias_body(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::application_base_has_conditional_alias_body(db, def_store, type_id)
}

pub(crate) fn preserves_named_application_base(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id).is_some()
        || !matches!(
            tsz_solver::type_queries::classify_type_query(db, type_id),
            tsz_solver::type_queries::TypeQueryKind::Other
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::{DefinitionInfo, DefinitionStore};
    use tsz_solver::{PropertyInfo, SymbolRef, TypeParamInfo};

    fn register_interface_base(db: &TypeInterner, store: &DefinitionStore, name: &str) -> TypeId {
        let def_id = store.register(DefinitionInfo::interface(
            db.intern_string(name),
            vec![TypeParamInfo::simple(db.intern_string("T"))],
            vec![PropertyInfo::new(db.intern_string("value"), TypeId::STRING)],
        ));
        db.lazy(def_id)
    }

    #[test]
    fn non_class_nominal_application_surface_matches_by_def_id_for_renamed_interfaces() {
        for name in ["Carrier", "RenamedCarrier"] {
            let db = TypeInterner::new();
            let store = DefinitionStore::new();
            let base = register_interface_base(&db, &store, name);
            let source = db.application(base, vec![TypeId::STRING]);
            let target = db.application(base, vec![TypeId::STRING]);

            assert!(
                same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target],),
                "same interface application surface should match structurally for {name}"
            );
        }
    }

    #[test]
    fn non_class_nominal_application_surface_rejects_different_type_args() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let base = register_interface_base(&db, &store, "Carrier");
        let source = db.application(base, vec![TypeId::STRING]);
        let target = db.application(base, vec![TypeId::NUMBER]);

        assert!(
            !same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target]),
            "same generic base with different type arguments must not suppress TS2345"
        );
    }

    #[test]
    fn class_and_type_query_application_surfaces_do_not_match() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let class_def = store.register(DefinitionInfo::class(
            db.intern_string("Box"),
            vec![TypeParamInfo::simple(db.intern_string("T"))],
            vec![PropertyInfo::new(db.intern_string("value"), TypeId::STRING)],
            vec![],
        ));
        let class_app = db.application(db.lazy(class_def), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[class_app],
            &[class_app]
        ));

        let query_app = db.application(db.type_query(SymbolRef(7)), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[query_app],
            &[query_app]
        ));
    }
}
