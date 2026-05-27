//! Public declaration surfaces for mapped types.

use crate::construction::TypeDatabase;
use crate::types::{IntrinsicKind, MappedType, ObjectFlags, PropertyInfo, TypeData, TypeId};
use tsz_common::Atom;

/// Return the public declaration surface for an inferred mapped type whose
/// source is a constrained type parameter.
///
/// Declaration emit needs the type that `tsc` exposes for an inferred public
/// variable, not the deferred implementation form `{ [K in keyof T]: ... }`
/// when `T` only survives as a generic constraint. This query keeps that
/// reduction structural: primitive constraints pass through, while object-like
/// constraints are substituted into the mapped source and evaluated normally.
pub fn inferred_declaration_mapped_constraint_surface(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    inferred_declaration_mapped_constraint_surface_with(db, type_id, |ty| {
        crate::evaluation::evaluate::evaluate_type(db, ty)
    })
}

pub fn inferred_declaration_mapped_constraint_surface_with(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut evaluate: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    let mapped_id = crate::mapped_type_id(db, type_id)?;
    let mapped = db.mapped_type(mapped_id);
    let source = crate::keyof_inner_type(db, mapped.constraint)?;
    let Some(source_param) = crate::type_param_info(db, source) else {
        let source = evaluate(source);
        if is_primitive_or_primitive_union_for_mapped_surface(db, source) {
            return Some(source);
        }
        let evaluated = mapped_surface_with_optional_undefined(db, evaluate(type_id));
        return (evaluated != type_id
            && !matches!(db.lookup(evaluated), Some(TypeData::Mapped(_)))
            && is_concrete_object_like_mapped_surface_source(db, evaluated))
        .then_some(evaluated);
    };
    let declared_constraint = source_param.constraint?;
    let constraint = evaluate(declared_constraint);

    if is_primitive_or_primitive_union_for_mapped_surface(db, constraint) {
        return Some(constraint);
    }
    if is_number_wrapper_display_source(db, declared_constraint, constraint) {
        db.store_display_alias(constraint, declared_constraint);
    }

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type_preserving};

    let subst = TypeSubstitution::single(source_param.name, constraint);
    let substituted = MappedType {
        type_param: mapped.type_param,
        constraint: instantiate_type_preserving(db, mapped.constraint, &subst),
        name_type: mapped
            .name_type
            .map(|name_type| instantiate_type_preserving(db, name_type, &subst)),
        template: instantiate_type_preserving(db, mapped.template, &subst),
        readonly_modifier: mapped.readonly_modifier,
        optional_modifier: mapped.optional_modifier,
    };
    let substituted_type = db.mapped(substituted);
    let mut evaluated = evaluate(substituted_type);
    if let Some(sorted) =
        sort_number_wrapper_surface_object(db, declared_constraint, constraint, evaluated)
    {
        evaluated = sorted;
    }
    evaluated = mapped_surface_with_optional_undefined(db, evaluated);
    (evaluated != type_id && !matches!(db.lookup(evaluated), Some(TypeData::Mapped(_))))
        .then_some(evaluated)
}

fn mapped_surface_with_optional_undefined(db: &dyn TypeDatabase, surface: TypeId) -> TypeId {
    let Some(TypeData::Object(shape_id)) = db.lookup(surface) else {
        return surface;
    };
    let shape = db.object_shape(shape_id);
    let mut changed = false;
    let mut properties = shape.properties.clone();
    for prop in &mut properties {
        if !prop.optional || super::type_includes_undefined(db, prop.type_id) {
            continue;
        }
        let original_type = prop.type_id;
        prop.type_id = db.union2(prop.type_id, TypeId::UNDEFINED);
        if prop.write_type == original_type {
            prop.write_type = prop.type_id;
        }
        changed = true;
    }
    if changed {
        db.object_with_flags_and_symbol(properties, shape.flags, shape.symbol)
    } else {
        surface
    }
}

fn is_concrete_object_like_mapped_surface_source(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Mapped(_)
            | TypeData::Function(_)
            | TypeData::Callable(_),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => {
            is_concrete_object_like_mapped_surface_source(db, inner)
        }
        Some(TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .copied()
            .all(|member| is_concrete_object_like_mapped_surface_source(db, member)),
        _ => false,
    }
}

fn is_primitive_or_primitive_union_for_mapped_surface(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if crate::is_primitive_type(db, type_id) {
        return true;
    }
    let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
        return false;
    };
    db.type_list(list_id)
        .iter()
        .copied()
        .all(|member| crate::is_primitive_type(db, member))
}

pub fn sort_number_wrapper_properties_for_display(
    db: &dyn TypeDatabase,
    source: TypeId,
    resolved_source: TypeId,
    props: &mut [PropertyInfo],
) -> bool {
    if !is_number_wrapper_display_source(db, source, resolved_source) {
        return false;
    }
    if props.len() == 6
        && props
            .iter()
            .all(|prop| number_wrapper_rank(db, prop.name).is_some())
    {
        props.sort_by_key(|prop| number_wrapper_rank(db, prop.name));
        for (index, prop) in props.iter_mut().enumerate() {
            prop.declaration_order = (index + 1) as u32;
        }
        return true;
    }
    false
}

fn sort_number_wrapper_surface_object(
    db: &dyn TypeDatabase,
    source: TypeId,
    resolved_source: TypeId,
    type_id: TypeId,
) -> Option<TypeId> {
    let Some(TypeData::Object(shape_id)) = db.lookup(type_id) else {
        return None;
    };
    let mut props = db.object_shape(shape_id).properties.clone();
    sort_number_wrapper_properties_for_display(db, source, resolved_source, &mut props)
        .then(|| db.object_with_flags(props, ObjectFlags::PRESERVE_DECLARATION_ORDER))
}

fn is_number_wrapper_display_source(
    db: &dyn TypeDatabase,
    source: TypeId,
    resolved_source: TypeId,
) -> bool {
    is_number_wrapper_type_id(db, source)
        || is_number_wrapper_type_id(db, resolved_source)
        || db
            .get_display_alias(source)
            .is_some_and(|alias| is_number_wrapper_type_id(db, alias))
        || db
            .get_display_alias(resolved_source)
            .is_some_and(|alias| is_number_wrapper_type_id(db, alias))
}

fn is_number_wrapper_type_id(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    db.get_boxed_type(IntrinsicKind::Number) == Some(type_id)
        || matches!(
            db.lookup(type_id),
            Some(TypeData::Lazy(def_id)) if db.is_boxed_def_id(def_id, IntrinsicKind::Number)
        )
        || match db.lookup(type_id) {
            Some(TypeData::Application(app_id)) => {
                is_number_wrapper_type_id(db, db.type_application(app_id).base)
            }
            _ => false,
        }
}

fn number_wrapper_rank(db: &dyn TypeDatabase, name: Atom) -> Option<usize> {
    ([
        "toString",
        "toFixed",
        "toExponential",
        "toPrecision",
        "valueOf",
        "toLocaleString",
    ])
    .iter()
    .position(|candidate| db.resolve_atom_ref(name).as_ref() == *candidate)
}
