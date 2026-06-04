use super::state::checking as state_checking;
use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::def::{DefKind, DefinitionStore};
use tsz_solver::relations::subtype::TypeResolver;

pub(crate) use super::common::{
    PropertyAccessResult, application_info, array_element_type, callable_shape_for_type,
    contains_free_type_parameters, enum_def_id, get_indexed_access_type, get_type_query_symbol_ref,
    intersection_list_id, intersection_members, is_symbol_or_unique_symbol,
    is_template_literal_type, lazy_def_id, literal_value, no_infer_inner_type,
    object_shape_for_type, string_literal_value, type_has_displayable_name,
    type_parameter_constraint, union_list_id, union_members, widen_literal_to_primitive,
    widen_type_deep,
};
pub(crate) use tsz_solver::type_queries::AssignmentNumericDisplayChildren;

pub(crate) fn assignment_numeric_display_children(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> AssignmentNumericDisplayChildren {
    tsz_solver::type_queries::assignment_numeric_display_children(db, type_id)
}

/// `true` when `type_id` is an anonymous object type, or a union / intersection
/// that contains one (recursing through nested unions / intersections).
pub(crate) fn union_or_intersection_mentions_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::union_or_intersection_mentions_object(db, type_id)
}

/// Check whether an application's *declared* alias body is a mapped type
/// (e.g. `Partial<X>`, `Readonly<X>`, or `type F<T> = { [K in keyof T]... }`),
/// even when the concrete instantiation is fully resolved. Diagnostic
/// elaboration uses this to elaborate mapped-alias mismatches structurally
/// rather than via type-argument variance, matching tsc.
pub(crate) fn application_base_is_mapped_type<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::application_base_is_mapped_type_db(db, resolver, type_id)
}

pub(crate) fn alias_application_body_reduces_through_conditional_or_indexed(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(def_id) = super::common::get_application_lazy_def_id(db, type_id) else {
        return false;
    };
    let Some(def) = definitions.get(def_id) else {
        return false;
    };
    def.kind == DefKind::TypeAlias
        && def.body.is_some_and(|body| {
            alias_body_reduces_through_conditional_or_indexed(db, definitions, body, 0)
        })
}

pub(crate) fn evaluated_alias_application_has_concrete_display(
    db: &dyn TypeDatabase,
    candidate: TypeId,
    evaluated: TypeId,
) -> bool {
    candidate != evaluated
        && evaluated != TypeId::ERROR
        && !super::common::is_conditional_type(db, evaluated)
        && !super::common::is_index_access_type(db, evaluated)
        && !super::common::contains_type_parameters(db, evaluated)
}

fn alias_body_reduces_through_conditional_or_indexed(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    if super::common::is_index_access_type(db, type_id)
        || super::common::is_conditional_type(db, type_id)
    {
        return true;
    }
    if let Some(app) = super::common::type_application(db, type_id)
        && let Some(def_id) = super::common::lazy_def_id(db, app.base)
        && let Some(def) = definitions.get(def_id)
        && def.kind == DefKind::TypeAlias
        && let Some(body) = def.body
        && alias_body_reduces_through_conditional_or_indexed(db, definitions, body, depth + 1)
    {
        return true;
    }
    if let Some(def_id) = super::common::lazy_def_id(db, type_id)
        && let Some(def) = definitions.get(def_id)
        && def.kind == DefKind::TypeAlias
        && let Some(body) = def.body
    {
        return alias_body_reduces_through_conditional_or_indexed(db, definitions, body, depth + 1);
    }
    false
}

pub(crate) fn is_typeof_result_union(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    const STRING: u8 = 1 << 0;
    const NUMBER: u8 = 1 << 1;
    const BIGINT: u8 = 1 << 2;
    const BOOLEAN: u8 = 1 << 3;
    const SYMBOL: u8 = 1 << 4;
    const UNDEFINED: u8 = 1 << 5;
    const OBJECT: u8 = 1 << 6;
    const FUNCTION: u8 = 1 << 7;
    const ALL: u8 = STRING | NUMBER | BIGINT | BOOLEAN | SYMBOL | UNDEFINED | OBJECT | FUNCTION;

    let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) else {
        return false;
    };
    if members.len() != 8 {
        return false;
    }

    let mut seen = 0u8;
    for member in members {
        let Some(atom) = tsz_solver::type_queries::get_string_literal_value(db, member) else {
            return false;
        };
        let bit = match db.resolve_atom_ref(atom).as_ref() {
            "string" => STRING,
            "number" => NUMBER,
            "bigint" => BIGINT,
            "boolean" => BOOLEAN,
            "symbol" => SYMBOL,
            "undefined" => UNDEFINED,
            "object" => OBJECT,
            "function" => FUNCTION,
            _ => return false,
        };
        seen |= bit;
    }

    seen == ALL
}

