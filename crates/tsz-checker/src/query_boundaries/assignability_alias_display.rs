//! Structural facts for assignability alias-display diagnostics.

use super::common;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::def::{DefKind, DefinitionStore};

pub(crate) fn source_preserves_declared_generic_alias_display(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> bool {
    common::is_intersection_type(db, source) || common::object_shape_id(db, source).is_some()
}

pub(crate) fn source_can_use_declared_generic_alias_annotation(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    source: TypeId,
) -> bool {
    source_can_use_declared_generic_alias_annotation_inner(db, definitions, source, 0)
}

fn source_can_use_declared_generic_alias_annotation_inner(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    source: TypeId,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    if common::contains_conditional_type(db, source) || common::is_callable_type(db, source) {
        return true;
    }
    if let Some(app) = common::type_application(db, source)
        && lazy_alias_body_contains_conditional(db, definitions, app.base, depth + 1)
    {
        return true;
    }
    if lazy_alias_body_contains_conditional(db, definitions, source, depth + 1) {
        return true;
    }
    common::union_members(db, source).is_some_and(|members| {
        members.iter().any(|&member| {
            source_can_use_declared_generic_alias_annotation_inner(
                db,
                definitions,
                member,
                depth + 1,
            )
        })
    }) || common::intersection_members(db, source).is_some_and(|members| {
        members.iter().any(|&member| {
            source_can_use_declared_generic_alias_annotation_inner(
                db,
                definitions,
                member,
                depth + 1,
            )
        })
    })
}

fn lazy_alias_body_contains_conditional(
    db: &dyn TypeDatabase,
    definitions: &DefinitionStore,
    type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let Some(def_id) = common::lazy_def_id(db, type_id) else {
        return false;
    };
    let Some(def) = definitions.get(def_id) else {
        return false;
    };
    if def.kind != DefKind::TypeAlias {
        return false;
    }
    let Some(body) = def.body else {
        return false;
    };
    common::contains_conditional_type(db, body)
        || source_can_use_declared_generic_alias_annotation_inner(db, definitions, body, depth + 1)
}

pub(crate) fn is_application_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::application_id(db, type_id).is_some()
}

pub(crate) fn is_object_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::object_shape_id(db, type_id).is_some()
}

pub(crate) fn contains_undefined_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    common::type_contains_undefined(db, type_id)
}

pub(crate) fn has_optional_parameter_undefined_surface(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    fn signature_has_optional_parameter(sig: &tsz_solver::CallSignature) -> bool {
        sig.params.iter().any(|param| param.optional)
    }

    if common::function_shape_for_type(db, type_id)
        .is_some_and(|shape| shape.params.iter().any(|param| param.optional))
    {
        return true;
    }

    if common::callable_shape_for_type(db, type_id).is_some_and(|shape| {
        shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .any(signature_has_optional_parameter)
    }) {
        return true;
    }

    common::union_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| has_optional_parameter_undefined_surface(db, member))
    }) || common::intersection_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| has_optional_parameter_undefined_surface(db, member))
    })
}

pub(crate) fn is_literal_for_alias_display(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_literal_or_literal_union_type(db, type_id)
        || common::is_template_literal_type(db, type_id)
}
