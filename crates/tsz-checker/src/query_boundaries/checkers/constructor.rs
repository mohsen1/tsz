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

/// Rebuild a class constructor type whose construct-signature return still
/// carries the provisional re-entrancy-window intersection built by
/// `rough_class_instance_return_type` — `Self<Params> & <rough prescan
/// instance>` — so the return is the class's own deferred parameterized
/// self-application alone. A finished class constructor's construct return is
/// the instance type itself, never that intersection; the wrapped form is a
/// mid-resolution snapshot (a static member of the class re-entered its own
/// resolution, e.g. through an import cycle) and the rough member must not
/// outlive the window (#17586).
///
/// Only an APPLICATION of the class's own `Lazy(own_def)` qualifies as the
/// replacement: it is the parameterized instance reference generic-`new`
/// inference instantiates, and it resolves against the finished class body. A
/// bare `Lazy(own_def)` member is the class's value-side self-reference (for
/// class expressions it resolves to `typeof C`, not the instance), so the
/// intersection is left untouched in that shape. Returns `None` when no
/// construct signature carries the rewriteable artifact.
pub(crate) fn construct_returns_without_self_window_artifact(
    db: &dyn TypeDatabase,
    ctor_type: TypeId,
    own_def: tsz_solver::def::DefId,
) -> Option<TypeId> {
    use super::super::common::{
        application_info, callable_shape_for_type, intersection_members, lazy_def_id,
    };

    let shape = callable_shape_for_type(db, ctor_type)?;
    let mut new_shape = (*shape).clone();
    let mut changed = false;
    for sig in &mut new_shape.construct_signatures {
        let Some(members) = intersection_members(db, sig.return_type) else {
            continue;
        };
        let self_application = members.iter().copied().find(|&member| {
            application_info(db, member)
                .is_some_and(|(base, _)| lazy_def_id(db, base) == Some(own_def))
        });
        if let Some(self_application) = self_application
            && members.len() > 1
        {
            sig.return_type = self_application;
            changed = true;
        }
    }
    changed.then(|| db.callable(new_shape))
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

#[cfg(test)]
mod provisional_ctor_window_artifact_tests {
    use super::construct_returns_without_self_window_artifact;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::DefId;
    use tsz_solver::{CallSignature, CallableShape, PropertyInfo, TypeId};

    fn ctor_with_construct_return(db: &TypeInterner, return_type: TypeId) -> TypeId {
        db.callable(CallableShape {
            construct_signatures: vec![CallSignature::new(Vec::new(), return_type)],
            ..CallableShape::default()
        })
    }

    fn rough_instance(db: &TypeInterner, name: &str) -> TypeId {
        db.object(vec![PropertyInfo::new(
            db.intern_string(name),
            TypeId::STRING,
        )])
    }

    fn sanitized(db: &TypeInterner, ctor: TypeId, own_def: DefId) -> Option<TypeId> {
        construct_returns_without_self_window_artifact(db, ctor, own_def)
    }

    fn construct_return(db: &TypeInterner, ctor: TypeId) -> TypeId {
        tsz_solver::type_queries::get_callable_shape(db, ctor)
            .expect("sanitized constructor keeps its callable shape")
            .construct_signatures[0]
            .return_type
    }

    #[test]
    fn rewrites_self_application_member_to_the_self_application() {
        let db = TypeInterner::new();
        let own_def = DefId(7001);
        let self_app = db.application(db.lazy(own_def), vec![TypeId::ANY, TypeId::ANY]);
        let provisional_return = db.intersection(vec![self_app, rough_instance(&db, "alpha")]);
        let ctor = ctor_with_construct_return(&db, provisional_return);

        let sanitized_ctor =
            sanitized(&db, ctor, own_def).expect("wrapped provisional return is sanitized");
        assert_eq!(construct_return(&db, sanitized_ctor), self_app);
    }

    #[test]
    fn ignores_bare_self_lazy_member() {
        // A bare `Lazy(own_def)` member is the value-side self-reference (for
        // class expressions it resolves to `typeof C`, not the instance);
        // rewriting to it would clobber the instance members, so the
        // intersection stays untouched.
        let db = TypeInterner::new();
        let own_def = DefId(7002);
        let self_ref = db.lazy(own_def);
        let provisional_return = db.intersection(vec![self_ref, rough_instance(&db, "beta")]);
        let ctor = ctor_with_construct_return(&db, provisional_return);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }

    #[test]
    fn ignores_intersection_of_other_definitions() {
        let db = TypeInterner::new();
        let own_def = DefId(7003);
        let other_def = DefId(7004);
        let other_app = db.application(db.lazy(other_def), vec![TypeId::ANY]);
        let mixed_return = db.intersection(vec![other_app, rough_instance(&db, "gamma")]);
        let ctor = ctor_with_construct_return(&db, mixed_return);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }

    #[test]
    fn ignores_plain_instance_construct_return() {
        let db = TypeInterner::new();
        let own_def = DefId(7005);
        let instance = db.application(db.lazy(own_def), vec![TypeId::ANY]);
        let ctor = ctor_with_construct_return(&db, instance);

        assert!(sanitized(&db, ctor, own_def).is_none());
    }

    #[test]
    fn ignores_non_callable_types() {
        let db = TypeInterner::new();
        assert!(sanitized(&db, TypeId::OBJECT, DefId(7006)).is_none());
    }
}