pub(crate) fn object_shape_for_assignment_numeric_display(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
    tsz_solver::type_queries::object_shape_for_assignment_numeric_display(db, type_id)
}

pub(crate) fn is_global_object_interface_for_diagnostic(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    if db
        .get_boxed_type(tsz_solver::IntrinsicKind::Object)
        .is_some_and(|object_type| object_type == type_id)
    {
        return true;
    }
    lazy_def_id(db, type_id)
        .is_some_and(|def_id| db.is_boxed_def_id(def_id, tsz_solver::IntrinsicKind::Object))
}

pub(crate) fn simple_intersection_head_for_this_assignment_display(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let members = super::common::intersection_members(db, type_id)?;
    let head = members.first().copied()?;
    if super::common::type_application(db, head).is_some() {
        return None;
    }
    if super::common::object_shape_for_type(db, head).is_some()
        && !super::common::type_has_displayable_name(db, head)
    {
        return None;
    }
    Some(head)
}

pub(crate) fn distinct_type_parameters_share_declared_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    source_param: TypeId,
    target_param: TypeId,
) -> bool {
    if source_param == target_param {
        return false;
    }
    let Some(source_info) = super::common::type_param_info(db, source_param) else {
        return false;
    };
    let Some(target_info) = super::common::type_param_info(db, target_param) else {
        return false;
    };
    source_info.name == target_info.name
}

pub(crate) fn distinct_types_share_nominal_diagnostic_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    binder: &tsz_binder::BinderState,
    def_store: &tsz_solver::def::DefinitionStore,
    source: TypeId,
    target: TypeId,
) -> bool {
    if source == target {
        return false;
    }
    let Some(source_name) = nominal_diagnostic_name(db, binder, def_store, source) else {
        return false;
    };
    nominal_diagnostic_name(db, binder, def_store, target).is_some_and(|target_name| {
        target_name == source_name && !is_primitive_diagnostic_name(&target_name)
    })
}

fn nominal_diagnostic_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    binder: &tsz_binder::BinderState,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> Option<String> {
    if let Some(app) = type_application(db, type_id)
        && let Some(name) = nominal_diagnostic_name(db, binder, def_store, app.base)
    {
        return Some(name);
    }
    if let Some(alias) = db.get_display_alias(type_id)
        && alias != type_id
        && let Some(name) = nominal_diagnostic_name(db, binder, def_store, alias)
    {
        return Some(name);
    }
    if let Some(def_id) = lazy_def_id(db, type_id)
        && let Some(def) = def_store.get(def_id)
    {
        return Some(db.resolve_atom_ref(def.name).to_string());
    }
    let shape = object_shape_for_type(db, type_id)?;
    let symbol = binder.get_symbol(shape.symbol?)?;
    (!symbol.escaped_name.is_empty()).then(|| symbol.escaped_name.clone())
}

fn is_primitive_diagnostic_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
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

pub(crate) fn finite_mapped_property_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some((mapped_id, mapped)) = tsz_solver::type_queries::get_mapped_type_with_id(db, type_id)
    else {
        return false;
    };
    if mapped_key_constraint_has_named_origin(db, mapped.constraint) {
        return false;
    }
    tsz_solver::type_queries::collect_finite_mapped_property_names(db, mapped_id).is_some()
}

fn mapped_key_constraint_has_named_origin(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    if tsz_solver::type_queries::get_enum_def_id(db, type_id).is_some() {
        return true;
    }
    if tsz_solver::type_queries::get_lazy_def_id(db, type_id).is_some() {
        return true;
    }
    tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .copied()
            .any(|member| mapped_key_constraint_has_named_origin(db, member))
    })
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

pub(crate) fn contains_never_index_access_surface(
    db: &dyn tsz_solver::construction::TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
    max_depth: usize,
) -> bool {
    tsz_solver::type_queries::contains_never_index_access_surface(db, def_store, type_id, max_depth)
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
