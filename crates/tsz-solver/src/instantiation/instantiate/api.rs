use super::*;
use crate::caches::db::QueryDatabase;
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::instantiation::instantiate::cache_stability::ProjectInstantiationCacheLimitSnapshot;
use crate::instantiation::request::{InstantiationOptions, InstantiationRequest};
use crate::instantiation::result::{InstantiationMemoResult, InstantiationResult};
use crate::types::{ConditionalType, FunctionShape, PropertyInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::cell::RefCell;

thread_local! {
    static CONSTRAINT_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> =
        const { RefCell::new(None) };
}

#[inline]
fn with_constraint_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = CONSTRAINT_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    CONSTRAINT_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

#[inline]
fn can_skip_concrete_instantiation(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut visited = FxHashSet::default();
    can_skip_concrete_instantiation_inner(interner, type_id, &mut visited)
}

fn can_skip_concrete_instantiation_inner(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    visited: &mut FxHashSet<TypeId>,
) -> bool {
    if type_id.is_intrinsic() {
        return true;
    }
    if !visited.insert(type_id) {
        return true;
    }

    let Some(key) = interner.lookup(type_id) else {
        return true;
    };

    match key {
        TypeData::TypeParameter(_)
        | TypeData::Infer(_)
        | TypeData::Substitution { .. }
        | TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::StringIntrinsic { .. } => false,
        TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::BoundParameter(_)
        | TypeData::TypeQuery(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ModuleNamespace(_)
        | TypeData::ThisType => true,
        TypeData::Enum(_, member_type)
        | TypeData::Array(member_type)
        | TypeData::ReadonlyType(member_type)
        | TypeData::NoInfer(member_type) => {
            can_skip_concrete_instantiation_inner(interner, member_type, visited)
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => interner
            .type_list(list_id)
            .iter()
            .copied()
            .all(|member| can_skip_concrete_instantiation_inner(interner, member, visited)),
        TypeData::Tuple(tuple_id) => interner
            .tuple_list(tuple_id)
            .iter()
            .all(|elem| can_skip_concrete_instantiation_inner(interner, elem.type_id, visited)),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            shape.properties.iter().all(|prop| {
                can_skip_concrete_instantiation_inner(interner, prop.type_id, visited)
                    && can_skip_concrete_instantiation_inner(interner, prop.write_type, visited)
            }) && shape.string_index.as_ref().is_none_or(|idx| {
                can_skip_concrete_instantiation_inner(interner, idx.key_type, visited)
                    && can_skip_concrete_instantiation_inner(interner, idx.value_type, visited)
            }) && shape.number_index.as_ref().is_none_or(|idx| {
                can_skip_concrete_instantiation_inner(interner, idx.key_type, visited)
                    && can_skip_concrete_instantiation_inner(interner, idx.value_type, visited)
            })
        }
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            shape.type_params.is_empty()
                && shape.params.iter().all(|param| {
                    can_skip_concrete_instantiation_inner(interner, param.type_id, visited)
                })
                && shape.this_type.is_none_or(|this_type| {
                    can_skip_concrete_instantiation_inner(interner, this_type, visited)
                })
                && can_skip_concrete_instantiation_inner(interner, shape.return_type, visited)
                && shape.type_predicate.is_none_or(|predicate| {
                    predicate.type_id.is_none_or(|predicate_type| {
                        can_skip_concrete_instantiation_inner(interner, predicate_type, visited)
                    })
                })
        }
        TypeData::Callable(shape_id) => {
            let shape = interner.callable_shape(shape_id);
            let signatures_are_identity = shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .all(|sig| {
                    sig.type_params.is_empty()
                        && sig.params.iter().all(|param| {
                            can_skip_concrete_instantiation_inner(interner, param.type_id, visited)
                        })
                        && sig.this_type.is_none_or(|this_type| {
                            can_skip_concrete_instantiation_inner(interner, this_type, visited)
                        })
                        && can_skip_concrete_instantiation_inner(interner, sig.return_type, visited)
                        && sig.type_predicate.is_none_or(|predicate| {
                            predicate.type_id.is_none_or(|predicate_type| {
                                can_skip_concrete_instantiation_inner(
                                    interner,
                                    predicate_type,
                                    visited,
                                )
                            })
                        })
                });
            signatures_are_identity
                && shape.properties.iter().all(|prop| {
                    can_skip_concrete_instantiation_inner(interner, prop.type_id, visited)
                        && can_skip_concrete_instantiation_inner(interner, prop.write_type, visited)
                })
                && shape.string_index.as_ref().is_none_or(|idx| {
                    can_skip_concrete_instantiation_inner(interner, idx.key_type, visited)
                        && can_skip_concrete_instantiation_inner(interner, idx.value_type, visited)
                })
                && shape.number_index.as_ref().is_none_or(|idx| {
                    can_skip_concrete_instantiation_inner(interner, idx.key_type, visited)
                        && can_skip_concrete_instantiation_inner(interner, idx.value_type, visited)
                })
        }
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            can_skip_concrete_instantiation_inner(interner, app.base, visited)
                && app
                    .args
                    .iter()
                    .copied()
                    .all(|arg| can_skip_concrete_instantiation_inner(interner, arg, visited))
        }
    }
}

/// Shared body for the option-only wrappers
/// (`instantiate_type_preserving_cached`, `instantiate_type_preserving_meta_cached`,
/// `instantiate_type_with_infer_cached`).
///
/// All three apply the same "intrinsic check → empty/concrete identity
/// short-circuit → delegate to engine" prelude; the only thing that varies is
/// the option set passed to the instantiator. `instantiate_type_cached` does
/// NOT share this helper because it has additional allocation-free leaf fast
/// paths (`TypeParameter`, `IndexAccess(T, P)`) that must precede any cache-key
/// construction. The `substitute_this_*` variants also bypass this helper
/// because they intentionally skip the empty-subst short-circuit (their cache
/// key is keyed on `this_type`, not the substitution map).
#[inline]
fn instantiate_with_options_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    options: InstantiationOptions,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if substitution.is_empty() {
        return type_id;
    }
    instantiate_with_request_cached(
        interner,
        query_db,
        false,
        InstantiationRequest::new(type_id, substitution).with_options(options),
    )
    .into_type_id()
}

