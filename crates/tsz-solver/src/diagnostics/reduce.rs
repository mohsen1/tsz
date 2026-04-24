//! Deep reduction of meta-type applications for diagnostic display.
//!
//! This module provides a single public entry point used by the checker's
//! heritage-display boundary: [`deep_reduce_for_display`].
//!
//! Motivation: `tsc` applies `getReducedType` while building the display name
//! for a heritage instance type, so conditional utility applications such as
//! `InstanceType<typeof Foo>` render in their concrete form (`Foo`) even when
//! they appear nested inside an intersection or object member.
//!
//! `tsz`'s generic `TypeEvaluator` only evaluates the top-level node — it
//! stops at `Intersection` / `Union` / `Object` wrappers. This walker
//! descends into those composites and evaluates the leaves (`Application`,
//! `Conditional`) using the caller's `TypeResolver`. Concrete sub-structures
//! are preserved verbatim; only meta-typed leaves that fully reduce to a
//! non-meta type are replaced.
//!
//! The walker is *display-only*: it never widens, normalises, or re-orders
//! properties, and it is safe to call from diagnostic paths.

use rustc_hash::FxHashSet;

use crate::TypeDatabase;
use crate::TypeResolver;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::types::{PropertyInfo, TypeData, TypeId};

/// Deeply reduce meta-type applications inside `type_id` using `resolver`.
///
/// Returns a `TypeId` with the same structural shape as `type_id`, except
/// that any nested `Application` or `Conditional` that fully evaluates to a
/// non-meta type via `evaluator.evaluate(...)` is replaced with the reduced
/// form. If no leaf reduces, `type_id` is returned unchanged.
///
/// This is intended for diagnostic rendering paths where `tsc` applies
/// `getReducedType`. Callers must pass a resolver that can follow
/// `TypeData::Lazy` references (typically the checker's `TypeEnvironment`);
/// a `NoopResolver` would be a no-op.
pub fn deep_reduce_for_display<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut visited = FxHashSet::default();
    let mut evaluator = TypeEvaluator::with_resolver(db, resolver);
    reduce_inner(db, &mut evaluator, type_id, &mut visited)
}

fn reduce_inner<R: TypeResolver>(
    db: &dyn TypeDatabase,
    evaluator: &mut TypeEvaluator<'_, R>,
    type_id: TypeId,
    visited: &mut FxHashSet<TypeId>,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if !visited.insert(type_id) {
        return type_id;
    }

    let key = db.lookup(type_id);
    let result = match key {
        Some(TypeData::Application(_) | TypeData::Conditional(_)) => {
            let reduced = evaluator.evaluate(type_id);
            if reduced == TypeId::ERROR || reduced == type_id {
                type_id
            } else {
                match db.lookup(reduced) {
                    Some(
                        TypeData::Application(_) | TypeData::Conditional(_) | TypeData::Lazy(_),
                    ) => type_id,
                    _ => reduce_inner(db, evaluator, reduced, visited),
                }
            }
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mut changed = false;
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| {
                    let r = reduce_inner(db, evaluator, m, visited);
                    if r != m {
                        changed = true;
                    }
                    r
                })
                .collect();
            if changed {
                crate::intern::type_factory::TypeFactory::new(db).intersection(new_members)
            } else {
                type_id
            }
        }
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mut changed = false;
            let new_members: Vec<TypeId> = members
                .iter()
                .map(|&m| {
                    let r = reduce_inner(db, evaluator, m, visited);
                    if r != m {
                        changed = true;
                    }
                    r
                })
                .collect();
            if changed {
                crate::intern::type_factory::TypeFactory::new(db).union(new_members)
            } else {
                type_id
            }
        }
        Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            let mut new_props: Vec<PropertyInfo> = Vec::with_capacity(shape.properties.len());
            let mut changed = false;
            for prop in shape.properties.iter() {
                let new_read = reduce_inner(db, evaluator, prop.type_id, visited);
                let new_write = if prop.write_type == prop.type_id {
                    new_read
                } else {
                    reduce_inner(db, evaluator, prop.write_type, visited)
                };
                if new_read != prop.type_id || new_write != prop.write_type {
                    changed = true;
                    let mut updated = prop.clone();
                    updated.type_id = new_read;
                    updated.write_type = new_write;
                    new_props.push(updated);
                } else {
                    new_props.push(prop.clone());
                }
            }
            if !changed {
                type_id
            } else if matches!(key, Some(TypeData::ObjectWithIndex(_))) {
                let mut new_shape = (*shape).clone();
                new_shape.properties = new_props;
                crate::intern::type_factory::TypeFactory::new(db).object_with_index(new_shape)
            } else {
                crate::intern::type_factory::TypeFactory::new(db).object(new_props)
            }
        }
        _ => type_id,
    };
    visited.remove(&type_id);
    result
}
