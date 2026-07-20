//! Stage-1′ Slice-1 RESTRICTED alpha instantiation-cache canon (task #87),
//! gated on `TSZ_ALPHA_SCOPED_KEY` (default-OFF; byte-parity when OFF — the
//! deployed #13406 canon in `api.rs` is untouched).
//!
//! # What this extends and why it is sound
//!
//! The deployed canon renames only BARE, unconstrained free type parameters
//! (keyed by name) and BAILS the moment a substitution arg contains a
//! `Function`/`Callable`/`Mapped`/`Infer`/`Application`/… That bail set is a
//! genuine SOUNDNESS FIREWALL, not a coverage gap: tsz interns a shape's own
//! bound param and a structurally-identical FREE param to ONE `TypeId` (the
//! deduped `type_param` constructor), so a free param captured inside a binder
//! shape is indistinguishable from that shape's own bound var — no name/TypeId
//! scope can canonicalize+restore it soundly (the task #87 capture finding).
//!
//! This module lifts the firewall for exactly the capture-FREE cases and no
//! others:
//! - recurse into `Application` and the composites the deployed canon already
//!   handles (never a binder shape → capture is impossible);
//! - rename EVERY free `TypeParameter` (bare, constrained, defaulted, const),
//!   keyed on its own `TypeId`. R1 is then automatic — two params with
//!   structurally different constraints are different `TypeParamInfo`s, hence
//!   different `TypeId`s, hence different markers;
//! - BAIL (`None`) on any `Function`/`Callable`/`Mapped`/`Infer`/`Enum`/
//!   `TemplateLiteral`/`StringIntrinsic`/`BoundParameter`. A bail on the RESULT
//!   canon is the capture firewall: a captured result never gets stored, so no
//!   restore can leak one caller's param to another (an unhandled variant is a
//!   perf miss, never a correctness bug).
//!
//! The three functions stay in LOCKSTEP with `bindings` as the contract:
//! `bindings[i]` is the original free-param `TypeId` renamed to
//! `BoundParameter(i)`. `canon` erases free params to markers; `restore` maps
//! markers back to the caller's own params and is provably unable to touch a
//! bound var — bound vars live only inside binder shapes, which `canon` bailed
//! on, so a stored alpha result contains `BoundParameter` ONLY as markers.
//!
//! Priced at 43.22×/225.74× sound instantiation/app-eval key collapse on
//! zustand (bail-on-binder 74/49,489 = 0.15%), essentially the full scope-aware
//! payoff without the capture-hard binder recursion.

