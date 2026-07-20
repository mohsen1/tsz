//! Lazy-application and resolver-need probes for instantiation.
//!
//! Structural predicates that decide when a mapped / conditional / indexed-access
//! template must defer to a real `TypeResolver` (unresolved `Lazy(DefId)`
//! applications, `infer` / type-parameter references) plus the concrete-conditional
//! evaluation fast path. Extracted verbatim from `api.rs` to keep that shard under
//! the size limit.

use super::*;

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

/// Whether a mapped template's conditional condition references a body the
/// instantiator's `NoopResolver` cannot expand, so eager evaluation would
/// collapse or garble it and the mapped must be deferred to the resolver-backed
/// outer evaluator.
///
/// A **direct** `Lazy(DefId)` on either side (`T[K] extends Function ? …`) is
/// always a boundary. A **lazy application** anywhere in the condition
/// (`T[K] extends V<any> ? K : never`, cross-file generic interface) only
/// collapses to `never` for a plain subtype-test filter; when the `extends`
/// clause introduces `infer` types (`P[K] extends V<infer X> ? V<…> : …`) the
/// conditional is an extraction the instantiator already evaluates correctly,
/// and deferring it would leave the surrounding mapped un-reduced — drifting the
/// materialized shape and its diagnostics (see `tsxLibraryManagedAttributes`).
/// So a lazy application is treated as a boundary only when the condition
/// carries no `infer`.
pub(super) fn conditional_condition_needs_resolver(
    interner: &dyn TypeDatabase,
    template: TypeId,
) -> bool {
    let Some(cond) = crate::type_queries::get_conditional_type(interner, template) else {
        return false;
    };
    let bare_lazy = matches!(interner.lookup(cond.extends_type), Some(TypeData::Lazy(_)))
        || matches!(interner.lookup(cond.check_type), Some(TypeData::Lazy(_)));
    // A conditional whose `extends` clause introduces `infer` types is an
    // extraction (`P[K] extends V<infer X> ? … : …`), not a resolver-less
    // collapse-to-`never` filter; keep infer-bearing conditions eager.
    let lazy_application =
        !crate::visitors::visitor_predicates::contains_infer_types(interner, cond.extends_type)
            && (type_contains_lazy_application(interner, cond.extends_type)
                || type_contains_lazy_application(interner, cond.check_type));
    bare_lazy || lazy_application
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
/// Whether deferral of type-parameter-default conditionals whose `check`/`extends`
/// still hold an unresolved `Lazy(DefId)`/`Recursive` ref is active.
///
/// Default-on; `TSZ_DISABLE_DEFER_LAZY_DEFAULT_CONDITIONAL=1` is the kill switch
/// that restores the prior eager (resolver-less) branch selection. See
/// [`maybe_evaluate_concrete_conditional`].
fn defer_lazy_default_conditional_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !std::env::var("TSZ_DISABLE_DEFER_LAZY_DEFAULT_CONDITIONAL").is_ok_and(|v| v == "1")
    })
}

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
    // The branch is picked by a *resolver-less* subtype check (no `query_db`),
    // so it cannot follow a semantic ref. When `check_type` or `extends_type`
    // contains a `Lazy(DefId)`/`Recursive` ref — for example a type-parameter
    // default whose `extends` is `keyof T` and `T` was substituted with the
    // unresolved `Lazy(Route)` form, leaving `keyof Lazy(Route)` — the relation
    // sees a deferred meta-type over an unresolved alias and silently answers
    // `false`, picking the wrong (false) branch and baking it into the default.
    // That is how ts-rest's `ClientInferRequestBase` third-parameter default
    // (`'headers' extends keyof T ? Prettify<...> : never`) collapses to `never`
    // and the surrounding `Without<...>` mapped type can no longer surface
    // `headers`. Leave such conditionals deferred so the later resolver-aware
    // evaluator (reached on property access / mapped-type expansion) resolves
    // the alias and picks the branch correctly. `Application` bases are
    // intentionally NOT disqualifying here: the documented
    // `K extends string ? Map<K, V> : Map<string, V>` case keeps its concrete
    // `string extends string` check/extends and must still resolve eagerly to
    // preserve the branch `Application` `TypeId` identity.
    // Kill switch: `TSZ_DISABLE_DEFER_LAZY_DEFAULT_CONDITIONAL=1` restores the
    // prior eager (resolver-less) branch selection.
    if defer_lazy_default_conditional_enabled()
        && (crate::type_queries::contains_lazy_or_recursive_db(interner, cond.check_type)
            || crate::type_queries::contains_lazy_or_recursive_db(interner, cond.extends_type))
    {
        tracing::trace!(
            type_id = type_id.0,
            check = cond.check_type.0,
            extends = cond.extends_type.0,
            "maybe_evaluate_concrete_conditional: deferring (check/extends holds Lazy/Recursive ref)"
        );
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

/// Check whether `type_id` references the given logical type-parameter binder.
///
/// Used to detect circular type parameter defaults. When a default resolves
/// to (or contains) the parameter it is defaulting, tsc falls back to `any`.
/// This is a shallow check: it handles the direct self-reference case
/// (`type T<X extends C = X>`) and union/intersection wrappers.
pub(super) fn type_references_param(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    param: TypeParamInfo,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match interner.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.is_same_binder(param),
        Some(TypeData::Union(members_id)) | Some(TypeData::Intersection(members_id)) => {
            let members = interner.type_list(members_id);
            members
                .iter()
                .any(|&m| type_references_param(interner, m, param))
        }
        _ => false,
    }
}
