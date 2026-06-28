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
//!  - **Write gate** (kind-split): the meta-operation kinds
//!    (`IndexAccess`/`KeyOf`/`Application`) commit only from the checker's
//!    authoritative, context-free type-resolution pass (the
//!    `with_closed_eval_writes` plus `with_query_db` boundary, a *complete*
//!    resolver); a resolver-backed
//!    mid-relation/inference/narrowing evaluator runs against a *partial*
//!    resolver (lib/utility bodies in the `resolver_generation()==0` registration
//!    window not yet materialized) and can compute a definite-but-under-resolved
//!    meta-operation head that diverges from the resolved answer
//!    (`partialOfLargeAPIIsAbleToBeWorkedWith`); a closed `Conditional` is exempt
//!    and may commit from any top-level evaluation, because it has no operand
//!    whose deferred/under-resolved expansion the meta-operation kinds key on
//!    (its branch selection either resolves definitely or the run is already
//!    excluded by the `unresolved_def_seen`/`tainted`/limit gates); caching the
//!    closed conditionals the resolver-backed contexts compute and re-compute is
//!    the deep-recursion win, and reads stay open to every evaluator since a
//!    stored value is always a fully-resolved answer.
//!  - **Limit gate**: a run that returned a typed incomplete
//!    [`crate::evaluation::result::EvaluationResult`] verdict, or hit any
//!    legacy recursion/complexity limit (`deep_recursion_seen`, the `TS2589`
//!    depth machinery, or the `TS2590` union-too-complex flag), caches nothing
//!    — a cached read must never short-circuit an expansion the type system must
//!    continue in order to re-derive those diagnostics. A run that evaluated an
//!    application whose base `DefId` had no resolvable body
//!    (`unresolved_def_seen`) also caches nothing: its results are
//!    registration-window artifacts that would permanently shadow the answer
//!    derived after the real body registers.

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
        if !closed_eval_cache_enabled() {
            return None;
        }
        // A *limited-resolver* evaluator (the checker's first-pass
        // `TypeEnvironment` evaluation, whose `Lazy` resolution is intentionally
        // partial) must recompute a meta-operation
        // (`IndexAccess`/`KeyOf`/`Application`) rather than consume the
        // authoritative pass's stored result. A stored meta-operation result is a
        // *fully materialized* form (e.g. a generic interface instance such as
        // `Validator<NonNullable<string>>` flattened to its structural callable),
        // produced by the complete resolver. Feeding that materialized form into
        // the limited evaluator's in-flight inference — where a freshly recomputed
        // result keeps the operand in the deferred / application form an `infer`
        // match still recognizes — yields a different (under-reduced) answer. The
        // limited evaluator memoizes that answer further up, poisoning the
        // still-deferred outer application's evaluation, and a later authoritative
        // read then observes the poisoned form:
        // `RequiredKeys<V> = { … extends Validator<infer T> ? … }[keyof V]`
        // collapses to `never`, dropping required keys and flipping a downstream
        // conditional into a spurious `TS2322`. This is the read-side mirror of the
        // write gate, which already commits these meta-operation kinds only from
        // the authoritative (complete-resolver) pass (see `commit_closed_eval_writes`
        // and the `limited_resolver` field): the limited evaluator's results are
        // context-dependent, so it neither publishes nor consumes a materialized
        // meta-operation across the partial/complete boundary. A closed
        // `Conditional` stays readable (its stored value is always a fully-resolved
        // branch), and authoritative / plain query-backed evaluators keep reading
        // every kind, so the deep-recursion meta-operation reuse is preserved. The
        // cheap field check short-circuits before the eligibility walks below for
        // every non-limited evaluator.
        if self.limited_resolver
            && matches!(
                self.interner.lookup(type_id),
                Some(TypeData::IndexAccess(_, _) | TypeData::KeyOf(_) | TypeData::Application(_))
            )
        {
            return None;
        }
        if !self.is_closed_cacheable_kind(type_id)
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
        // A substitution-independent node's *final* result is a pure function of
        // `(TypeId, no_unchecked, exact_optional)` and the project's single fixed
        // resolver — but only once that resolver can resolve every
        // `Lazy`/`TypeQuery`/`Application` operand the node reaches. The
        // per-kind writer split below (see `authoritative` / `writable`) is what
        // keeps a partial-resolver evaluator from persisting an under-resolved
        // answer a sibling read would observe.
        let is_top_level = closed_eval_cache_enabled() && self.guard.depth() == 0;
        if !is_top_level
            || self.request_termination_kind.is_some()
            || self.recursion_limit_hit()
            || self.unresolved_def_seen()
            || (self.interner.is_union_too_complex() && !union_too_complex_before)
        {
            return;
        }
        // Whether this evaluator is the checker's authoritative, context-free
        // type-resolution pass (a *complete* resolver). Only that pass may write
        // the meta-operation kinds (`IndexAccess`/`KeyOf`/`Application`), because
        // a resolver-backed mid-relation/inference/narrowing evaluator runs
        // against a *partial* resolver (lib/utility bodies in the
        // `resolver_generation()==0` registration window not yet materialized) and
        // can compute a definite-but-under-resolved meta-operation head whose
        // value diverges from the resolved answer — the
        // `partialOfLargeAPIIsAbleToBeWorkedWith` `TS2322` regression (an
        // under-resolved `Partial<MyAPI>[keyof MyAPI]` write type rejecting a
        // well-typed assignment). A closed `Conditional` is exempt: it has no
        // operand whose deferred/under-resolved expansion the meta-operation
        // kinds key on — its branch selection either resolves to a definite,
        // resolver-stable answer or the run is already excluded by the
        // `unresolved_def_seen` / `tainted` / limit gates above (the
        // `defer_resolver_less_application_check` and bare-`UnresolvedTypeName`
        // deferrals both set `unresolved_def_seen`). Letting the resolver-backed
        // contexts cache *only* the closed conditionals they compute and
        // re-compute is the deep-recursion win (`AutoPath`/`MetaPath`/`Join`,
        // issue #13250) without the under-resolved-meta-operation hazard.
        let authoritative = self.closed_eval_writes_allowed && self.query_db.is_some();
        let no_unchecked = self.no_unchecked_indexed_access;
        // Collect first to avoid borrowing the per-evaluator cache while the
        // content query borrows the interner. A node whose own evaluation window
        // saw a recursion/limit event is in `tainted`; such a bounded result
        // must never enter the project-wide cache (it would permanently shadow
        // the complete answer a deeper-budget run derives), so it is excluded
        // here in addition to the whole-run `recursion_limit_hit` gate above.
        let entries: Vec<(TypeId, TypeId)> = self
            .cache
            .iter()
            .filter(|(node, _)| !node.is_intrinsic() && !self.tainted.contains(node))
            .map(|(&node, &node_result)| (node, node_result))
            .collect();
        for (node, node_result) in entries {
            // A non-authoritative (resolver-backed) evaluator may write only a
            // closed `Conditional` that *resolved* to a definite branch. If the
            // result is itself a `Conditional`, branch selection was deferred —
            // the `check`/`extends`-is-`Lazy`/`Application` deferral
            // (`evaluate_conditional`) that does *not* set `unresolved_def_seen`,
            // a resolver-state hazard the `(TypeId, …)` key does not capture — so
            // it must not persist (a later complete-resolver pass resolves the
            // same `TypeId` to a real branch). The authoritative pass has a
            // complete resolver, so its conditional results are always resolved.
            let writable = if authoritative {
                true
            } else {
                matches!(self.interner.lookup(node), Some(TypeData::Conditional(_)))
                    && !matches!(
                        self.interner.lookup(node_result),
                        Some(TypeData::Conditional(_))
                    )
            };
            if writable
                && self.is_closed_cacheable_kind(node)
                && !crate::type_queries::is_substitution_dependent_type(self.interner, node)
                && !self.has_unresolvable_type_query_operand(node)
            {
                self.interner
                    .insert_closed_eval_cache(node, no_unchecked, node_result);
            }
        }
    }

    /// Whether an `IndexAccess`/`KeyOf` entry's operand chain reaches a
    /// `TypeQuery` (`typeof X`) the current resolver cannot resolve yet.
    ///
    /// A `TypeQuery` operand resolves through mutable checker state
    /// (`symbol_types` fills in as declarations are checked), so an evaluation
    /// that ran before `X`'s value type was computed produces a deferred
    /// identity result. Committing that deferral would permanently shadow the
    /// resolvable answer for every later evaluator — e.g. `(typeof C)[number]`
    /// where `const C = [...A, 'z'] as const` is lowered through a type alias
    /// before `C`'s initializer is typed (kysely `BINARY_OPERATORS` family).
    ///
    /// This is a *write-side* gate only. Reads stay permissive: a stored value
    /// was produced by an authoritative run that could resolve the query, so
    /// serving it to a resolver that currently cannot is strictly better than
    /// a miss.
    fn has_unresolvable_type_query_operand(&self, type_id: TypeId) -> bool {
        let operand = match self.interner.lookup(type_id) {
            Some(TypeData::IndexAccess(obj, _) | TypeData::KeyOf(obj)) => obj,
            _ => return false,
        };
        self.operand_chain_has_unresolvable_type_query(operand, 0)
    }

    /// Recursive helper for [`Self::has_unresolvable_type_query_operand`]:
    /// walk nested `IndexAccess`/`KeyOf` operands and resolved `TypeQuery`
    /// bodies looking for a query the resolver cannot answer. The depth bound
    /// guards against pathological self-referential `typeof` chains.
    fn operand_chain_has_unresolvable_type_query(&self, obj: TypeId, depth: u32) -> bool {
        const MAX_OPERAND_CHAIN_DEPTH: u32 = 16;
        if depth >= MAX_OPERAND_CHAIN_DEPTH {
            return true;
        }
        match self.interner.lookup(obj) {
            Some(TypeData::TypeQuery(sym_ref)) => {
                match self.resolver.resolve_type_query(sym_ref, self.interner) {
                    Some(body) if body != obj => {
                        self.operand_chain_has_unresolvable_type_query(body, depth + 1)
                    }
                    _ => true,
                }
            }
            Some(TypeData::IndexAccess(inner, _) | TypeData::KeyOf(inner)) => {
                self.operand_chain_has_unresolvable_type_query(inner, depth + 1)
            }
            _ => false,
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
            // A `Conditional` reaches this gate only after the caller has already
            // proven the node is *not* substitution-dependent (the read path's
            // `try_closed_eval_read` and the write path's
            // `commit_closed_eval_writes` both require
            // `!is_substitution_dependent_type`, which is `true` for any node
            // whose structure contains a `TypeParameter`/`Infer`/`ThisType`/
            // `BoundParameter` — descending conditional `check`/`extends`/
            // `true`/`false` branches via `ChildPolicy::CONTENT_PREDICATE`).
            // The historic conditional exclusion (`body_has_conditional`) guarded
            // against `infer` placeholders binding against use-site
            // inference/narrowing/contextual state the `(TypeId, no_unchecked)`
            // key does not capture. With no `infer` anywhere in the structure
            // that hazard cannot arise: a closed conditional evaluates to a
            // definite answer that is a pure function of its `TypeId`, the
            // `no_unchecked` flag, and the project's single fixed resolver.
            //
            // This is the lever for the deferred deep-recursion families
            // (`AutoPath`/`MetaPath`/`Flatten`/`Join` in ts-toolbelt): each path
            // closes hundreds of distinct conditional `TypeId`s during
            // instantiation, and every fresh resolver-backed evaluator that meets
            // one re-walks it from scratch (the `closed_eval_cache` excluded the
            // conditional kind, the persistent eval memo is opt-out for
            // resolver-backed contexts, and the structural-inertness fixed point
            // only retires `result == type_id` self-maps). Caching the closed
            // conditional's result retires that recompute for every later
            // evaluator regardless of resolver.
            Some(TypeData::Conditional(_)) => true,
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
        // Routed through the project-wide `contains_conditional_cache` (see
        // `contains_conditional_type`) so the eligibility gate is amortized O(1)
        // per node rather than an O(subtree) walk on every cache-miss
        // evaluation. The cached walker enumerates children identically to the
        // generic `contains_type_matching(.., Conditional)` walk — both treat
        // `Lazy`/`Application` bases as opaque leaves — so the answer is
        // unchanged.
        crate::type_queries::contains_conditional_type(self.interner, type_id)
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
        self.is_index_object_cacheable_bounded(obj, 0)
    }

    /// Depth-bounded core of [`Self::is_index_object_cacheable`].
    ///
    /// The walk descends through `IndexAccess`/`KeyOf` operands, resolved `Lazy`
    /// alias bodies, and `Intersection` members. A `Lazy` alias whose body
    /// re-references the same alias through an `IndexAccess`/`KeyOf` (e.g. an
    /// `A = …A[K]…` shape reachable via cross-file import cycles) forms a
    /// multi-step cycle: each step alternates `Lazy -> body` and
    /// `IndexAccess -> operand`, so `body != obj` is satisfied at every step and
    /// the single-step check cannot break it. The [`MAX_DEF_DEPTH`] bound — the
    /// same alias-expansion limit the evaluator uses elsewhere — terminates such
    /// cycles. Exceeding it returns `false`, conservatively treating the operand
    /// as non-cacheable, which can never change a diagnostic since the
    /// substitution-independent closed-form cache is an optimization, not a
    /// semantic input.
    ///
    /// [`MAX_DEF_DEPTH`]: crate::limits::MAX_DEF_DEPTH
    fn is_index_object_cacheable_bounded(&self, obj: TypeId, depth: u32) -> bool {
        if depth >= crate::limits::MAX_DEF_DEPTH {
            return false;
        }
        match self.interner.lookup(obj) {
            Some(TypeData::Application(_)) => self.is_application_body_cacheable(obj),
            // A nested index access / keyof over a cacheable object stays fine.
            Some(TypeData::IndexAccess(inner_obj, _) | TypeData::KeyOf(inner_obj)) => {
                self.is_index_object_cacheable_bounded(inner_obj, depth + 1)
            }
            // Resolve a `Lazy` alias to decide on its body (e.g. `Dict =
            // Record<string, number>` resolves to a mapped/index-signature type).
            Some(TypeData::Lazy(def_id)) => match self.resolver.resolve_lazy(def_id, self.interner)
            {
                Some(body) if body != obj => {
                    self.is_index_object_cacheable_bounded(body, depth + 1)
                }
                _ => false,
            },
            // An intersection is safe only if every member is.
            Some(TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .all(|&m| self.is_index_object_cacheable_bounded(m, depth + 1)),
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
    use crate::caches::db::TypeApplicationEvalCache;
    use crate::caches::query_cache::QueryCache;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::evaluation::result::TerminationKind;

    fn evaluator(interner: &TypeInterner) -> TypeEvaluator<'_> {
        TypeEvaluator::new(interner)
    }

    /// A limited-resolver evaluator (the checker's intentionally-partial
    /// first-pass `TypeEnvironment` evaluation) must *recompute* a cacheable
    /// `IndexAccess`/`KeyOf` rather than consume the authoritative pass's stored,
    /// fully-materialized result — consuming it across the partial/complete
    /// boundary poisons its in-flight inference (a `propTypeValidatorInference`
    /// style false `TS2322`). A non-limited (authoritative / plain query-backed)
    /// evaluator keeps reading the cache, so the meta-operation reuse is
    /// preserved. The cache key (`no_unchecked_indexed_access`/`exact_optional`)
    /// is supplied by the same `QueryCache` for both store and read, so a hit is
    /// exactly the stored value.
    #[test]
    fn limited_resolver_does_not_read_cached_meta_operations() {
        let interner = TypeInterner::new();
        // Two cacheable meta-operations over a plain (non-index-signature) object.
        let idx = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        let keyof = interner.keyof(TypeId::OBJECT);
        let cache = QueryCache::new(&interner);
        cache.insert_closed_eval_cache(idx, false, TypeId::NUMBER);
        cache.insert_closed_eval_cache(keyof, false, TypeId::BOOLEAN);

        // Non-limited evaluator: reads the authoritative stored result.
        let authoritative = TypeEvaluator::new(&cache);
        assert_eq!(
            authoritative.try_closed_eval_read(idx),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            authoritative.try_closed_eval_read(keyof),
            Some(TypeId::BOOLEAN)
        );

        // Limited-resolver evaluator: must not consume the materialized result.
        let limited = TypeEvaluator::new(&cache).with_limited_resolver();
        assert_eq!(limited.try_closed_eval_read(idx), None);
        assert_eq!(limited.try_closed_eval_read(keyof), None);
    }

    /// A closed-eval write is publishable only when the request completed. The
    /// legacy `recursion_limit_hit` backstop remains below this gate, but an
    /// explicit typed incomplete verdict is already enough to reject the write.
    #[test]
    fn incomplete_request_verdict_blocks_closed_eval_write() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);

        let complete_node = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        let mut complete = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        complete.cache.insert(complete_node, TypeId::NUMBER);
        complete.commit_closed_eval_writes(false);
        assert_eq!(
            cache.lookup_closed_eval_cache(complete_node, false),
            Some(TypeId::NUMBER)
        );

        let incomplete_node = interner.keyof(TypeId::OBJECT);
        let mut incomplete = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        incomplete.cache.insert(incomplete_node, TypeId::BOOLEAN);
        incomplete.request_termination_kind = Some(TerminationKind::DepthExceeded);
        incomplete.commit_closed_eval_writes(false);
        assert_eq!(cache.lookup_closed_eval_cache(incomplete_node, false), None);
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

    /// A `Lazy` alias whose resolved body re-references the same alias through an
    /// `IndexAccess` forms a multi-step cycle (`A -> A[K] -> A -> …`). Each step
    /// alternates `Lazy -> body` and `IndexAccess -> operand`, so the single-step
    /// `body != obj` check is satisfied at every hop and cannot break it. Such
    /// shapes arise from cross-file import cycles. The cache-eligibility walk must
    /// terminate (returning the conservative "not cacheable") instead of
    /// overflowing the stack.
    #[test]
    fn cacheable_kinds_terminate_on_cyclic_alias_index_access() {
        /// Resolver whose single alias body is supplied after construction so it
        /// can reference its own `Lazy` node (`A = A[string]`).
        struct CyclicAliasResolver {
            def_id: DefId,
            body: TypeId,
        }
        impl TypeResolver for CyclicAliasResolver {
            fn resolve_ref(
                &self,
                _symbol: crate::types::SymbolRef,
                _interner: &dyn crate::caches::db::TypeDatabase,
            ) -> Option<TypeId> {
                None
            }
            fn resolve_lazy(
                &self,
                def_id: DefId,
                _interner: &dyn crate::caches::db::TypeDatabase,
            ) -> Option<TypeId> {
                (def_id == self.def_id).then_some(self.body)
            }
        }

        let interner = TypeInterner::new();
        let def_id = DefId(7);
        let lazy = interner.lazy(def_id);
        // `A = A[string]`: an index access whose operand is the alias itself.
        let body = interner.index_access(lazy, TypeId::STRING);
        let resolver = CyclicAliasResolver { def_id, body };
        let ev = TypeEvaluator::with_resolver(&interner, &resolver);

        // Must terminate (no stack overflow) and conservatively exclude the
        // cyclic shape from the substitution-independent cache.
        assert!(!ev.is_closed_cacheable_kind(body));
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
