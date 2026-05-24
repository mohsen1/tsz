use super::state::checking as state_checking;
use tsz_solver::{TypeId, construction::TypeDatabase};

pub(crate) use super::common::{
    application_info, array_element_type, callable_shape_for_type, enum_def_id,
    intersection_list_id, intersection_members, is_symbol_or_unique_symbol,
    is_template_literal_type, lazy_def_id, literal_value, no_infer_inner_type, union_list_id,
    union_members, widen_literal_to_primitive,
};
pub(crate) use tsz_solver::type_queries::AssignmentNumericDisplayChildren;

pub(crate) fn assignment_numeric_display_children(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
) -> AssignmentNumericDisplayChildren {
    tsz_solver::type_queries::assignment_numeric_display_children(db, type_id)
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