struct AlphaInstantiationCacheKey {
    key: InstantiationCacheKey,
    bindings: SmallVec<[TypeId; 4]>,
}

fn alpha_instantiation_cache_key(
    interner: &dyn TypeDatabase,
    request: InstantiationRequest<'_>,
) -> Option<AlphaInstantiationCacheKey> {
    if request.options().mode_bits() != 0 || request.this_type().is_some() {
        return None;
    }

    let mut binders = FxHashMap::default();
    let mut bindings = SmallVec::<[TypeId; 4]>::new();
    let mut changed = false;
    let mut alpha_pairs = SmallVec::<[(Atom, TypeId); 4]>::new();

    for (name, type_id) in request.substitution().canonical_pairs() {
        let mut visited = FxHashSet::default();
        let alpha_type = alpha_canonicalize_type(
            interner,
            type_id,
            &mut binders,
            &mut bindings,
            &mut changed,
            &mut visited,
        )?;
        alpha_pairs.push((name, alpha_type));
    }

    changed.then(|| AlphaInstantiationCacheKey {
        key: InstantiationCacheKey::new(
            request.type_id(),
            CanonicalSubst::from_pairs(alpha_pairs),
            request.options().mode_bits(),
            request.this_type(),
        )
        .with_identity_domain(request.substitution().identity_domain_for_cache()),
        bindings,
    })
}

