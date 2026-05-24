//! Instantiation engine internals split out of `instantiate.rs` to keep each
//! source shard under the repository file-size limit. Contains the cached
//! entry points, the public `instantiate_*` / `substitute_this_*` wrappers,
//! and the lazy-application detection helpers. Behavior is unchanged; this is
//! a pure code-organization split.

use super::*;
use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::instantiation::request::{InstantiationOptions, InstantiationRequest};
use crate::instantiation::result::InstantiationResult;
use crate::types::{FunctionShape, ParamInfo, TypeData, TypeId, TypeParamInfo, TypePredicate};
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use tsz_common::interner::Atom;

// === pool + helper ===
// Reusable scratch `FxHashSet<TypeId>` for the recursive DFS used by
// `instantiate_type_params_to_constraints`. Mirrors the pool pattern from
// #4722 / #4790 / #4801.
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

// === free functions ===
/// Shared body for the option-only wrappers
/// (`instantiate_type_preserving_cached`, `instantiate_type_preserving_meta_cached`,
/// `instantiate_type_with_infer_cached`).
///
/// All three apply the same "intrinsic check → empty/identity short-circuit →
/// delegate to engine" prelude; the only thing that varies is the option set
/// passed to the instantiator. `instantiate_type_cached` does NOT share this
/// helper because it has additional allocation-free leaf fast paths
/// (`TypeParameter`, `IndexAccess(T, P)`) that must precede any cache-key
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
        InstantiationRequest::new(type_id, substitution).with_options(options),
    )
    .into_type_id()
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
pub(crate) fn instantiate_with_request_cached(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    if let Some(db) = query_db {
        let key = request.cache_key();
        if let Some(cached) = db.lookup_instantiation_cache(&key) {
            return InstantiationResult::ok(cached);
        }
        let result = run_instantiator(interner, request);
        if !result.depth_exceeded() {
            db.insert_instantiation_cache(key, result.type_id());
        }
        return result;
    }
    run_instantiator(interner, request)
}

/// Drive a single `TypeInstantiator` configured from `request`, without
/// touching any cache.
fn run_instantiator(
    interner: &dyn TypeDatabase,
    request: InstantiationRequest<'_>,
) -> InstantiationResult {
    let options = request.options();
    let mut instantiator = TypeInstantiator::new(interner, request.substitution());
    instantiator.substitute_infer = options.substitute_infer();
    instantiator.preserve_meta_types = options.preserve_meta_types();
    instantiator.preserve_unsubstituted_type_params = options.preserve_unsubstituted_type_params();
    instantiator.shallow_this_only = options.shallow_this_only();
    instantiator.this_type = request.this_type();
    let result = instantiator.instantiate(request.type_id());
    InstantiationResult::from_walk(result, instantiator.depth_exceeded)
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
    instantiate_with_request_cached(interner, None, request)
}

/// Like [`instantiate_type`], but treats `shadowed_params` as locally bound.
/// Type parameters in that list are returned unchanged even when their
/// constraints reference substituted outer type parameters, so a fresh local
/// binding such as a mapped type's iteration variable cannot be rewritten
/// into its constraint by the forward-reference fallback in `instantiate_key`.
pub(crate) fn instantiate_type_with_shadowed(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    shadowed_params: &[Atom],
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if substitution.is_empty() {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.shadowed.extend_from_slice(shadowed_params);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        return TypeId::ERROR;
    }
    result
}