use crate::caches::db::TypeDatabase;
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::instantiation::request::InstantiationRequest;
use crate::types::{
    ConditionalType, IndexSignature, ObjectShape, PropertyInfo, TupleElement, TypeData, TypeId,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tsz_common::interner::Atom;

/// R3 restore bound: a stored alpha result is a pure function of the same
/// instantiation the caller would otherwise recompute, so restoring it visits
/// O(result size) nodes — comparable to recompute. This cap bounds the
/// pathological case: past it, `restore` returns `None` and the caller falls
/// through to the authoritative recompute, so restore can never exceed it.
const MAX_RESTORE_NODES: u32 = 1 << 20;

/// Restricted alpha instantiation-cache key. Mirrors the deployed
/// `alpha_instantiation_cache_key` shape (returns `key` + `bindings`) but uses
/// the restricted `canon`. `None` = the deployed wholesale decline
/// (`mode_bits`/`this_type`) or a bail, so the caller keeps its raw key.
pub(super) fn restricted_instantiation_cache_key(
    interner: &dyn TypeDatabase,
    request: InstantiationRequest<'_>,
) -> Option<(InstantiationCacheKey, SmallVec<[TypeId; 4]>)> {
    if request.options().mode_bits() != 0 || request.this_type().is_some() {
        return None;
    }
    let mut binders: FxHashMap<TypeId, u32> = FxHashMap::default();
    let mut bindings = SmallVec::<[TypeId; 4]>::new();
    let mut changed = false;
    let mut alpha_pairs = SmallVec::<[(Atom, TypeId); 4]>::new();
    for (name, type_id) in request.substitution().canonical_pairs() {
        let mut visited = FxHashSet::default();
        let alpha_type = canon(
            interner,
            type_id,
            &mut binders,
            &mut bindings,
            &mut changed,
            &mut visited,
        )?;
        alpha_pairs.push((name, alpha_type));
    }
    changed.then(|| {
        (
            InstantiationCacheKey::new(
                request.type_id(),
                CanonicalSubst::from_pairs(alpha_pairs),
                request.options().mode_bits(),
                request.this_type(),
            ),
            bindings,
        )
    })
}

/// Canonicalize a computed instantiation result into its alpha form for storage
/// under the alpha key: the caller's free params (recorded in `bindings`,
/// keyed by `TypeId`) become their markers. `None` = a bail (e.g. the result
/// contains a binder shape — the capture firewall), so no alpha result is
/// stored.
pub(super) fn restricted_canonicalize_cached_result(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
) -> Option<TypeId> {
    let mut binders: FxHashMap<TypeId, u32> = FxHashMap::default();
    for (index, &binding) in bindings.iter().enumerate() {
        binders.insert(binding, index as u32);
    }
    let mut alpha_bindings: SmallVec<[TypeId; 4]> = bindings.iter().copied().collect();
    let mut changed = false;
    let mut visited = FxHashSet::default();
    canon(
        interner,
        type_id,
        &mut binders,
        &mut alpha_bindings,
        &mut changed,
        &mut visited,
    )
}

/// Restore a stored alpha-form result to the caller's own params: each
/// `BoundParameter(i)` marker maps to `bindings[i]`. `None` on any bail (an
/// out-of-range marker, a binder shape that should never appear, or the R3 node
/// bound), so the caller recomputes.
pub(super) fn restricted_restore_alpha_result(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
) -> Option<TypeId> {
    let mut visited = FxHashSet::default();
    let mut budget = MAX_RESTORE_NODES;
    restore(interner, type_id, bindings, &mut visited, &mut budget)
}

/// The restricted canon walk. Renames every free `TypeParameter` to a
/// `TypeId`-keyed De-Bruijn marker, recurses `Application` + composites, and
/// bails on any binder-introducing or un-canonicalizable variant.
fn canon(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    binders: &mut FxHashMap<TypeId, u32>,
    bindings: &mut SmallVec<[TypeId; 4]>,
    changed: &mut bool,
    visited: &mut FxHashSet<TypeId>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return Some(type_id);
    }
    let key = interner.lookup(type_id)?;
    match key {
        // Every free param is renamed, keyed on its own TypeId (R1 automatic).
        TypeData::TypeParameter(_) => {
            let index = if let Some(index) = binders.get(&type_id).copied() {
                index
            } else {
                let index = bindings.len() as u32;
                binders.insert(type_id, index);
                bindings.push(type_id);
                index
            };
            *changed = true;
            Some(interner.bound_parameter(index))
        }
        // BAIL set: binder-introducing shapes (the capture firewall) plus the
        // variants the deployed canon also cannot canonicalize. Placed before
        // the cycle guard so a bail is never masked by a revisit.
        TypeData::BoundParameter(_)
        | TypeData::Infer(_)
        | TypeData::Enum(_, _)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Mapped(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::StringIntrinsic { .. } => None,
        // Cycle/DAG-reconvergence guard: a revisited composite returns raw
        // (un-canonicalized). Sound — an un-renamed free param keeps two
        // callers' keys distinct, so no false collapse and no leak (the same
        // property the deployed canon relies on).
        _ if !visited.insert(type_id) => Some(type_id),
        // Application: the one extension over the deployed canon — recurse the
        // base and args under no binder scope. This is the capture-free
        // headroom (free params embedded in `StateCreator<T, …>` etc.).
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            let base = canon(interner, app.base, binders, bindings, changed, visited)?;
            let mut local_changed = base != app.base;
            let mut args = Vec::with_capacity(app.args.len());
            for &arg in &app.args {
                let next = canon(interner, arg, binders, bindings, changed, visited)?;
                local_changed |= next != arg;
                args.push(next);
            }
            Some(if local_changed {
                interner.application(base, args)
            } else {
                type_id
            })
        }
        TypeData::Array(element) => {
            let next = canon(interner, element, binders, bindings, changed, visited)?;
            Some(if next == element {
                type_id
            } else {
                interner.array(next)
            })
        }
        TypeData::ReadonlyType(inner) => {
            let next = canon(interner, inner, binders, bindings, changed, visited)?;
            Some(if next == inner {
                type_id
            } else {
                interner.readonly_type(next)
            })
        }
        TypeData::NoInfer(inner) => {
            let next = canon(interner, inner, binders, bindings, changed, visited)?;
            Some(if next == inner {
                type_id
            } else {
                interner.no_infer(next)
            })
        }
        TypeData::Substitution {
            base_type,
            constraint,
        } => {
            let next_base = canon(interner, base_type, binders, bindings, changed, visited)?;
            let next_constraint = canon(interner, constraint, binders, bindings, changed, visited)?;
            Some(if next_base == base_type && next_constraint == constraint {
                type_id
            } else {
                interner.substitution(next_base, next_constraint)
            })
        }
        TypeData::Tuple(tuple_id) => {
            let elements = interner.tuple_list(tuple_id);
            let mut local_changed = false;
            let mut next = Vec::with_capacity(elements.len());
            for element in elements.iter() {
                let t = canon(
                    interner,
                    element.type_id,
                    binders,
                    bindings,
                    changed,
                    visited,
                )?;
                local_changed |= t != element.type_id;
                next.push(TupleElement {
                    type_id: t,
                    ..*element
                });
            }
            Some(if local_changed {
                interner.tuple(next)
            } else {
                type_id
            })
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = interner.type_list(list_id);
            let mut local_changed = false;
            let mut next = Vec::with_capacity(members.len());
            for &member in members.iter() {
                let alpha = canon(interner, member, binders, bindings, changed, visited)?;
                local_changed |= alpha != member;
                next.push(alpha);
            }
            Some(if !local_changed {
                type_id
            } else if matches!(key, TypeData::Union(_)) {
                interner.union(next)
            } else {
                interner.intersection(next)
            })
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let mut local_changed = false;
            let mut properties = Vec::with_capacity(shape.properties.len());
            for prop in &shape.properties {
                let read = canon(interner, prop.type_id, binders, bindings, changed, visited)?;
                let write = canon(
                    interner,
                    prop.write_type,
                    binders,
                    bindings,
                    changed,
                    visited,
                )?;
                local_changed |= read != prop.type_id || write != prop.write_type;
                properties.push(PropertyInfo {
                    type_id: read,
                    write_type: write,
                    ..prop.clone()
                });
            }
            let string_index = canon_index_sig(
                interner,
                shape.string_index,
                binders,
                bindings,
                changed,
                visited,
                &mut local_changed,
            )?;
            let number_index = canon_index_sig(
                interner,
                shape.number_index,
                binders,
                bindings,
                changed,
                visited,
                &mut local_changed,
            )?;
            let symbol_index = canon_index_sig(
                interner,
                shape.symbol_index,
                binders,
                bindings,
                changed,
                visited,
                &mut local_changed,
            )?;
            if !local_changed {
                return Some(type_id);
            }
            let shape = ObjectShape {
                flags: shape.flags,
                properties,
                string_index,
                number_index,
                symbol_index,
                symbol: shape.symbol,
            };
            Some(if matches!(key, TypeData::ObjectWithIndex(_)) {
                interner.object_with_index(shape)
            } else {
                interner.object_with_flags_and_symbol(shape.properties, shape.flags, shape.symbol)
            })
        }
        TypeData::IndexAccess(object, index) => {
            let object_next = canon(interner, object, binders, bindings, changed, visited)?;
            let index_next = canon(interner, index, binders, bindings, changed, visited)?;
            Some(if object_next == object && index_next == index {
                type_id
            } else {
                interner.index_access(object_next, index_next)
            })
        }
        TypeData::KeyOf(operand) => {
            let next = canon(interner, operand, binders, bindings, changed, visited)?;
            Some(if next == operand {
                type_id
            } else {
                interner.keyof(next)
            })
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            let check_type = canon(
                interner,
                cond.check_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let extends_type = canon(
                interner,
                cond.extends_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let true_type = canon(
                interner,
                cond.true_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let false_type = canon(
                interner,
                cond.false_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            Some(
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    type_id
                } else {
                    interner.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                },
            )
        }
        // Leaves returned unchanged (no free param to rename inside them).
        TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::TypeQuery(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::ModuleNamespace(_) => Some(type_id),
    }
}

#[allow(clippy::too_many_arguments)]
fn canon_index_sig(
    interner: &dyn TypeDatabase,
    signature: Option<IndexSignature>,
    binders: &mut FxHashMap<TypeId, u32>,
    bindings: &mut SmallVec<[TypeId; 4]>,
    changed: &mut bool,
    visited: &mut FxHashSet<TypeId>,
    local_changed: &mut bool,
) -> Option<Option<IndexSignature>> {
    let Some(signature) = signature else {
        return Some(None);
    };
    let key_type = canon(
        interner,
        signature.key_type,
        binders,
        bindings,
        changed,
        visited,
    )?;
    let value_type = canon(
        interner,
        signature.value_type,
        binders,
        bindings,
        changed,
        visited,
    )?;
    *local_changed |= key_type != signature.key_type || value_type != signature.value_type;
    Some(Some(IndexSignature {
        key_type,
        value_type,
        ..signature
    }))
}

/// The restricted restore walk — the inverse of [`canon`] restricted to the
/// markers. Maps `BoundParameter(i)` → `bindings[i]`, recurses `Application` +
/// composites, and bails on anything a stored alpha result should never contain
/// or on the R3 node bound.
fn restore(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
    visited: &mut FxHashSet<TypeId>,
    budget: &mut u32,
) -> Option<TypeId> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    if type_id.is_intrinsic() {
        return Some(type_id);
    }
    let key = interner.lookup(type_id)?;
    match key {
        TypeData::BoundParameter(index) => bindings.get(index as usize).copied(),
        // A stored alpha result never contains these (canon bailed on them);
        // bail defensively so a marker can never be left unrestored inside one.
        TypeData::Infer(_)
        | TypeData::Enum(_, _)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Mapped(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::StringIntrinsic { .. } => None,
        _ if !visited.insert(type_id) => Some(type_id),
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            let base = restore(interner, app.base, bindings, visited, budget)?;
            let mut changed = base != app.base;
            let mut args = Vec::with_capacity(app.args.len());
            for &arg in &app.args {
                let next = restore(interner, arg, bindings, visited, budget)?;
                changed |= next != arg;
                args.push(next);
            }
            Some(if changed {
                interner.application(base, args)
            } else {
                type_id
            })
        }
        TypeData::Array(element) => {
            let next = restore(interner, element, bindings, visited, budget)?;
            Some(if next == element {
                type_id
            } else {
                interner.array(next)
            })
        }
        TypeData::ReadonlyType(inner) => {
            let next = restore(interner, inner, bindings, visited, budget)?;
            Some(if next == inner {
                type_id
            } else {
                interner.readonly_type(next)
            })
        }
        TypeData::NoInfer(inner) => {
            let next = restore(interner, inner, bindings, visited, budget)?;
            Some(if next == inner {
                type_id
            } else {
                interner.no_infer(next)
            })
        }
        TypeData::Substitution {
            base_type,
            constraint,
        } => {
            let next_base = restore(interner, base_type, bindings, visited, budget)?;
            let next_constraint = restore(interner, constraint, bindings, visited, budget)?;
            Some(if next_base == base_type && next_constraint == constraint {
                type_id
            } else {
                interner.substitution(next_base, next_constraint)
            })
        }
        TypeData::Tuple(tuple_id) => {
            let elements = interner.tuple_list(tuple_id);
            let mut changed = false;
            let mut next = Vec::with_capacity(elements.len());
            for element in elements.iter() {
                let restored = restore(interner, element.type_id, bindings, visited, budget)?;
                changed |= restored != element.type_id;
                next.push(TupleElement {
                    type_id: restored,
                    ..*element
                });
            }
            Some(if changed {
                interner.tuple(next)
            } else {
                type_id
            })
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = interner.type_list(list_id);
            let mut changed = false;
            let mut next = Vec::with_capacity(members.len());
            for &member in members.iter() {
                let restored = restore(interner, member, bindings, visited, budget)?;
                changed |= restored != member;
                next.push(restored);
            }
            Some(if !changed {
                type_id
            } else if matches!(key, TypeData::Union(_)) {
                interner.union(next)
            } else {
                interner.intersection(next)
            })
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let mut changed = false;
            let mut properties = Vec::with_capacity(shape.properties.len());
            for prop in &shape.properties {
                let read = restore(interner, prop.type_id, bindings, visited, budget)?;
                let write = restore(interner, prop.write_type, bindings, visited, budget)?;
                changed |= read != prop.type_id || write != prop.write_type;
                properties.push(PropertyInfo {
                    type_id: read,
                    write_type: write,
                    ..prop.clone()
                });
            }
            let string_index = restore_index_sig(
                interner,
                shape.string_index,
                bindings,
                visited,
                &mut changed,
                budget,
            )?;
            let number_index = restore_index_sig(
                interner,
                shape.number_index,
                bindings,
                visited,
                &mut changed,
                budget,
            )?;
            let symbol_index = restore_index_sig(
                interner,
                shape.symbol_index,
                bindings,
                visited,
                &mut changed,
                budget,
            )?;
            if !changed {
                return Some(type_id);
            }
            let shape = ObjectShape {
                flags: shape.flags,
                properties,
                string_index,
                number_index,
                symbol_index,
                symbol: shape.symbol,
            };
            Some(if matches!(key, TypeData::ObjectWithIndex(_)) {
                interner.object_with_index(shape)
            } else {
                interner.object_with_flags_and_symbol(shape.properties, shape.flags, shape.symbol)
            })
        }
        TypeData::IndexAccess(object, index) => {
            let object_next = restore(interner, object, bindings, visited, budget)?;
            let index_next = restore(interner, index, bindings, visited, budget)?;
            Some(if object_next == object && index_next == index {
                type_id
            } else {
                interner.index_access(object_next, index_next)
            })
        }
        TypeData::KeyOf(operand) => {
            let next = restore(interner, operand, bindings, visited, budget)?;
            Some(if next == operand {
                type_id
            } else {
                interner.keyof(next)
            })
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            let check_type = restore(interner, cond.check_type, bindings, visited, budget)?;
            let extends_type = restore(interner, cond.extends_type, bindings, visited, budget)?;
            let true_type = restore(interner, cond.true_type, bindings, visited, budget)?;
            let false_type = restore(interner, cond.false_type, bindings, visited, budget)?;
            Some(
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    type_id
                } else {
                    interner.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                },
            )
        }
        // Leaves (including a free `TypeParameter` left raw at a revisit path)
        // are returned unchanged; restore only rewrites markers.
        _ => Some(type_id),
    }
}

fn restore_index_sig(
    interner: &dyn TypeDatabase,
    signature: Option<IndexSignature>,
    bindings: &[TypeId],
    visited: &mut FxHashSet<TypeId>,
    changed: &mut bool,
    budget: &mut u32,
) -> Option<Option<IndexSignature>> {
    let Some(signature) = signature else {
        return Some(None);
    };
    let key_type = restore(interner, signature.key_type, bindings, visited, budget)?;
    let value_type = restore(interner, signature.value_type, bindings, visited, budget)?;
    *changed |= key_type != signature.key_type || value_type != signature.value_type;
    Some(Some(IndexSignature {
        key_type,
        value_type,
        ..signature
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::types::{TypeParamInfo, TypeParamOrigin};

    fn tp(interner: &TypeInterner, name: &str, constraint: Option<TypeId>) -> TypeId {
        interner.type_param(TypeParamInfo {
            name: interner.intern_string(name),
            constraint,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        })
    }

    /// R1: two free params whose constraints differ structurally must NEVER
    /// share a marker. TypeId keying delivers this — different constraint =>
    /// different `TypeParamInfo` => different `TypeId` => different marker.
    #[test]
    fn r1_different_constraints_never_share_a_marker() {
        let interner = TypeInterner::new();
        let t_a = tp(&interner, "T", Some(TypeId::NUMBER));
        let t_b = tp(&interner, "T", Some(TypeId::STRING)); // same name, different constraint
        assert_ne!(t_a, t_b, "distinct constraints must intern distinctly");
        // Arg = [T_a, T_b] as a tuple; canon must produce two distinct markers.
        let arg = interner.tuple(vec![
            TupleElement {
                type_id: t_a,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: t_b,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let mut binders = FxHashMap::default();
        let mut bindings = SmallVec::<[TypeId; 4]>::new();
        let mut changed = false;
        let mut visited = FxHashSet::default();
        let alpha = canon(
            &interner,
            arg,
            &mut binders,
            &mut bindings,
            &mut changed,
            &mut visited,
        )
        .expect("tuple of two free params canonicalizes");
        assert!(changed);
        assert_eq!(bindings.len(), 2, "two distinct params => two bindings");
        assert_eq!(bindings[0], t_a);
        assert_eq!(bindings[1], t_b);
        // The two markers are BoundParameter(0) and BoundParameter(1): distinct.
        let TypeData::Tuple(list) = interner.lookup(alpha).unwrap() else {
            panic!("expected tuple");
        };
        let elems = interner.tuple_list(list);
        assert_ne!(
            elems[0].type_id, elems[1].type_id,
            "R1: different-constraint params must map to different markers"
        );
    }

    /// Two references to the SAME free param collapse to one marker, and the
    /// canon→restore round-trip reinstates a second caller's own param.
    #[test]
    fn same_param_collapses_and_round_trips() {
        let interner = TypeInterner::new();
        let t = tp(&interner, "T", None);
        // Arg = { a: T, b: T } — one param twice.
        let arg = interner.object(vec![
            PropertyInfo::new(interner.intern_string("a"), t),
            PropertyInfo::new(interner.intern_string("b"), t),
        ]);
        let mut binders = FxHashMap::default();
        let mut bindings = SmallVec::<[TypeId; 4]>::new();
        let mut changed = false;
        let mut visited = FxHashSet::default();
        let alpha = canon(
            &interner,
            arg,
            &mut binders,
            &mut bindings,
            &mut changed,
            &mut visited,
        )
        .expect("object canonicalizes");
        assert_eq!(bindings.len(), 1, "one distinct param => one marker");
        // Restore with a DIFFERENT caller's param U: alpha[T:=marker0] then
        // restore[marker0:=U] must equal the same object over U.
        let u = tp(&interner, "U", None);
        let restored = restricted_restore_alpha_result(&interner, alpha, &[u])
            .expect("restore maps the marker");
        let expected = interner.object(vec![
            PropertyInfo::new(interner.intern_string("a"), u),
            PropertyInfo::new(interner.intern_string("b"), u),
        ]);
        assert_eq!(
            restored, expected,
            "restore reinstates caller-2's own param"
        );
    }

    /// A binder shape in the arg bails the whole key (`None`) — the capture
    /// firewall. Renaming a bound var could never be soundly restored, so we
    /// never produce a key for it.
    #[test]
    fn binder_shape_bails() {
        use crate::types::FunctionShape;
        let interner = TypeInterner::new();
        let t = tp(&interner, "T", None);
        // A function `(x: T) => T` — a binder shape carrying a free param.
        let func = interner.function(FunctionShape::new(
            vec![crate::types::ParamInfo::required(
                interner.intern_string("x"),
                t,
            )],
            t,
        ));
        let mut binders = FxHashMap::default();
        let mut bindings = SmallVec::<[TypeId; 4]>::new();
        let mut changed = false;
        let mut visited = FxHashSet::default();
        assert_eq!(
            canon(
                &interner,
                func,
                &mut binders,
                &mut bindings,
                &mut changed,
                &mut visited
            ),
            None,
            "a binder shape must bail (capture firewall)"
        );
    }

    /// Restore leaves a raw `TypeParameter` (a bound var that reached a revisit
    /// path, or any non-marker) untouched — it only rewrites `BoundParameter`
    /// markers. Guards the "provably unable to touch bound vars" invariant.
    #[test]
    fn restore_leaves_non_markers_untouched() {
        let interner = TypeInterner::new();
        let t = tp(&interner, "T", None);
        let arr = interner.array(t);
        // No markers present: restore is the identity.
        let restored = restricted_restore_alpha_result(&interner, arr, &[]).unwrap();
        assert_eq!(restored, arr);
    }
}