fn alpha_canonicalize_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    binders: &mut FxHashMap<Atom, u32>,
    bindings: &mut SmallVec<[TypeId; 4]>,
    changed: &mut bool,
    visited: &mut FxHashSet<TypeId>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return Some(type_id);
    }

    let key = interner.lookup(type_id)?;
    match key {
        TypeData::TypeParameter(info)
            if info.constraint.is_none() && info.default.is_none() && !info.is_const =>
        {
            let index = if let Some(index) = binders.get(&info.name).copied() {
                index
            } else {
                let index = bindings.len() as u32;
                binders.insert(info.name, index);
                bindings.push(type_id);
                index
            };
            *changed = true;
            Some(interner.bound_parameter(index))
        }
        TypeData::TypeParameter(_) => Some(type_id),
        TypeData::BoundParameter(_) => None,
        _ if !visited.insert(type_id) => Some(type_id),
        TypeData::Array(element) => {
            let next =
                alpha_canonicalize_type(interner, element, binders, bindings, changed, visited)?;
            Some(if next == element {
                type_id
            } else {
                interner.array(next)
            })
        }
        TypeData::ReadonlyType(inner) => {
            let next =
                alpha_canonicalize_type(interner, inner, binders, bindings, changed, visited)?;
            Some(if next == inner {
                type_id
            } else {
                interner.readonly_type(next)
            })
        }
        TypeData::NoInfer(inner) => {
            let next =
                alpha_canonicalize_type(interner, inner, binders, bindings, changed, visited)?;
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
            let next_base =
                alpha_canonicalize_type(interner, base_type, binders, bindings, changed, visited)?;
            let next_constraint =
                alpha_canonicalize_type(interner, constraint, binders, bindings, changed, visited)?;
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
                let type_id = alpha_canonicalize_type(
                    interner,
                    element.type_id,
                    binders,
                    bindings,
                    changed,
                    visited,
                )?;
                local_changed |= type_id != element.type_id;
                next.push(TupleElement {
                    type_id,
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
                let alpha =
                    alpha_canonicalize_type(interner, member, binders, bindings, changed, visited)?;
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
                let read = alpha_canonicalize_type(
                    interner,
                    prop.type_id,
                    binders,
                    bindings,
                    changed,
                    visited,
                )?;
                let write = alpha_canonicalize_type(
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

            let string_index = alpha_canonicalize_index_signature(
                interner,
                shape.string_index,
                binders,
                bindings,
                changed,
                visited,
                &mut local_changed,
            )?;
            let number_index = alpha_canonicalize_index_signature(
                interner,
                shape.number_index,
                binders,
                bindings,
                changed,
                visited,
                &mut local_changed,
            )?;
            let symbol_index = alpha_canonicalize_index_signature(
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
            let object_next =
                alpha_canonicalize_type(interner, object, binders, bindings, changed, visited)?;
            let index_next =
                alpha_canonicalize_type(interner, index, binders, bindings, changed, visited)?;
            Some(if object_next == object && index_next == index {
                type_id
            } else {
                interner.index_access(object_next, index_next)
            })
        }
        TypeData::KeyOf(operand) => {
            let next =
                alpha_canonicalize_type(interner, operand, binders, bindings, changed, visited)?;
            Some(if next == operand {
                type_id
            } else {
                interner.keyof(next)
            })
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            let check_type = alpha_canonicalize_type(
                interner,
                cond.check_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let extends_type = alpha_canonicalize_type(
                interner,
                cond.extends_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let true_type = alpha_canonicalize_type(
                interner,
                cond.true_type,
                binders,
                bindings,
                changed,
                visited,
            )?;
            let false_type = alpha_canonicalize_type(
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
        TypeData::Infer(_)
        | TypeData::Enum(_, _)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Mapped(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::Application(_) => None,
    }
}

fn alpha_canonicalize_index_signature(
    interner: &dyn TypeDatabase,
    signature: Option<IndexSignature>,
    binders: &mut FxHashMap<Atom, u32>,
    bindings: &mut SmallVec<[TypeId; 4]>,
    changed: &mut bool,
    visited: &mut FxHashSet<TypeId>,
    local_changed: &mut bool,
) -> Option<Option<IndexSignature>> {
    let Some(signature) = signature else {
        return Some(None);
    };
    let key_type = alpha_canonicalize_type(
        interner,
        signature.key_type,
        binders,
        bindings,
        changed,
        visited,
    )?;
    let value_type = alpha_canonicalize_type(
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

fn restore_alpha_result(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
) -> Option<TypeId> {
    let mut visited = FxHashSet::default();
    restore_alpha_type(interner, type_id, bindings, &mut visited)
}

fn restore_alpha_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
    visited: &mut FxHashSet<TypeId>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return Some(type_id);
    }
    let key = interner.lookup(type_id)?;
    match key {
        TypeData::BoundParameter(index) => bindings.get(index as usize).copied(),
        _ if !visited.insert(type_id) => Some(type_id),
        TypeData::Array(element) => {
            let next = restore_alpha_type(interner, element, bindings, visited)?;
            Some(if next == element {
                type_id
            } else {
                interner.array(next)
            })
        }
        TypeData::ReadonlyType(inner) => {
            let next = restore_alpha_type(interner, inner, bindings, visited)?;
            Some(if next == inner {
                type_id
            } else {
                interner.readonly_type(next)
            })
        }
        TypeData::NoInfer(inner) => {
            let next = restore_alpha_type(interner, inner, bindings, visited)?;
            Some(if next == inner {
                type_id
            } else {
                interner.no_infer(next)
            })
        }
        TypeData::Tuple(tuple_id) => {
            let elements = interner.tuple_list(tuple_id);
            let mut changed = false;
            let mut next = Vec::with_capacity(elements.len());
            for element in elements.iter() {
                let restored = restore_alpha_type(interner, element.type_id, bindings, visited)?;
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
                let restored = restore_alpha_type(interner, member, bindings, visited)?;
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
                let read = restore_alpha_type(interner, prop.type_id, bindings, visited)?;
                let write = restore_alpha_type(interner, prop.write_type, bindings, visited)?;
                changed |= read != prop.type_id || write != prop.write_type;
                properties.push(PropertyInfo {
                    type_id: read,
                    write_type: write,
                    ..prop.clone()
                });
            }
            let string_index = restore_alpha_index_signature(
                interner,
                shape.string_index,
                bindings,
                visited,
                &mut changed,
            )?;
            let number_index = restore_alpha_index_signature(
                interner,
                shape.number_index,
                bindings,
                visited,
                &mut changed,
            )?;
            let symbol_index = restore_alpha_index_signature(
                interner,
                shape.symbol_index,
                bindings,
                visited,
                &mut changed,
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
            let object_next = restore_alpha_type(interner, object, bindings, visited)?;
            let index_next = restore_alpha_type(interner, index, bindings, visited)?;
            Some(if object_next == object && index_next == index {
                type_id
            } else {
                interner.index_access(object_next, index_next)
            })
        }
        TypeData::KeyOf(operand) => {
            let next = restore_alpha_type(interner, operand, bindings, visited)?;
            Some(if next == operand {
                type_id
            } else {
                interner.keyof(next)
            })
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            let check_type = restore_alpha_type(interner, cond.check_type, bindings, visited)?;
            let extends_type = restore_alpha_type(interner, cond.extends_type, bindings, visited)?;
            let true_type = restore_alpha_type(interner, cond.true_type, bindings, visited)?;
            let false_type = restore_alpha_type(interner, cond.false_type, bindings, visited)?;
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
        TypeData::Application(_) => None,
        _ => Some(type_id),
    }
}

fn restore_alpha_index_signature(
    interner: &dyn TypeDatabase,
    signature: Option<IndexSignature>,
    bindings: &[TypeId],
    visited: &mut FxHashSet<TypeId>,
    changed: &mut bool,
) -> Option<Option<IndexSignature>> {
    let Some(signature) = signature else {
        return Some(None);
    };
    let key_type = restore_alpha_type(interner, signature.key_type, bindings, visited)?;
    let value_type = restore_alpha_type(interner, signature.value_type, bindings, visited)?;
    *changed |= key_type != signature.key_type || value_type != signature.value_type;
    Some(Some(IndexSignature {
        key_type,
        value_type,
        ..signature
    }))
}

/// Apply the instantiator walk that `request` describes, with optional
/// cross-call caching on `query_db`.
///
/// This is the single staged entry point that the legacy `_cached` wrappers
/// share. It owns:
///
/// - the option-driven instantiator setup (mode flags, `this_type`),
/// - the cache probe / fill against [`InstantiationCacheKey`],
/// - the depth-exceeded collapse into [`InstantiationResult`].
///
/// Callers should preserve their own variant-specific fast paths (intrinsic /
/// empty / identity / leaf shortcuts) before reaching this function so the
/// allocation-free shortcuts in `instantiate_type_cached` keep working.
#[inline]
/// Debug kill-switch for the project-wide instantiation cache (#14345).
/// Set `TSZ_DISABLE_INSTANTIATION_CACHE=1` to bypass both reads and writes,
/// mirroring `TSZ_DISABLE_CLOSED_EVAL_CACHE`. Defaults to enabled; used only to
/// bisect regressions.
fn project_instantiation_cache_enabled() -> bool {
    #[cfg(any(test, debug_assertions))]
    if PROJECT_INST_CACHE_DISABLED_FOR_TEST.with(std::cell::Cell::get) {
        return false;
    }
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSZ_DISABLE_INSTANTIATION_CACHE").is_err())
}

#[cfg(any(test, debug_assertions))]
thread_local! {
    /// Per-thread test override letting the per-file `QueryCache` wiring tests
    /// (in this crate and the checker crate) disable the project-wide cache so
    /// they can assert per-file hit/miss statistics in isolation. Held via
    /// [`ProjectInstCacheDisabledGuard`].
    static PROJECT_INST_CACHE_DISABLED_FOR_TEST: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// RAII guard that disables the project-wide instantiation cache on this thread
/// for the per-file `QueryCache` wiring tests; re-enables on drop. Hold one at
/// the top of a test that asserts per-file instantiation cache statistics.
/// Available in `test`/`debug_assertions` builds so the checker crate's wiring
/// tests can use it too (mirrors `force_enable_perf_counters_for_tests`).
#[cfg(any(test, debug_assertions))]
pub struct ProjectInstCacheDisabledGuard;

#[cfg(any(test, debug_assertions))]
impl ProjectInstCacheDisabledGuard {
    pub fn new() -> Self {
        PROJECT_INST_CACHE_DISABLED_FOR_TEST.with(|d| d.set(true));
        Self
    }
}

#[cfg(any(test, debug_assertions))]
impl Default for ProjectInstCacheDisabledGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, debug_assertions))]
impl Drop for ProjectInstCacheDisabledGuard {
    fn drop(&mut self) {
        PROJECT_INST_CACHE_DISABLED_FOR_TEST.with(|d| d.set(false));
    }
}

pub(crate) fn instantiate_with_request_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    allow_alpha_cache: bool,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    let resolver_rereduce_request = resolver_rereduce_instantiation_request(query_db);
    // Project-wide instantiation cache (#14345), consulted by ALL callers —
    // including the `query_db=None` evaluators that bypass the per-file
    // `QueryCache` instantiation cache and otherwise re-mint the same
    // `(body, subst, options, this_type)` walk. Sound because instantiation is
    // pure structural substitution in the default resolver-less mode:
    // `Lazy`/`TypeQuery` are leaves, and conditional/mapped bodies are not
    // evaluated during the walk, so the result is a pure function of the key and
    // the immutable interner — query_db-independent (proven by
    // `instantiate_generic_cached_no_query_db_disables_cache`). The staged
    // resolver re-reduce mode deliberately breaks that purity by reading a
    // caller-attached `DefinitionStore`, so it bypasses both project-wide and
    // per-query instantiation caches. The store-gate below refuses any result
    // produced under a limit, mirroring the substitution-independent
    // `closed_eval_cache`'s limit-gate set.
    let proto_key = if !resolver_rereduce_request && project_instantiation_cache_enabled() {
        let key = request.cache_key();
        if let Some(cached) = interner.lookup_proto_instantiation_cache(&key) {
            return InstantiationResult::ok(cached);
        }
        Some(key)
    } else {
        None
    };
    if let Some(proto_key) = proto_key {
        // COMPLETE limit gate (mirrors closed_eval_cache's full predicate set,
        // adapted to the instantiation layer). A result produced under ANY
        // sticky, diagnostic-producing limit must NOT be cached, or a later
        // cache hit would short-circuit that diagnostic on re-instantiation (the
        // #13889-class trap one layer down). Snapshot the sticky flags BEFORE so
        // a newly-tripped flag is attributed to THIS instantiation, not an
        // earlier sibling that already set it.
        let limit_snapshot = ProjectInstantiationCacheLimitSnapshot::capture(interner);
        let result =
            instantiate_with_request_cached_inner(interner, query_db, allow_alpha_cache, request);
        let request_state_stability = limit_snapshot.request_state_stability_after(interner);
        let memo_result = InstantiationMemoResult::for_project_cache(
            result,
            request_state_stability.is_stable_for_project_cache(),
        );
        if memo_result.is_stable_for_project_cache() {
            interner
                .insert_proto_instantiation_cache(proto_key, memo_result.into_result().type_id());
        }
        return result;
    }
    instantiate_with_request_cached_inner(interner, query_db, allow_alpha_cache, request)
}

fn instantiate_with_request_cached_inner(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    allow_alpha_cache: bool,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    if let Some(db) = query_db {
        if resolver_rereduce_instantiation_request(query_db) {
            return run_instantiator(interner, query_db, request);
        }
        let key = request.cache_key();
        if let Some(cached) = db.lookup_instantiation_cache(&key) {
            return InstantiationResult::ok(cached);
        }
        let alpha_key = if allow_alpha_cache {
            alpha_instantiation_cache_key(interner, request)
        } else {
            None
        };
        if let Some(alpha_key) = &alpha_key
            && alpha_key.key != key
            && let Some(cached) = db.lookup_instantiation_cache(&alpha_key.key)
            && let Some(restored) = restore_alpha_result(interner, cached, &alpha_key.bindings)
        {
            db.insert_instantiation_cache_with_project_stability(key, restored, false);
            return InstantiationResult::ok(restored);
        }
        let limit_snapshot = ProjectInstantiationCacheLimitSnapshot::capture(interner);
        let result = run_instantiator(interner, query_db, request);
        if !result.depth_exceeded() {
            let request_state_stability = limit_snapshot.request_state_stability_after(interner);
            let memo_result = InstantiationMemoResult::for_project_cache(
                result,
                request_state_stability.is_stable_for_project_cache(),
            );
            db.insert_instantiation_cache_with_project_stability(
                key,
                result.type_id(),
                memo_result.is_stable_for_project_cache(),
            );
            if let Some(alpha_key) = alpha_key
                && let Some(alpha_result) = alpha_canonicalize_cached_result(
                    interner,
                    result.type_id(),
                    &alpha_key.bindings,
                )
            {
                db.insert_instantiation_cache_with_project_stability(
                    alpha_key.key,
                    alpha_result,
                    memo_result.is_stable_for_project_cache(),
                );
            }
        }
        return result;
    }
    run_instantiator(interner, query_db, request)
}

#[inline]
fn resolver_rereduce_instantiation_request(query_db: Option<&dyn QueryDatabase>) -> bool {
    query_db.is_some() && super::flags::inst_resolver_rereduce_enabled()
}

fn alpha_canonicalize_cached_result(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    bindings: &[TypeId],
) -> Option<TypeId> {
    let mut binders = FxHashMap::default();
    let mut alpha_bindings = SmallVec::<[TypeId; 4]>::new();
    for (index, binding) in bindings.iter().copied().enumerate() {
        let TypeData::TypeParameter(info) = interner.lookup(binding)? else {
            return None;
        };
        binders.insert(info.name, index as u32);
        alpha_bindings.push(binding);
    }
    let mut changed = false;
    let mut visited = FxHashSet::default();
    alpha_canonicalize_type(
        interner,
        type_id,
        &mut binders,
        &mut alpha_bindings,
        &mut changed,
        &mut visited,
    )
}

/// Drive a single `TypeInstantiator` configured from `request`, without
/// touching any cache.
fn run_instantiator(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    let options = request.options();
    let mut instantiator =
        TypeInstantiator::new(interner, request.substitution()).with_query_db(query_db);
    instantiator.substitute_infer = options.substitute_infer();
    instantiator.preserve_meta_types = options.preserve_meta_types();
    instantiator.preserve_unsubstituted_type_params = options.preserve_unsubstituted_type_params();
    instantiator.shallow_this_only = options.shallow_this_only();
    instantiator.this_type = request.this_type();
    let result = instantiator.instantiate(request.type_id());
    InstantiationResult::from_walk_with_ambient_limit(
        result,
        instantiator.termination(),
        instantiator.ambient_frame_exhausted(),
    )
}

/// Convenience function for instantiating a type with a substitution.
///
/// Cache-aware overload of [`instantiate_type`]. When the caller provides a
/// `&dyn QueryDatabase`, the cross-call instantiation cache on `QueryCache`
/// is consulted before recursive walking and populated afterwards. Existing
/// callers that pass `&dyn TypeDatabase` (i.e. the no-cache path) continue
/// to work unchanged via [`instantiate_type`].
#[inline]
pub fn instantiate_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_type_cached(interner, None, type_id, substitution)
}

/// Instantiate `request` against `interner` without using any cross-call
/// cache.
///
/// This is the typed boundary that mirrors the legacy `instantiate_type_*`
/// family. Pass an [`InstantiationRequest`] built with the desired
/// [`InstantiationOptions`] and (optionally) `this_type`; the result reports
/// both the produced `TypeId` and whether the recursion-depth guard tripped.
///
/// Callers that already have a `&dyn QueryDatabase` should keep using
/// [`instantiate_type_cached`] and friends, which now route through the same
/// staged engine internally and additionally consult the cross-call cache.
pub fn instantiate_type_with_request(
    interner: &dyn TypeDatabase,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    instantiate_with_request_cached(interner, None, false, request)
}

/// Like [`instantiate_type`], but treats `shadowed_params` as locally bound.
/// Type parameters in that list are returned unchanged even when their
/// constraints reference substituted outer type parameters, so a fresh local
/// binding such as a mapped type's iteration variable cannot be rewritten
/// into its constraint by the forward-reference fallback in `instantiate_key`.
pub(crate) fn instantiate_type_with_shadowed_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    shadowed_params: &[TypeParamInfo],
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if substitution.is_empty() {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution).with_query_db(query_db);
    instantiator.shadowed.extend_from_slice(shadowed_params);
    // The walk returns a relation-preserving value on a depth/frame bail (it
    // never surfaces a substitution-bound free type parameter), so keep it
    // rather than collapsing to `TypeId::ERROR`; see #13652 / `bail_value`.
    instantiator.instantiate(type_id)
}

/// Cache-aware variant of [`instantiate_type`].
///
/// `query_db = Some(db)` enables the cross-call instantiation cache on
/// `QueryCache`.
///
/// The leaf fast paths (`TypeParameter` direct hit, `IndexAccess(T, P)`) run
/// BEFORE any cache-key construction so they remain allocation-free.
#[inline]
pub fn instantiate_type_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    // Fast path: intrinsic types never need instantiation
    if type_id.is_intrinsic() {
        return type_id;
    }
    if substitution.is_empty() {
        return type_id;
    }
    // Fast path: TypeParameter directly in the substitution — return immediately.
    // This is the most common leaf case in mapped type template instantiation.
    // MUST run BEFORE any CanonicalSubst construction so we don't pay
    // hash/alloc for trivial leaf substitutions.
    if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_id)
        && let Some(result) = substitution.get_for_type_parameter(&info)
    {
        return result;
    }
    // Fast path: IndexAccess(T, P) — the most common mapped type template pattern.
    // Recursively instantiate obj and idx without creating a TypeInstantiator.
    // Same reasoning as above: cache-key construction MUST NOT happen for this case.
    if let Some(TypeData::IndexAccess(obj, idx)) = interner.lookup(type_id) {
        let new_obj = instantiate_type_cached(interner, query_db, obj, substitution);
        let new_idx = instantiate_type_cached(interner, query_db, idx, substitution);
        if new_obj == obj && new_idx == idx {
            return type_id;
        }
        if let Some(db) = query_db
            && super::flags::inst_resolver_rereduce_enabled()
            && !crate::visitor::contains_type_parameters(interner, new_obj)
            && !crate::visitor::contains_type_parameters(interner, new_idx)
            && (index_access_operand_needs_resolver(interner, new_obj)
                || index_access_operand_needs_resolver(interner, new_idx))
        {
            // #14346 global re-reduce depth budget: bail to the deferred
            // index-access when the shared native-depth budget is exhausted.
            if let Some(_g) = super::flags::rereduce_depth_try_enter() {
                return db.evaluate_index_access(new_obj, new_idx);
            }
            return interner.index_access(new_obj, new_idx);
        }
        return interner.index_access(new_obj, new_idx);
    }
    // Concrete identity short-circuit — no cache key construction needed. This
    // is intentionally narrower than `!contains_type_parameters`: the TSZ
    // instantiator also normalizes concrete meta-types (`keyof`, indexed
    // access, mapped, template, string-intrinsic), so those shapes must still
    // walk even when an unrelated substitution cannot affect their leaves.
    if can_skip_concrete_instantiation(interner, type_id) {
        return type_id;
    }

    instantiate_with_request_cached(
        interner,
        query_db,
        false,
        InstantiationRequest::new(type_id, substitution),
    )
    .into_type_id()
}

/// Instantiate every type parameter reachable from `type_id` to its constraint.
///
/// This is used as an error-recovery surface after failed overload resolution:
/// tsc keeps the constructor/call fallback type, but the fallback should expose
/// constrained key types like `object` rather than raw, unresolved parameters
/// such as `T` or `K`.
pub fn instantiate_type_params_to_constraints(db: &dyn QueryDatabase, type_id: TypeId) -> TypeId {
    let mut substitution = TypeSubstitution::new();
    with_constraint_visited(|visited| {
        collect_type_param_constraint_substitutions(
            db.as_type_database(),
            type_id,
            &mut substitution,
            visited,
        );
    });
    if substitution.is_empty() {
        type_id
    } else {
        instantiate_type_cached(db.as_type_database(), Some(db), type_id, &substitution)
    }
}

/// [`instantiate_type_params_to_constraints`] for callers that only hold a
/// `&dyn TypeDatabase` (no cross-call cache).
///
/// Maps every type parameter reachable from `type_id` to its constraint and
/// re-instantiates. This is the base-constraint mapper used when reducing an
/// instantiable type to its apparent form — e.g. an `IndexAccess` object such
/// as `Parameters<F>` collapses to `Parameters<(...args: any[]) => any>` when
/// `F extends (...args: any[]) => any`, which then evaluates to `any[]`.
pub fn instantiate_type_params_to_constraints_uncached(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    let mut substitution = TypeSubstitution::new();
    with_constraint_visited(|visited| {
        collect_type_param_constraint_substitutions(interner, type_id, &mut substitution, visited);
    });
    if substitution.is_empty() {
        type_id
    } else {
        instantiate_type(interner, type_id, &substitution)
    }
}

/// Maximum number of substitution passes [`resolve_unbound_type_params_to_defaults`]
/// makes. A parameter default may reference an earlier base parameter
/// (`B = A[]`), so one pass can re-introduce a now-resolvable parameter; the
/// bound plus the per-pass no-progress check terminate self-referential
/// defaults (`T = T`).
const MAX_UNBOUND_DEFAULT_DEPTH: usize = 8;

/// Resolve "dangling" free type parameters in `member_type` — those that are
/// free in the member but NOT present in `in_scope` — to their declared
/// `default → constraint → unknown`, matching tsc's `fillMissingTypeArguments`.
///
/// A value-position member read can resolve to a type that still mentions a
/// base class's type parameter when the class extended that base WITHOUT type
/// arguments (`class Der extends Base`, where `Base<P = …>`): the omitted
/// argument is never bound, so the bare `P` would otherwise leak into the
/// member's value type (a false `TS2339`/`TS7053`/`TS2322` — the raw-parameter
/// sibling of the `error`/`never`-in-a-type-argument-slot leak family). `tsc`
/// binds such an omitted base argument to the parameter's default.
///
/// `in_scope` holds the type parameters legitimately bound by the enclosing
/// generic context (the class's / function's own parameters, e.g. `T` of a
/// generic `Box<T>`); those are preserved, so only genuinely unbound base
/// parameters are resolved. Type parameters bound by an enclosing callable
/// signature are already excluded by [`free_type_parameter_ids_in`].
pub fn resolve_unbound_type_params_to_defaults<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    in_scope: &std::collections::HashSet<TypeId, S>,
) -> TypeId {
    resolve_type_params_to_defaults_core(db, member_type, |param_id, _info| {
        !in_scope.contains(&param_id)
    })
}

/// Resolve dangling free type parameters only when their declaration supplies
/// a concrete fallback (`default` or `constraint`).
///
/// This is the property-access version of [`resolve_unbound_type_params_to_defaults`]
/// for member surfaces that can also be produced by mapped/conditional
/// evaluation. Such evaluators legitimately leave unconstrained helper
/// parameters abstract for diagnostic display, so this variant avoids the final
/// `unknown` fallback for unconstrained parameters.
pub fn resolve_unbound_type_params_to_declared_fallbacks<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    member_type: TypeId,
    in_scope: &std::collections::HashSet<TypeId, S>,
) -> TypeId {
    resolve_type_params_to_defaults_core(db, member_type, |param_id, info| {
        !in_scope.contains(&param_id) && (info.default.is_some() || info.constraint.is_some())
    })
}

/// Resolve the type parameters whose declared name is in `names` and appear
/// free in `ty` to their `default → constraint → unknown`, matching tsc's
/// instantiation of a *failed* generic call's result with default type
/// arguments (`getInferredTypes` falling back to `getDefaultTypeArgumentType`).
///
/// When a generic call (function or constructor) fails the argument-count check
/// before inference runs, tsc still produces a best-effort result type by
/// substituting each of the *signature's own* type parameters with its default,
/// then its constraint, then `unknown`. Resolving only the named (signature-own)
/// parameters preserves any enclosing-scope type parameter the result legitimately
/// mentions (e.g. a nested generic referencing an outer parameter), which must
/// stay abstract. This is the value-position sibling of
/// [`resolve_unbound_type_params_to_defaults`]: both stop a bare, unbound generic
/// parameter from leaking into a value type (a false `TS2322`/`TS2339`).
pub fn resolve_named_type_params_to_defaults<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    ty: TypeId,
    names: &std::collections::HashSet<tsz_common::Atom, S>,
) -> TypeId {
    if names.is_empty() {
        return ty;
    }
    resolve_type_params_to_defaults_core(db, ty, |_param_id, info| names.contains(&info.name))
}

