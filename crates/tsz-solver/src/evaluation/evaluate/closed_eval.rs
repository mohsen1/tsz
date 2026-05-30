//! Substitution-independent persistent evaluation cache (`closed_eval_cache`).
//!
//! Recursive TypeBox-style shapes (`Static<T,P> = (T & {params:P})['static']`,
//! `PropertiesReduce<T,P> = { [K in keyof T]: Static<T[K], P> }`) re-evaluate the
//! same closed subtrees thousands of times across the many fresh `TypeEvaluator`
//! instances that instantiation and the checker's first/second passes spin up.
//! This module memoizes the evaluation of *substitution-independent* nodes in a
//! project-wide cache so that work is O(1) on repeat shapes.
//!
//! Caching here can only change speed, never results, because of three gates:
//!  - **Input gate**: the cached node contains no `TypeParameter`/`Infer`/
//!    `ThisType`/`BoundParameter`, so its evaluation does not depend on the
//!    active substitution environment — only on the project's single fixed
//!    resolver (via any `Lazy`/`TypeQuery` refs). The mapping is stable per
//!    `TypeId`.
//!  - **Authoritative-write gate**: only evaluators with a `query_db` (the
//!    second-pass `CheckerContext` and subtype-checker evaluators) write. The
//!    limited first-pass `TypeEnvironment` resolver never stores an
//!    under-resolved result a sibling read would observe. Reads are safe for any
//!    resolver because the stored value is a definite, authoritative answer.
//!  - **Limit gate**: a run that hit any recursion/complexity limit
//!    (`deep_recursion_seen`, the `TS2589` depth machinery, or the `TS2590`
//!    union-too-complex flag) caches nothing — a cached read must never
//!    short-circuit an expansion the type system must continue in order to
//!    re-derive those diagnostics.

use super::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};

