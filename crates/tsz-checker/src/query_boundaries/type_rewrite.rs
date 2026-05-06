//! Type rewrite helpers that intentionally inspect solver type shapes at the
//! checker boundary.

use tsz_solver::{QueryDatabase, SymbolRef, TypeData, TypeId};

pub(crate) fn replace_type_queries_and_lazies_with(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    mut replacement_for: impl FnMut(SymbolRef) -> Option<TypeId>,
    mut replacement_for_lazy: impl FnMut(tsz_solver::def::DefId) -> Option<TypeId>,
) -> TypeId {
    fn rewrite(
        db: &dyn QueryDatabase,
        type_id: TypeId,
        replacement_for: &mut impl FnMut(SymbolRef) -> Option<TypeId>,
        replacement_for_lazy: &mut impl FnMut(tsz_solver::def::DefId) -> Option<TypeId>,
        active: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        if !active.insert(type_id) {
            return type_id;
        }
        if let Some(element) =
            tsz_solver::visitor::array_element_type(db.as_type_database(), type_id)
        {
            let rewritten_element =
                rewrite(db, element, replacement_for, replacement_for_lazy, active);
            let rewritten = if rewritten_element == element {
                type_id
            } else {
                db.factory().array(rewritten_element)
            };
            active.remove(&type_id);
            return rewritten;
        }
        let Some(key) = db.lookup(type_id) else {
            active.remove(&type_id);
            return type_id;
        };
        let rewritten = match key {
            TypeData::TypeQuery(symbol) => replacement_for(symbol).unwrap_or(type_id),
            TypeData::Lazy(def_id) => replacement_for_lazy(def_id).unwrap_or(type_id),
            TypeData::Union(list_id) => {
                let members = db.type_list(list_id);
                let rewritten: Vec<_> = members
                    .iter()
                    .map(|&member| {
                        rewrite(db, member, replacement_for, replacement_for_lazy, active)
                    })
                    .collect();
                if members
                    .iter()
                    .zip(rewritten.iter())
                    .all(|(&before, &after)| before == after)
                {
                    type_id
                } else {
                    db.factory().union_preserve_members(rewritten)
                }
            }
            TypeData::Intersection(list_id) => {
                let members = db.type_list(list_id);
                let rewritten: Vec<_> = members
                    .iter()
                    .map(|&member| {
                        rewrite(db, member, replacement_for, replacement_for_lazy, active)
                    })
                    .collect();
                if members
                    .iter()
                    .zip(rewritten.iter())
                    .all(|(&before, &after)| before == after)
                {
                    type_id
                } else {
                    db.factory().intersection(rewritten)
                }
            }
            TypeData::Tuple(list_id) => {
                let elements = db.tuple_list(list_id);
                let rewritten: Vec<_> = elements
                    .iter()
                    .map(|element| {
                        let mut element = *element;
                        element.type_id = rewrite(
                            db,
                            element.type_id,
                            replacement_for,
                            replacement_for_lazy,
                            active,
                        );
                        element
                    })
                    .collect();
                if elements
                    .iter()
                    .zip(rewritten.iter())
                    .all(|(&before, &after)| before == after)
                {
                    type_id
                } else {
                    db.factory().tuple(rewritten)
                }
            }
            TypeData::Function(shape_id) => {
                let mut shape = db.function_shape(shape_id).as_ref().clone();
                let mut changed = false;
                for param in &mut shape.params {
                    let rewritten = rewrite(
                        db,
                        param.type_id,
                        replacement_for,
                        replacement_for_lazy,
                        active,
                    );
                    if rewritten != param.type_id {
                        param.type_id = rewritten;
                        changed = true;
                    }
                }
                let return_type = rewrite(
                    db,
                    shape.return_type,
                    replacement_for,
                    replacement_for_lazy,
                    active,
                );
                if return_type != shape.return_type {
                    shape.return_type = return_type;
                    changed = true;
                }
                let this_type = shape.this_type.map(|this_type| {
                    rewrite(db, this_type, replacement_for, replacement_for_lazy, active)
                });
                if this_type != shape.this_type {
                    shape.this_type = this_type;
                    changed = true;
                }
                if let Some(mut predicate) = shape.type_predicate {
                    let predicate_type = predicate.type_id.map(|type_id| {
                        rewrite(db, type_id, replacement_for, replacement_for_lazy, active)
                    });
                    if predicate_type != predicate.type_id {
                        predicate.type_id = predicate_type;
                        shape.type_predicate = Some(predicate);
                        changed = true;
                    }
                }
                if changed {
                    db.factory().function(shape)
                } else {
                    type_id
                }
            }
            TypeData::Application(app_id) => {
                let app = db.type_application(app_id);
                let base = rewrite(db, app.base, replacement_for, replacement_for_lazy, active);
                let args: Vec<_> = app
                    .args
                    .iter()
                    .map(|&arg| rewrite(db, arg, replacement_for, replacement_for_lazy, active))
                    .collect();
                if base == app.base && args.iter().zip(app.args.iter()).all(|(&a, &b)| a == b) {
                    type_id
                } else {
                    db.factory().application(base, args)
                }
            }
            TypeData::IndexAccess(object_type, index_type) => {
                let object_type = rewrite(
                    db,
                    object_type,
                    replacement_for,
                    replacement_for_lazy,
                    active,
                );
                let index_type = rewrite(
                    db,
                    index_type,
                    replacement_for,
                    replacement_for_lazy,
                    active,
                );
                if let Some(TypeData::IndexAccess(before_object, before_index)) = db.lookup(type_id)
                    && object_type == before_object
                    && index_type == before_index
                {
                    type_id
                } else {
                    db.factory().index_access(object_type, index_type)
                }
            }
            TypeData::KeyOf(inner) => {
                let inner = rewrite(db, inner, replacement_for, replacement_for_lazy, active);
                if let Some(TypeData::KeyOf(before)) = db.lookup(type_id)
                    && inner == before
                {
                    type_id
                } else {
                    db.factory().keyof(inner)
                }
            }
            TypeData::ReadonlyType(inner) => {
                let inner = rewrite(db, inner, replacement_for, replacement_for_lazy, active);
                if let Some(TypeData::ReadonlyType(before)) = db.lookup(type_id)
                    && inner == before
                {
                    type_id
                } else {
                    db.factory().readonly_type(inner)
                }
            }
            TypeData::NoInfer(inner) => {
                let inner = rewrite(db, inner, replacement_for, replacement_for_lazy, active);
                if let Some(TypeData::NoInfer(before)) = db.lookup(type_id)
                    && inner == before
                {
                    type_id
                } else {
                    db.no_infer(inner)
                }
            }
            _ => type_id,
        };
        active.remove(&type_id);
        rewritten
    }

    rewrite(
        db,
        type_id,
        &mut replacement_for,
        &mut replacement_for_lazy,
        &mut rustc_hash::FxHashSet::default(),
    )
}