/// Free type parameters of `roots` whose declared name is in `names`,
/// returned as `(name, TypeId)` pairs — the exact interned parameter ids,
/// deduplicated across roots by the underlying visitor.
///
/// The preserve-side complement of [`resolve_named_type_params_to_defaults`]:
/// a caller that owns a generic scope (e.g. a synthesized construct signature
/// whose parameters ARE the binding context for a member read) uses the exact
/// `TypeId`s to mark those parameters in scope, so the unbound-defaults fill
/// ([`resolve_unbound_type_params_to_declared_fallbacks`]) does not collapse
/// them. Matching by name is required because a semantically identical
/// `TypeParamInfo` can intern to a distinct `TypeId` from the one the member
/// types actually reference.
pub fn free_type_params_named<S: std::hash::BuildHasher>(
    db: &dyn TypeDatabase,
    roots: impl IntoIterator<Item = TypeId>,
    names: &std::collections::HashSet<tsz_common::Atom, S>,
) -> Vec<(tsz_common::Atom, TypeId)> {
    use crate::visitors::visitor_predicates::free_type_parameter_ids_in;

    if names.is_empty() {
        return Vec::new();
    }
    free_type_parameter_ids_in(db, roots)
        .into_iter()
        .filter_map(|param_id| match db.lookup(param_id) {
            Some(TypeData::TypeParameter(info)) if names.contains(&info.name) => {
                Some((info.name, param_id))
            }
            _ => None,
        })
        .collect()
}

