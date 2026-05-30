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
        let is_top_level =
            closed_eval_cache_enabled() && self.query_db.is_some() && self.guard.depth() == 0;
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
    /// - The meta-operation kinds `IndexAccess` and `KeyOf` drive the
    ///   `Static<T[K], P> = (T & {params:P})['static']` re-evaluation fan-out and
    ///   carry no application display-alias provenance, so reusing their result
    ///   cannot change a user-facing alias.
    /// - An `Application` is eligible unless its base resolves to a *bare*
    ///   homomorphic mapped type (`{ [K in keyof T]: … }`, e.g.
    ///   `Partial`/`Readonly`). The subtype checker has a dedicated homomorphic
    ///   relation path that needs the structural mapped form; pre-evaluating and
    ///   caching the expanded object changed assignability (`mappedTypes5`).
    ///   User aliases whose body is a conditional/intersection/another
    ///   application (`Static`, `PropertiesReduce`, `Evaluate`) stay eligible.
    /// - `Union`/`Intersection` are excluded: caching a normalized result can
    ///   shrink a cross-product so a later read no longer trips the `TS2590`
    ///   complexity limit (`templateLiteralTypes1`).
    /// - An `IndexAccess`/`KeyOf` is eligible only when its operand object is
    ///   itself cacheable. This excludes index access over a mapped-type-derived
    ///   application such as `Record<string, number>[K]`, whose element-access
    ///   assignability diagnostics (`TS2862`/`TS2322`) the checker derives from
    ///   the structural index-signature form (`keyofAndIndexedAccess2`).
    pub(super) fn is_closed_cacheable_kind(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::KeyOf(operand)) => self.is_index_object_cacheable(operand),
            Some(TypeData::IndexAccess(obj, _)) => self.is_index_object_cacheable(obj),
            Some(TypeData::Application(_)) => self.is_application_body_non_mapped(type_id),
            _ => false,
        }
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
            Some(TypeData::Application(_)) => self.is_application_body_non_mapped(obj),
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

    /// Whether an `Application` type's base resolves to a non-mapped body, i.e.
    /// it is not a homomorphic mapped utility (`Partial`/`Readonly`/`Record`).
    fn is_application_body_non_mapped(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_id) = self.resolve_application_def_id(app.base) else {
            // Unresolvable base: keep opaque, do not cache.
            return false;
        };
        match self.resolver.resolve_lazy(def_id, self.interner) {
            Some(body) => !matches!(self.interner.lookup(body), Some(TypeData::Mapped(_))),
            // Body not resolvable by this resolver: be conservative.
            None => false,
        }
    }
}
