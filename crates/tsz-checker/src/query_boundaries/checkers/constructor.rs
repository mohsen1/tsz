use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{PropertyInfo, TypeId};

pub(crate) use super::super::common::has_construct_signatures;
pub(crate) use tsz_solver::type_queries::{
    AbstractConstructorAnchor, ConstructorAccessKind, ConstructorReturnMergeKind, InstanceTypeKind,
    construct_return_type_for_type,
};

pub(crate) fn classify_for_instance_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> InstanceTypeKind {
    tsz_solver::type_queries::classify_for_instance_type(db, type_id)
}

pub(crate) fn classify_for_constructor_return_merge(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorReturnMergeKind {
    tsz_solver::type_queries::classify_for_constructor_return_merge(db, type_id)
}

pub(crate) fn resolve_abstract_constructor_anchor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AbstractConstructorAnchor {
    tsz_solver::type_queries::resolve_abstract_constructor_anchor(db, type_id)
}

pub(crate) fn classify_for_constructor_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorAccessKind {
    tsz_solver::type_queries::classify_for_constructor_access(db, type_id)
}

/// Get the construct return type for a single constructor type member.
/// Returns the raw return type (possibly Lazy) without resolution,
/// suitable for display name formatting that preserves named type references.
pub(crate) fn construct_return_type_for_display(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    construct_return_type_for_type(db, type_id)
}

pub(crate) fn constructor_return_intersection_or_single(
    db: &dyn TypeDatabase,
    returns: Vec<TypeId>,
) -> TypeId {
    tsz_solver::utils::intersection_or_single(db, returns)
}

pub(crate) fn constructor_instance_intersection_or_single(
    db: &dyn TypeDatabase,
    instance_types: Vec<TypeId>,
) -> TypeId {
    tsz_solver::utils::intersection_or_single(db, instance_types)
}

pub(crate) fn mixin_returned_class_instance_type(
    db: &dyn TypeDatabase,
    returned_instance: TypeId,
    base_instance: TypeId,
) -> TypeId {
    db.intersection2(returned_instance, base_instance)
}

pub(crate) fn mixin_instance_returns_with_base_last(
    db: &dyn TypeDatabase,
    returns: Vec<TypeId>,
    base_instance: TypeId,
) -> TypeId {
    tsz_solver::type_queries::mixin_instance_returns_with_base_last(db, returns, base_instance)
}

pub(crate) fn mixin_return_type_with_base_constructor(
    db: &dyn TypeDatabase,
    return_type: TypeId,
    base_arg_type: TypeId,
) -> TypeId {
    db.intersection2(return_type, base_arg_type)
}

pub(crate) fn constructor_type_without_abstract_flag(
    db: &dyn TypeDatabase,
    ctor_type: TypeId,
) -> TypeId {
    match classify_for_constructor_return_merge(db, ctor_type) {
        ConstructorReturnMergeKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            if !shape.is_abstract {
                return ctor_type;
            }
            let mut new_shape = (*shape).clone();
            new_shape.is_abstract = false;
            db.callable(new_shape)
        }
        ConstructorReturnMergeKind::Intersection(members) => {
            let mut updated_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for member in members {
                let updated = constructor_type_without_abstract_flag(db, member);
                if updated != member {
                    changed = true;
                }
                updated_members.push(updated);
            }
            if changed {
                db.intersection(updated_members)
            } else {
                ctor_type
            }
        }
        ConstructorReturnMergeKind::Function(_) | ConstructorReturnMergeKind::Other => ctor_type,
    }
}

pub(crate) fn constructor_type_with_construct_return(
    db: &dyn TypeDatabase,
    ctor_type: TypeId,
    instance_type: TypeId,
) -> TypeId {
    match classify_for_constructor_return_merge(db, ctor_type) {
        ConstructorReturnMergeKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            let mut new_shape = (*shape).clone();
            for sig in &mut new_shape.construct_signatures {
                sig.return_type = instance_type;
            }
            db.callable(new_shape)
        }
        ConstructorReturnMergeKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            if !shape.is_constructor {
                return ctor_type;
            }
            let mut new_shape = (*shape).clone();
            new_shape.return_type = instance_type;
            db.function(new_shape)
        }
        ConstructorReturnMergeKind::Intersection(members) => {
            let mut updated_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for member in members {
                let updated = constructor_type_with_construct_return(db, member, instance_type);
                if updated != member {
                    changed = true;
                }
                updated_members.push(updated);
            }
            if changed {
                db.intersection(updated_members)
            } else {
                ctor_type
            }
        }
        ConstructorReturnMergeKind::Other => ctor_type,
    }
}

pub(crate) fn constructor_type_with_base_instance_return(
    db: &dyn QueryDatabase,
    ctor_type: TypeId,
    base_instance_type: TypeId,
) -> TypeId {
    match classify_for_constructor_return_merge(db, ctor_type) {
        ConstructorReturnMergeKind::Callable(_) | ConstructorReturnMergeKind::Function(_) => {
            let result = tsz_solver::type_queries::data::intersect_constructor_returns(
                db,
                ctor_type,
                base_instance_type,
            );
            if result != ctor_type {
                result
            } else {
                ctor_type
            }
        }
        ConstructorReturnMergeKind::Intersection(members) => {
            let mut updated_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for member in members {
                let updated =
                    constructor_type_with_base_instance_return(db, member, base_instance_type);
                if updated != member {
                    changed = true;
                }
                updated_members.push(updated);
            }
            if changed {
                db.intersection(updated_members)
            } else {
                ctor_type
            }
        }
        ConstructorReturnMergeKind::Other => ctor_type,
    }
}

pub(crate) fn constructor_type_with_base_properties(
    db: &dyn TypeDatabase,
    ctor_type: TypeId,
    base_props: &FxHashMap<Atom, PropertyInfo>,
) -> TypeId {
    if base_props.is_empty() {
        return ctor_type;
    }

    match classify_for_constructor_return_merge(db, ctor_type) {
        ConstructorReturnMergeKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            let mut prop_map: FxHashMap<Atom, PropertyInfo> = shape
                .properties
                .iter()
                .map(|prop| (prop.name, prop.clone()))
                .collect();
            for (name, prop) in base_props {
                prop_map.entry(*name).or_insert_with(|| prop.clone());
            }
            let mut new_shape = (*shape).clone();
            new_shape.properties = prop_map.into_values().collect();
            db.callable(new_shape)
        }
        ConstructorReturnMergeKind::Intersection(members) => {
            let mut updated_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for member in members {
                let updated = constructor_type_with_base_properties(db, member, base_props);
                if updated != member {
                    changed = true;
                }
                updated_members.push(updated);
            }
            if changed {
                db.intersection(updated_members)
            } else {
                ctor_type
            }
        }
        ConstructorReturnMergeKind::Function(_) | ConstructorReturnMergeKind::Other => ctor_type,
    }
}