/// Shared driver for the default-type-argument fallback. `should_resolve`
/// decides, per free type parameter, whether it is replaced by its
/// `default → constraint → unknown` fill (true) or preserved (false). The
/// multi-pass loop lets a parameter default that references an earlier
/// (already-resolved) parameter settle, bounded by [`MAX_UNBOUND_DEFAULT_DEPTH`].
fn resolve_type_params_to_defaults_core(
    db: &dyn TypeDatabase,
    ty: TypeId,
    should_resolve: impl Fn(TypeId, &crate::types::TypeParamInfo) -> bool,
) -> TypeId {
    use crate::visitors::visitor_predicates::free_type_parameter_ids_in;

    let mut current = ty;
    for _ in 0..MAX_UNBOUND_DEFAULT_DEPTH {
        let mut substitution = TypeSubstitution::new();
        for param_id in free_type_parameter_ids_in(db, [current]) {
            let Some(TypeData::TypeParameter(info)) = db.lookup(param_id) else {
                continue;
            };
            if !should_resolve(param_id, &info) {
                continue;
            }
            let usable = |t: Option<TypeId>| t.filter(|&t| t != TypeId::ERROR);
            let fill = usable(info.default)
                .or_else(|| usable(info.constraint))
                .unwrap_or(TypeId::UNKNOWN);
            substitution.insert(info.name, fill);
        }
        if substitution.is_empty() {
            return current;
        }
        let next = instantiate_type(db, current, &substitution);
        if next == current {
            return current;
        }
        current = next;
    }
    current
}