/// Debug kill-switch for the substitution-independent `closed_eval_cache`.
/// Set `TSZ_DISABLE_CLOSED_EVAL_CACHE=1` to bypass both reads and writes.
/// Used only to bisect regressions; defaults to enabled.
fn closed_eval_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TSZ_DISABLE_CLOSED_EVAL_CACHE").is_err())
}

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Try to return a cached evaluation result for a cacheable, substitution-
    /// independent `type_id`. Returns `None` on a miss or an ineligible node.
    pub(super) fn try_closed_eval_read(&self, type_id: TypeId) -> Option<TypeId> {
        if !closed_eval_cache_enabled()
            || !self.is_closed_cacheable_kind(type_id)
            || crate::type_queries::is_substitution_dependent_type(self.interner, type_id)
        {
            return None;
        }
        self.interner
            .lookup_closed_eval_cache(type_id, self.no_unchecked_indexed_access)
    }

    /// Commit this evaluator's per-evaluator cache entries to the project-wide
    /// `closed_eval_cache`, subject to the authoritative-write and limit gates.
    ///
    /// `union_too_complex_before` is the `TS2590` flag snapshot taken before the
    /// top-level evaluation began; if the run newly tripped the flag, nothing is
    /// cached.
    pub(super) fn commit_closed_eval_writes(&self, union_too_complex_before: bool) {
        // Only the checker's authoritative, context-free type-resolution pass
        // (opted in via `with_closed_eval_writes`) writes. Evaluators running
        // mid-relation / mid-inference / mid-narrowing must not, since their
        // results can depend on context the cache key does not capture.
        let is_top_level = closed_eval_cache_enabled()
            && self.closed_eval_writes_allowed
            && self.query_db.is_some()
            && self.guard.depth() == 0;
        if !is_top_level
            || self.silent_depth_bailed
            || self.guard.is_exceeded()
            || self.deep_recursion_seen
            || (self.interner.is_union_too_complex() && !union_too_complex_before)
        {
            return;
        }
        let no_unchecked = self.no_unchecked_indexed_access;
        // Collect first to avoid borrowing the per-evaluator cache while the
        // content query borrows the interner.
        let entries: Vec<(TypeId, TypeId)> = self
            .cache
            .iter()
            .filter(|(node, _)| !node.is_intrinsic())
            .map(|(&node, &node_result)| (node, node_result))
            .collect();
        for (node, node_result) in entries {
            if self.is_closed_cacheable_kind(node)
                && !crate::type_queries::is_substitution_dependent_type(self.interner, node)
            {
                self.interner
                    .insert_closed_eval_cache(node, no_unchecked, node_result);
            }
        }
    }

    /// Whether `type_id` is eligible for the substitution-independent
    /// `closed_eval_cache`.
    ///
    /// Eligible kinds are the meta-operations `IndexAccess`/`KeyOf` and an
    /// alias `Application`, each subject to two structural exclusions:
    ///
    /// 1. **No conditional in the syntactic body.** An `IndexAccess`/`KeyOf`
    ///    node, or an application's resolved alias body, must not syntactically
    ///    contain a `Conditional` type (scanning the structure but treating
    ///    nested `Lazy`/`Application` bases as opaque leaves). A conditional's
    ///    evaluation can bind `infer` placeholders whose result depends on the
    ///    *inference* / *narrowing* / *contextual* state at the use site — state
    ///    the `(TypeId, no_unchecked)` cache key does not capture
    ///    (`propTypeValidatorInference`, `strictSubtypeAndNarrowing`,
    ///    `contextuallyTypedJsxAttribute2`). The `TypeBox` `Static<T,P> = (T &
    ///    {params:P})['static']` / `PropertiesReduce` bodies are
    ///    intersection/index-access shaped with no syntactic conditional, so they
    ///    stay eligible — the conditional (`Evaluate`) only appears one alias
    ///    deeper, behind an opaque `Lazy`/`Application` boundary.
    /// 2. **Index object not index-signature bearing.** For `IndexAccess`/`KeyOf`
    ///    the operand object must not be (or resolve to) a bare mapped type or an
    ///    index-signature object (`Record<string, number>[K]`), whose
    ///    element-access diagnostics the checker derives from the structural
    ///    index-signature form (`keyofAndIndexedAccess2`). For an `Application`
    ///    the resolved body must not be a bare `Mapped` (homomorphic
    ///    `Partial`/`Readonly`/`Record`; `mappedTypes5`).
    ///
    /// `Union`/`Intersection` node inputs are not cacheable: caching a normalized
    /// result can shrink a cross-product so a later read no longer trips the
    /// `TS2590` complexity limit (`templateLiteralTypes1`).
    pub(super) fn is_closed_cacheable_kind(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::KeyOf(operand)) => {
                self.is_index_object_cacheable(operand) && !self.body_has_conditional(type_id)
            }
            Some(TypeData::IndexAccess(obj, _)) => {
                self.is_index_object_cacheable(obj) && !self.body_has_conditional(type_id)
            }
            Some(TypeData::Application(_)) => self.is_application_body_cacheable(type_id),
            _ => false,
        }
    }

    /// Whether the *syntactic* structure of `type_id` contains a `Conditional`.
    ///
    /// `contains_type_matching` descends into a type's structure (object members,
    /// union/intersection members, mapped templates, index-access operands,
    /// application arguments) but treats nested `Lazy`/`Application` bases as
    /// opaque leaves — it does not resolve aliases. That boundary is exactly what
    /// distinguishes the safe and unsafe shapes:
    /// - A conditional's evaluation can bind `infer` placeholders and resolve
    ///   against the inference/contextual state at the use site, which the
    ///   `(TypeId, no_unchecked)` cache key does not capture. When the conditional
    ///   sits directly in the body's structure (e.g. `RequiredKeys<V> = { [K in
    ///   keyof V]-?: … extends Validator<infer T> ? … }[keyof V]`), this returns
    ///   `true` and the body is excluded
    ///   (`propTypeValidatorInference`/`strictSubtypeAndNarrowing`).
    /// - The `TypeBox` `Static<T,P> = (T & {params:P})['static']` body is an
    ///   `IndexAccess` over an intersection with no syntactic conditional, so it
    ///   stays eligible — the conditional (`Evaluate`) only appears behind a
    ///   further alias boundary this scan does not cross. Application-chain
    ///   utilities like `Omit`/`Pick`/`ComponentPropsWithRef` are already
    ///   excluded earlier by the `IndexAccess`-body requirement.
    fn body_has_conditional(&self, type_id: TypeId) -> bool {
        crate::visitors::visitor_predicates::contains_type_matching(self.interner, type_id, |k| {
            matches!(k, TypeData::Conditional(_))
        })
    }

    /// Whether the object operand of a cacheable `IndexAccess`/`KeyOf` is safe to
    /// cache over.
    ///
    /// Restricted to operands that are *not* index-signature bearing: a bare
    /// mapped type, or an application/alias that resolves to one (`Record`,
    /// `Partial`, `Readonly`), keeps index-signature-driven element-access
    /// diagnostics that the checker derives from the structural form
    /// (`keyofAndIndexedAccess2`). Intersections/objects/tuples and applications
    /// with non-mapped bodies (`Static`, `PropertiesReduce`) are safe.
    fn is_index_object_cacheable(&self, obj: TypeId) -> bool {
        match self.interner.lookup(obj) {
            Some(TypeData::Application(_)) => self.is_application_body_cacheable(obj),
            // A nested index access / keyof over a cacheable object stays fine.
            Some(TypeData::IndexAccess(inner_obj, _) | TypeData::KeyOf(inner_obj)) => {
                self.is_index_object_cacheable(inner_obj)
            }
            // Resolve a `Lazy` alias to decide on its body (e.g. `Dict =
            // Record<string, number>` resolves to a mapped/index-signature type).
            Some(TypeData::Lazy(def_id)) => match self.resolver.resolve_lazy(def_id, self.interner)
            {
                Some(body) if body != obj => self.is_index_object_cacheable(body),
                _ => false,
            },
            // An intersection is safe only if every member is.
            Some(TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .all(|&m| self.is_index_object_cacheable(m)),
            // A bare mapped object keeps its index-signature relation behavior;
            // an object carrying an index signature does too.
            Some(TypeData::Mapped(_) | TypeData::ObjectWithIndex(_)) => false,
            _ => true,
        }
    }

    /// Whether an `Application` type is safe to cache by its base's resolved
    /// alias body.
    ///
    /// The body must be an `IndexAccess` (the `TypeBox` `Static<T,P> = (T &
    /// {params:P})['static']` shape) carrying no `Conditional` within the bounded
    /// resolution scan. This is intentionally narrow:
    /// - Mapped / index-signature bodies (`Partial`/`Readonly`/`Record`) need the
    ///   structural mapped form for relation/diagnostics
    ///   (`mappedTypes5`/`keyofAndIndexedAccess2`).
    /// - Application / conditional-bearing bodies (`Omit -> Pick -> Exclude`,
    ///   `RequiredKeys<V> = {…infer…}[keyof V]`, `ComponentPropsWithRef<…>`) bind
    ///   `infer` placeholders against inference/contextual state the cache key
    ///   does not capture (`propTypeValidatorInference`,
    ///   `contextuallyTypedJsxAttribute2`).
    ///
    /// `Static`'s `IndexAccess` body over an intersection has no syntactic
    /// conditional, so it stays eligible while the utility chains are excluded.
    fn is_application_body_cacheable(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_id) = self.resolve_application_def_id(app.base) else {
            // Unresolvable base: keep opaque, do not cache.
            return false;
        };
        match self.resolver.resolve_lazy(def_id, self.interner) {
            Some(body) => {
                matches!(
                    self.interner.lookup(body),
                    Some(TypeData::IndexAccess(_, _))
                ) && !self.body_has_conditional(body)
            }
            // Body not resolvable by this resolver: be conservative.
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::def::DefId;

    fn evaluator(interner: &TypeInterner) -> TypeEvaluator<'_> {
        TypeEvaluator::new(interner)
    }

    /// The substitution-independent cache is eligible for `IndexAccess`/`KeyOf`
    /// meta-operations but never for `Union`/`Intersection` node inputs (caching
    /// a normalized cross-product could suppress `TS2590`).
    #[test]
    fn cacheable_kinds_exclude_union_and_intersection() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // IndexAccess over a plain concrete object operand is eligible.
        let idx = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        assert!(ev.is_closed_cacheable_kind(idx));

        // keyof over a plain concrete operand is eligible.
        let keyof = interner.keyof(TypeId::OBJECT);
        assert!(ev.is_closed_cacheable_kind(keyof));

        // Union / Intersection node inputs are never eligible.
        let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
        let inter = interner.intersection(vec![TypeId::OBJECT, TypeId::STRING]);
        assert!(!ev.is_closed_cacheable_kind(union));
        assert!(!ev.is_closed_cacheable_kind(inter));

        // A primitive / plain object is not a meta-operation, so not eligible.
        assert!(!ev.is_closed_cacheable_kind(TypeId::STRING));
        assert!(!ev.is_closed_cacheable_kind(TypeId::OBJECT));
    }

    /// An `IndexAccess`/`KeyOf` whose structure contains a `Conditional` is
    /// excluded — the conditional can bind `infer` against context the cache key
    /// does not capture. The check is name-agnostic (uses structure, not
    /// spellings).
    #[test]
    fn cacheable_kinds_exclude_conditional_bearing_index_access() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // A conditional `string extends number ? 1 : 2` interned as the index.
        let cond = interner.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::ANY,
            false_type: TypeId::UNKNOWN,
            is_distributive: false,
        });
        // IndexAccess whose index operand is a conditional → structure contains
        // a conditional → excluded.
        let idx_with_cond = interner.index_access(TypeId::OBJECT, cond);
        assert!(ev.body_has_conditional(idx_with_cond));
        assert!(!ev.is_closed_cacheable_kind(idx_with_cond));

        // The same shape without the conditional stays eligible.
        let idx_plain = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        assert!(!ev.body_has_conditional(idx_plain));
        assert!(ev.is_closed_cacheable_kind(idx_plain));
    }

    /// An `IndexAccess`/`KeyOf` over an index-signature-bearing operand (a bare
    /// mapped type, or one reached through an alias) is excluded, because the
    /// checker derives element-access diagnostics from the structural form.
    #[test]
    fn cacheable_kinds_exclude_index_signature_operand() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // A `NoopResolver` cannot resolve a `Lazy` alias's body, so an index
        // access over a `Lazy` operand is conservatively excluded.
        let lazy = interner.lazy(DefId(123));
        let idx_over_lazy = interner.index_access(lazy, TypeId::STRING);
        assert!(!ev.is_closed_cacheable_kind(idx_over_lazy));

        // An application node with an unresolvable base is also excluded
        // (conservative: the body cannot be proven safe).
        let app = interner.application(lazy, vec![TypeId::STRING]);
        assert!(!ev.is_closed_cacheable_kind(app));
    }
}