/// Like [`instantiate_type_preserving`] but also replaces every
/// `IndexAccess(source, K)` in the template with `declared_type`, where `K`
/// is any `TypeParameter` whose name equals `iter_var`.
///
/// This is used for homomorphic `-?` mapped type evaluation. When `-?` strips
/// the optional modifier, tsc feeds the DECLARED property type (without the
/// `| undefined` that normal read access adds for optional properties) into the
/// template. Applying this substitution before the `K → key_literal` step
/// ensures that conditional and other composite templates that reference `T[K]`
/// see the de-optionalized type, matching tsc behavior.
pub(crate) fn instantiate_type_preserving_with_declared(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
    source: TypeId,
    iter_var: Atom,
    declared_type: TypeId,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.preserve_unsubstituted_type_params = true;
    instantiator.declared_index_type = Some((source, iter_var, declared_type));
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
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
    match interner.lookup(type_id) {
        // Fast path: TypeParameter directly in the substitution — return immediately.
        // This is the most common leaf case in mapped type template instantiation.
        // MUST run BEFORE any CanonicalSubst construction so we don't pay
        // hash/alloc for trivial leaf substitutions.
        Some(TypeData::TypeParameter(info)) => {
            if let Some(result) = substitution.get(info.name) {
                return result;
            }
        }
        // Fast path: IndexAccess(T, P) — the most common mapped type template pattern.
        // Recursively instantiate obj and idx without creating a TypeInstantiator.
        // Same reasoning as above: cache-key construction MUST NOT happen for this case.
        Some(TypeData::IndexAccess(obj, idx)) => {
            let new_obj = instantiate_type_cached(interner, query_db, obj, substitution);
            let new_idx = instantiate_type_cached(interner, query_db, idx, substitution);
            if new_obj == obj && new_idx == idx {
                return type_id;
            }
            return interner.index_access(new_obj, new_idx);
        }
        _ => {}
    }

    // Empty/identity short-circuit — no cache key construction needed.
    if substitution.is_empty() {
        return type_id;
    }

    instantiate_with_request_cached(
        interner,
        query_db,
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
) -> (TypeId, bool) {
    // Fast path: intrinsic types never need instantiation (no type-parameter
    // occurrences, no recursion). Skip the substitution probe AND the
    // `TypeInstantiator` construction. Mirrors the leaf fast path in
    // `instantiate_type_cached` / `instantiate_type_preserving_cached`.
    if type_id.is_intrinsic() {
        return (type_id, false);
    }
    if substitution.is_empty() {
        return (type_id, false);
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        (TypeId::ERROR, true)
    } else {
        (result, false)
    }
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
    if type_params.is_empty() || type_args.is_empty() {
        return type_id;
    }
    let substitution = TypeSubstitution::from_args(interner, type_params, type_args);
    if substitution.is_empty() || substitution.is_identity_for(interner, type_params) {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, &substitution);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
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

/// Check whether a mapped-type template is a **union or intersection** that
/// contains an `Application` type whose base is a `Lazy(DefId)` reference.
///
/// This pattern occurs in recursive mapped types like:
///   `Spec<T> = { [P in keyof T]: Func<T[P]> | Spec<T[P]> }`
/// where the template union includes a self-referential type alias application.
///
/// The instantiator's eager `evaluate_type` uses `NoopResolver`, which cannot
/// resolve `Lazy` references.  When a union member is an unresolvable
/// application, the mapped type evaluator produces an incomplete object that
/// silently drops that member.  Deferring lets the outer evaluator (which has
/// a proper `TypeResolver`) handle the full expansion.
///
/// We intentionally do NOT match a top-level Application (e.g. `Selector<S, T[K]>`)
/// because the evaluator correctly passes those through as-is.  Only unions/
/// intersections are at risk of member loss.
fn type_is_lazy_application(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }

    let Some(TypeData::Application(app_id)) = interner.lookup(type_id) else {
        return false;
    };
    let app = interner.type_application(app_id);
    !app.base.is_intrinsic() && matches!(interner.lookup(app.base), Some(TypeData::Lazy(..)))
}

/// Check whether `type_id` is a lazy application, or a union/intersection whose
/// immediate members contain one.
///
/// This intentionally does not recursively inspect arbitrary nested types.
/// Eager evaluation only loses members for the immediate mapped-template shape;
/// recursive matching also catches unrelated implementation details and can
/// change assignability/display behavior for conditionals that should still be
/// evaluated in place.
pub(super) fn template_has_lazy_application_in_composite(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let Some(data) = interner.lookup(type_id) else {
        return false;
    };
    match data {
        TypeData::Union(members) | TypeData::Intersection(members) => {
            let list = interner.type_list(members);
            list.iter().any(|&m| type_is_lazy_application(interner, m))
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            template_has_lazy_application_in_composite(interner, cond.true_type)
                || template_has_lazy_application_in_composite(interner, cond.false_type)
        }
        _ => false,
    }
}

/// Check whether `type_id` reaches an `Application(Lazy(_), _)` anywhere in
/// its structure.
///
/// `NoopResolver` cannot expand `Lazy` alias bodies, so eagerly evaluating
/// a type that contains such an application silently folds it into `never`.
/// Callers defer evaluation to an outer evaluator with a real resolver.
pub(super) fn type_contains_lazy_application(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::visitors::visitor_predicates::contains_type_matching(interner, type_id, |key| {
        let TypeData::Application(app_id) = key else {
            return false;
        };
        let app = interner.type_application(*app_id);
        matches!(interner.lookup(app.base), Some(TypeData::Lazy(_)))
    })
}

/// Check whether a mapped constraint needs a real resolver before it can be
/// evaluated without losing key information.
///
/// The instantiator runs with `NoopResolver`, so eagerly evaluating
/// `keyof Application(...)` here can collapse a mapped type before the actual
/// alias/application body is available. Deferring lets the outer evaluator,
/// which has a real `TypeResolver`, materialize the correct key set later.
pub(super) fn mapped_constraint_needs_resolver(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let key = match interner.lookup(type_id) {
        Some(key) => key,
        None => return false,
    };

    match key {
        TypeData::KeyOf(operand) => matches!(
            interner.lookup(operand),
            Some(TypeData::Application(_) | TypeData::Lazy(_) | TypeData::TypeQuery(_))
        ),
        TypeData::Application(_) | TypeData::Lazy(_) | TypeData::TypeQuery(_) => true,
        _ => false,
    }
}

/// Check whether an instantiated indexed-access operand should be evaluated by
/// the outer evaluator instead of the instantiator's `NoopResolver`.
///
/// Eagerly reducing `T[K]` is useful for simple concrete keys, but resolver-backed
/// meta-types inside either operand can still need alias expansion. For example,
/// `{ 1: T; 0: U }[Length<I> extends N ? 1 : 0]` must let the real evaluator
/// resolve `Length<I>` after `I` and `N` are substituted; reducing it here can
/// take the false branch because `Length` is still an unresolvable application.
pub(super) fn index_access_operand_needs_resolver(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    crate::visitors::visitor_predicates::contains_type_matching(interner, type_id, |key| {
        matches!(
            key,
            TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::TypeQuery(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::Mapped(_)
        )
    })
}

/// Evaluate a conditional type immediately if its `check_type` and `extends_type`
/// are both concrete (contain no type parameters and no infer types).
///
/// When a generic default like `K extends string ? Map<K, V> : Map<string, V>`
/// is instantiated with K=string, V=number, the result is a `ConditionalType`
/// `string extends string ? Map<string,number> : Map<string,number>`. Since
/// both sides are concrete, we can pick the branch directly without evaluating
/// it, preserving the `Application` `TypeId` identity of the branch. Returning
/// the branch unevaluated ensures that the substitution carries the same interned
/// `Map<string,number>` `Application` `TypeId` that the checker produces for the
/// source expression, so the subtype comparison succeeds without structural expansion.
pub(super) fn maybe_evaluate_concrete_conditional(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    let Some(TypeData::Conditional(cond_id)) = interner.lookup(type_id) else {
        return type_id;
    };
    let cond = interner.get_conditional(cond_id);
    // Only pick a branch when neither side contains type parameters or infer types.
    if crate::visitor::contains_type_parameters(interner, cond.check_type)
        || crate::visitor::contains_type_parameters(interner, cond.extends_type)
        || crate::type_queries::contains_infer_types_db(interner, cond.extends_type)
        || crate::type_queries::contains_infer_types_db(interner, cond.true_type)
        || crate::type_queries::contains_infer_types_db(interner, cond.false_type)
    {
        return type_id;
    }
    // For distributive conditionals where check_type is a union, distributing
    // would produce a union of branch results which requires the full evaluator.
    if cond.is_distributive && matches!(interner.lookup(cond.check_type), Some(TypeData::Union(_)))
    {
        return type_id;
    }
    // Both check and extends are concrete. Use a subtype check to pick the branch
    // and return it DIRECTLY (not evaluated) so Application TypeIds are preserved.
    let branch = if crate::relations::subtype::core::is_subtype_of(
        interner,
        cond.check_type,
        cond.extends_type,
    ) {
        cond.true_type
    } else {
        cond.false_type
    };
    tracing::trace!(
        type_id = type_id.0,
        check = cond.check_type.0,
        extends = cond.extends_type.0,
        true_type = cond.true_type.0,
        false_type = cond.false_type.0,
        branch = branch.0,
        "maybe_evaluate_concrete_conditional: picked branch"
    );
    branch
}

/// Check whether `type_id` references a type parameter with the given `name`.
///
/// Used to detect circular type parameter defaults. When a default resolves
/// to (or contains) the parameter it is defaulting, tsc falls back to `any`.
/// This is a shallow check: it handles the direct self-reference case
/// (`type T<X extends C = X>`) and union/intersection wrappers.
pub(super) fn type_references_param(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: tsz_common::interner::Atom,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match interner.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.name == param_name,
        Some(TypeData::Union(members_id)) | Some(TypeData::Intersection(members_id)) => {
            let members = interner.type_list(members_id);
            members
                .iter()
                .any(|&m| type_references_param(interner, m, param_name))
        }
        _ => false,
    }
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

    let subst = TypeSubstitution::from_args(interner, &shape.type_params, type_args);

    let new_params: Vec<_> = shape
        .params
        .iter()
        .map(|p| {
            let (new_ty, _) = instantiate_type_with_depth_status(interner, p.type_id, &subst);
            ParamInfo {
                name: p.name,
                type_id: new_ty,
                optional: p.optional,
                rest: p.rest,
            }
        })
        .collect();

    let (new_return, _) = instantiate_type_with_depth_status(interner, shape.return_type, &subst);

    let new_this = shape
        .this_type
        .map(|t| instantiate_type_with_depth_status(interner, t, &subst).0);

    let new_predicate = shape.type_predicate.map(|tp| TypePredicate {
        type_id: tp
            .type_id
            .map(|t| instantiate_type_with_depth_status(interner, t, &subst).0),
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