fn collect_type_param_constraint_substitutions(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &mut TypeSubstitution,
    visited: &mut FxHashSet<TypeId>,
) {
    if type_id.is_intrinsic() || !visited.insert(type_id) {
        return;
    }

    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            if let Some(constraint) = info.constraint {
                substitution.insert(info.name, constraint);
                collect_type_param_constraint_substitutions(db, constraint, substitution, visited);
            }
            if let Some(default) = info.default {
                collect_type_param_constraint_substitutions(db, default, substitution, visited);
            }
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            collect_type_param_constraint_substitutions(db, app.base, substitution, visited);
            for &arg in &app.args {
                collect_type_param_constraint_substitutions(db, arg, substitution, visited);
            }
        }
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            for member in db.type_list(list_id).iter().copied() {
                collect_type_param_constraint_substitutions(db, member, substitution, visited);
            }
        }
        Some(
            TypeData::Array(element) | TypeData::ReadonlyType(element) | TypeData::KeyOf(element),
        ) => {
            collect_type_param_constraint_substitutions(db, element, substitution, visited);
        }
        Some(TypeData::IndexAccess(object, index)) => {
            collect_type_param_constraint_substitutions(db, object, substitution, visited);
            collect_type_param_constraint_substitutions(db, index, substitution, visited);
        }
        _ => {}
    }
}

/// Instantiate a type while preserving unsubstituted type parameters.
///
/// Unlike `instantiate_type`, this does NOT fall back to replacing type
/// parameters with their instantiated constraints when they are not in the
/// substitution map. This is needed when instantiating mapped type bodies
/// (constraint + template) with the outer type arguments, so that the mapped
/// key parameter (e.g., `P` from `[P in keyof T]: T[P]`) stays as a type
/// parameter instead of being collapsed to its constraint.
pub fn instantiate_type_preserving(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_type_preserving_cached(interner, None, type_id, substitution)
}

/// Cache-aware variant of [`instantiate_type_preserving`].
pub fn instantiate_type_preserving_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_with_options_cached(
        interner,
        query_db,
        type_id,
        substitution,
        InstantiationOptions::new().with_preserve_unsubstituted_type_params(true),
    )
}

/// Instantiate a type and report whether instantiation depth overflowed.
///
/// This variant is intentionally NOT cached (the cross-call cache lives on
/// the five public entry points; this primitive is also used internally by
/// recursion-sensitive paths that need the depth-overflow signal).
pub fn instantiate_type_with_depth_status(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> InstantiationResult {
    // Fast path: intrinsic types never need instantiation (no type-parameter
    // occurrences, no recursion). Skip the substitution probe AND the
    // `TypeInstantiator` construction. Mirrors the leaf fast path in
    // `instantiate_type_cached` / `instantiate_type_preserving_cached`.
    if type_id.is_intrinsic() {
        return InstantiationResult::ok(type_id);
    }
    if substitution.is_empty() {
        return InstantiationResult::ok(type_id);
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    let result = instantiator.instantiate(type_id);
    // Report the overflow verdict for callers that gate on it, but hand back
    // the relation-preserving bail value (never a substitution-bound free type
    // parameter) instead of the `TypeId::ERROR` sentinel (#13652).
    InstantiationResult::from_walk(result, instantiator.termination())
}

/// Convenience function for instantiating a type while preserving meta-type
/// structure such as `keyof`, index access, and mapped types.
///
/// This is used when callers need to inspect whether an instantiated type still
/// structurally depends on a nominal symbol before a later evaluation pass can
/// safely reduce it.
pub fn instantiate_type_preserving_meta(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_type_preserving_meta_cached(interner, None, type_id, substitution)
}

/// Cache-aware variant of [`instantiate_type_preserving_meta`].
pub fn instantiate_type_preserving_meta_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_with_options_cached(
        interner,
        query_db,
        type_id,
        substitution,
        InstantiationOptions::new().with_preserve_meta_types(true),
    )
}

/// Convenience function for instantiating a type while substituting infer variables.
pub fn instantiate_type_with_infer(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_type_with_infer_cached(interner, None, type_id, substitution)
}

/// Cache-aware variant of [`instantiate_type_with_infer`].
pub fn instantiate_type_with_infer_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    instantiate_with_options_cached(
        interner,
        query_db,
        type_id,
        substitution,
        InstantiationOptions::new().with_substitute_infer(true),
    )
}

/// Convenience function for instantiating a generic type with type arguments.
///
/// Fill in type parameter defaults for an application's args when fewer args
/// are provided than parameters exist. Returns `None` if any missing arg has
/// no default. Defaults that reference earlier type parameters are properly
/// instantiated via `TypeSubstitution::from_args`.
///
/// Example: `Generator<T>` with params `[T, TReturn=any, TNext=unknown]`
/// returns `Some([T, any, unknown])`.
pub fn fill_application_defaults(
    interner: &dyn TypeDatabase,
    args: &[TypeId],
    type_params: &[TypeParamInfo],
) -> Option<Vec<TypeId>> {
    if args.len() >= type_params.len() {
        return Some(args[..type_params.len()].to_vec());
    }
    let subst = TypeSubstitution::from_args(interner, type_params, args);
    let mut result = Vec::with_capacity(type_params.len());
    for (i, param) in type_params.iter().enumerate() {
        if i < args.len() {
            result.push(args[i]);
        } else {
            let resolved = subst.get(param.name)?;
            result.push(resolved);
        }
    }
    Some(result)
}

/// Uses `is_identity_for` instead of the name-only `is_identity` check to
/// correctly handle same-name type parameters from different scopes (e.g.,
/// alias `T` vs function `T extends object`).
pub fn instantiate_generic(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    instantiate_generic_cached(interner, None, type_id, type_params, type_args)
}

/// Cache-aware variant of [`instantiate_generic`]. Shares the canonical
/// `(body, canonical_subst, mode_bits=0, this_type=None)` cache slot so
/// recursive utility expansion that re-applies the same body/substitution
/// pair reuses memoized walks instead of re-traversing the body each step.
///
/// Routes through the staged [`instantiate_with_request_cached`] engine
/// rather than [`instantiate_type_cached`] so the full `TypeInstantiator`
/// walk runs on every cache miss. The leaf fast path on `instantiate_type_cached`
/// for top-level `IndexAccess` returns the raw `IndexAccess(obj, idx)`
/// without the eager `evaluate_index_access` step that
/// `TypeInstantiator::instantiate` performs for fully-concrete index-access
/// types; delegating to it would regress mapped/keyof conformance for
/// bodies whose top-level shape is `T[K]`.
pub fn instantiate_generic_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    // Hoisted before substitution-building so intrinsic bodies skip the
    // FxHashMap allocation and default-resolution passes in `from_args`.
    if type_id.is_intrinsic() || type_params.is_empty() || type_args.is_empty() {
        return type_id;
    }
    let substitution = TypeSubstitution::from_args(interner, type_params, type_args);
    if substitution.is_empty() || substitution.is_identity_for(interner, type_params) {
        return type_id;
    }
    instantiate_with_request_cached(
        interner,
        query_db,
        true,
        InstantiationRequest::new(type_id, &substitution),
    )
    .into_type_id()
}

/// Substitute `ThisType` with a concrete type throughout a type.
///
/// Used for method call return types where `this` refers to the receiver's type.
/// For example, in a fluent builder pattern:
/// ```typescript
/// class Builder { setName(n: string): this { ... } }
/// const b: Builder = new Builder().setName("foo"); // this → Builder
/// ```
pub fn substitute_this_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    substitute_this_type_cached(interner, None, type_id, this_type)
}

/// Cache-aware variant of [`substitute_this_type`].
///
/// We DO probe the cache here even though the substitution is empty, because
/// `this_type.is_some()` makes the `(type_id, this_type)` tuple a meaningful
/// cache key.
///
/// `preserve_unsubstituted_type_params` is forced on so the instantiator's
/// constraint fallback does not collapse type parameters to their constraints
/// when the constraint contains a `ThisType` reference. Example: `T extends A`
/// where `A` has `self(): this` — `substitute_this_type(T, T)` must return
/// `T`, not the constraint with `ThisType` rewritten.
pub fn substitute_this_type_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    // Quick check: if the type is intrinsic, no substitution needed
    if type_id.is_intrinsic() {
        return type_id;
    }
    let empty_subst = TypeSubstitution::new();
    instantiate_with_request_cached(
        interner,
        query_db,
        false,
        InstantiationRequest::new(type_id, &empty_subst)
            .with_options(InstantiationOptions::new().with_preserve_unsubstituted_type_params(true))
            .with_this_type(this_type),
    )
    .into_type_id()
}

/// Shallow variant of [`substitute_this_type`] for call-return-position use.
///
/// When a method declared as `<T>(...): this & T` is called on a receiver,
/// the call-return-type substitution should replace `ThisType` references at
/// the structural level of the return type (Intersection / Union /
/// `IndexAccess` / `KeyOf` / Conditional / Application / etc.) but NOT recurse
/// into named Object/ObjectWithIndex internals.
///
/// Named Object types (interfaces, classes — those with a backing symbol)
/// own a polymorphic `this` scope. Their stored method bodies' `this`
/// references must stay raw so that property access on the post-substitution
/// type (typically an intersection wrapping the receiver) can rebind `this`
/// to the actual intersection at call site, not lock it to a single member.
///
/// Counter-example: `instantiate_type_with_this` for class-inheritance
/// specialization needs the **deep** [`substitute_this_type`] entry which
/// walks Object internals. The two forms split here.
///
/// This fixes the chained `extend({a}).extend({b})` pattern in
/// `intersectionThisTypes.ts`.
pub fn substitute_this_type_at_return_position(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    let empty_subst = TypeSubstitution::new();
    instantiate_with_request_cached(
        interner,
        query_db,
        false,
        InstantiationRequest::new(type_id, &empty_subst)
            .with_options(
                InstantiationOptions::new()
                    .with_preserve_unsubstituted_type_params(true)
                    .with_shallow_this_only(true),
            )
            .with_this_type(this_type),
    )
    .into_type_id()
}

/// Instantiate a generic function type with explicit type arguments.
///
/// Takes a function type and type arguments, applies the substitution to all
/// parts of the function shape (parameters, return type, `this_type`, predicate),
/// and returns a new non-generic function type.
///
/// Returns `None` if the input is not a function type or has no type parameters.
///
/// This is used for JSX components with explicit type arguments like:
/// ```typescript
/// declare function Comp<T>(props: { data: T }): JSX.Element;
/// <Comp<number> data={42} />  // Comp instantiated with T = number
/// ```
pub fn instantiate_function_with_type_args(
    interner: &dyn TypeDatabase,
    func_type: TypeId,
    type_args: &[TypeId],
) -> Option<TypeId> {
    use crate::visitors::visitor::function_shape_id;

    let shape_id = function_shape_id(interner, func_type)?;
    let shape = interner.function_shape(shape_id);

    if shape.type_params.is_empty() || type_args.is_empty() {
        return None;
    }

    // Only allow partial instantiation if we have enough args
    if type_args.len() > shape.type_params.len() {
        return None;
    }

    let subst = TypeSubstitution::from_signature_args(interner, &shape.type_params, type_args);

    let new_params: Vec<_> = shape
        .params
        .iter()
        .map(|p| {
            let new_ty =
                instantiate_type_with_depth_status(interner, p.type_id, &subst).into_type_id();
            ParamInfo {
                type_id: new_ty,
                ..*p
            }
        })
        .collect();

    let new_return =
        instantiate_type_with_depth_status(interner, shape.return_type, &subst).into_type_id();

    let new_this = shape
        .this_type
        .map(|t| instantiate_type_with_depth_status(interner, t, &subst).into_type_id());

    let new_predicate = shape.type_predicate.map(|tp| TypePredicate {
        type_id: tp
            .type_id
            .map(|t| instantiate_type_with_depth_status(interner, t, &subst).into_type_id()),
        ..tp
    });

    Some(interner.function(FunctionShape {
        type_params: vec![],
        params: new_params,
        this_type: new_this,
        return_type: new_return,
        type_predicate: new_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    }))
}
