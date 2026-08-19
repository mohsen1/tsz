use super::db::TypeDatabase;
use crate::def::DefId;
use crate::types::{TypeData, TypeId, TypeParamInfo};
use std::sync::Arc;

/// Redundant-supertype reduction of an intersection type, for diagnostic
/// DISPLAY only. It does not change any interned identity — in particular it
/// must not feed back into `try_merge_objects_in_intersection`'s raw,
/// non-distributing property-type merge, which existing intersection-origin
/// tracking (`get_merged_intersection_origin`) depends on staying raw.
///
/// `A & B` collapses to `A` when `B` is a union that literally contains `A`
/// as a member. That is exactly the shape an `exactOptionalPropertyTypes`
/// property-write/read merge can leave unreduced (`boolean & (boolean |
/// undefined)` instead of tsc's plain `boolean`): one member's write type is
/// always literally a member of the other member's own union, so a
/// `TypeId`-membership check is enough here — no general subtyping needed.
/// The default body is expressed purely in terms of existing `TypeDatabase`
/// queries, so every implementor gets it for free via the blanket impl below.
pub trait IntersectionDisplayReduction: TypeDatabase {
    fn intersection_reduced_for_display(&self, id: TypeId) -> TypeId {
        let Some(TypeData::Intersection(list_id)) = self.lookup(id) else {
            return id;
        };
        let members = self.type_list(list_id);
        let is_redundant_supertype = |member: TypeId| match self.lookup(member) {
            Some(TypeData::Union(union_list)) => {
                let union_members = self.type_list(union_list);
                members
                    .iter()
                    .any(|&other| other != member && union_members.contains(&other))
            }
            _ => false,
        };
        let kept: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| !is_redundant_supertype(m))
            .collect();
        match kept.len() {
            1 => kept[0],
            n if n == members.len() => id,
            _ => kept
                .into_iter()
                .reduce(|acc, m| self.intersect_types_raw2(acc, m))
                .unwrap_or(id),
        }
    }
}

impl<T: TypeDatabase + ?Sized> IntersectionDisplayReduction for T {}

/// Construction capability for replaying an unsimplified intersection member
/// list without producing a new diagnostic signal.
///
/// This is intentionally narrower than [`crate::caches::db::TypeDatabase`].
/// Structural graph replay needs to preserve an existing intersection without
/// invoking semantic normalization or repeatedly folding two-member helpers.
pub trait TypeRawIntersectionConstruction {
    /// Flatten and order-deduplicate `members`, then intern the remaining raw
    /// intersection in `O(N)` time without subtype reduction, object merging,
    /// or mutation of the interner-wide union-complexity flag.
    fn intersect_types_raw_for_replay(&self, members: Vec<TypeId>) -> TypeId;
}

/// Cache hooks for solver type-content traversal predicates.
///
/// The answers are stable for a `TypeId` within one interner because interned
/// type data is immutable. Keeping these hooks out of [`TypeDatabase`] makes
/// traversal-cache capability visible as a narrower contract than
/// [`crate::caches::db::TypeDatabase`].
pub trait TypePredicateCache {
    /// Look up a cached `contains_this_type(type_id)` result if available.
    ///
    /// Default impl returns `None` (no caching). The primary implementation
    /// on `TypeInterner` consults a project-wide `DashMap`; the `QueryCache`
    /// delegate forwards through to the interner so all sharing callers hit
    /// the same cache.
    fn contains_this_type_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_this_type(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_this_type_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_infer_types_db(type_id)` result if available.
    fn contains_infer_types_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_infer_types_db(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_infer_types_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_type_query_db(type_id)` result if available.
    fn contains_type_query_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_type_query_db(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_type_query_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached full-reachability `contains_type_query` result if
    /// available. Distinct from [`Self::contains_type_query_cached`]: this slot
    /// memoizes the `ChildPolicy::FULL` walk used to gate `collect_type_queries`.
    fn contains_type_query_full_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of the full-reachability `contains_type_query` walk in
    /// the shared interner cache. Default impl is a no-op.
    fn set_contains_type_query_full_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_never_type_db(type_id)` result if available.
    /// Default impl returns `None` (no caching).
    fn contains_never_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_never_type_db(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_never_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_error_type_db(type_id)` result if available.
    /// Default impl returns `None` (no caching).
    fn contains_error_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_error_type_db(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_error_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached free-type-parameter containment result if available.
    /// Default impl returns `None` (no caching).
    fn contains_free_type_params_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record a free-type-parameter containment result in the shared interner
    /// cache. Default impl is a no-op.
    fn set_contains_free_type_params_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached extractable-type-parameter containment result (the
    /// reachability gate on `extract_type_params_from_type`) if available.
    /// Default impl returns `None` (no caching).
    fn contains_extractable_type_params_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record an extractable-type-parameter containment result in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_extractable_type_params_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached free-`infer` containment result (the `FREE_INFER`
    /// policy walk backing `contains_free_infer_types`) if available. Default
    /// impl returns `None` (no caching).
    fn contains_free_infer_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record a free-`infer` containment result in the shared interner cache.
    /// Default impl is a no-op.
    fn set_contains_free_infer_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_type_parameters_db(type_id)` result if
    /// available. Default impl returns `None` (no caching).
    fn contains_type_params_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_type_parameters_db(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_type_params_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_lazy_or_recursive_db(type_id)` result.
    /// Default impl returns `None` (no caching).
    fn contains_lazy_or_recursive_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_lazy_or_recursive_db(type_id)` in the
    /// shared interner cache. Default impl is a no-op.
    fn set_contains_lazy_or_recursive_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_unresolved_application(type_id)` result.
    /// Default impl returns `None` (no caching).
    fn contains_unresolved_application_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_unresolved_application(type_id)` in the
    /// shared interner cache. Default impl is a no-op.
    fn set_contains_unresolved_application_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `is_resolver_dependent_type(type_id)` result.
    /// Default impl returns `None` (no caching).
    fn contains_resolver_dependent_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `is_resolver_dependent_type(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_contains_resolver_dependent_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `is_structurally_eval_inert(type_id)` result (whether the
    /// type evaluates to itself under every evaluator and resolver). Default impl
    /// returns `None` (no caching).
    fn structurally_eval_inert_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `is_structurally_eval_inert(type_id)` in the shared
    /// interner cache. Default impl is a no-op.
    fn set_structurally_eval_inert_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached alias-opaque `contains Conditional` walk result, used by
    /// the `closed_eval_cache` eligibility gate. Default impl returns `None`.
    fn contains_conditional_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of the alias-opaque `contains Conditional` walk in the
    /// shared interner cache. Default impl is a no-op.
    fn set_contains_conditional_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached result for the narrow
    /// `visitor_predicates::contains_type_parameters` walk
    /// (`TypeParameter | Infer`). The answer is a pure function of each
    /// visited `TypeId`. Default impl returns `None` (no caching).
    fn contains_param_or_infer_root_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record a narrow `visitor_predicates::contains_type_parameters` walk
    /// result. Default no-op.
    fn set_contains_param_or_infer_root_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached root result for the depth-limited
    /// `contains_generic_type_parameters_db` walk. Default `None`.
    fn contains_generic_params_root_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record a root result of the depth-limited
    /// `contains_generic_type_parameters_db` walk. Default no-op.
    fn set_contains_generic_params_root_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `is_generic_type_with_union_constraint(type_id)` result.
    /// Default `None`.
    fn is_generic_with_union_constraint_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record an `is_generic_type_with_union_constraint(type_id)` result.
    /// Default no-op.
    fn set_is_generic_with_union_constraint_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `is_generic_type_without_nullable_constraint(type_id)`
    /// result. Default `None`.
    fn is_generic_without_nullable_constraint_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record an `is_generic_type_without_nullable_constraint(type_id)` result.
    /// Default no-op.
    fn set_is_generic_without_nullable_constraint_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached result of the evaluator's `type_contains_infer`
    /// walk (structural `Infer` nodes only; descends `Application` bases).
    /// Default impl returns `None` (no caching).
    fn eval_contains_infer_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record a result of the evaluator's `type_contains_infer` walk.
    /// Only cycle-untainted (fully explored) results may be stored.
    /// Default impl is a no-op.
    fn set_eval_contains_infer_cache(&self, _type_id: TypeId, _result: bool) {}

    /// Look up a cached `contains_file_relative_content_db(type_id)` result.
    /// Default impl returns `None` (no caching).
    fn contains_file_relative_cached(&self, _type_id: TypeId) -> Option<bool> {
        None
    }

    /// Record the result of `contains_file_relative_content_db(type_id)` in
    /// the shared interner cache. Default impl is a no-op.
    fn set_contains_file_relative_cache(&self, _type_id: TypeId, _result: bool) {}
}

/// Narrow signal for tuple-size overflow discovered during solver evaluation.
///
/// Keeping this out of [`crate::caches::db::TypeDatabase`] avoids growing the
/// general storage interface for a diagnostic side channel used by large tuple
/// synthesis.
pub trait TypeTupleLimitSignal {
    /// Atomically read and clear the "tuple too large" flag.
    ///
    /// Returns `true` if a tuple spread was aborted because the synthesized
    /// element count would exceed `MAX_REPRESENTABLE_TUPLE_LENGTH`. The checker
    /// uses this to emit `TS2799` instead of `TS2589`.
    fn take_tuple_too_large(&self) -> bool {
        false
    }

    /// Mark that a tuple spread synthesis was aborted due to the element-count limit.
    fn mark_tuple_too_large(&self) {}

    /// Peek the sticky `tuple_too_large` flag without clearing it (mirrors
    /// `is_union_too_complex`). Used by the project-wide instantiation cache's
    /// limit gate. Default `false`.
    fn is_tuple_too_large(&self) -> bool {
        false
    }

    /// Peek the interner-poison flag (type-count budget exceeded -> new
    /// interning degrades to `TypeId::ERROR`). Used by the project-wide
    /// instantiation cache's limit gate. Default `false`.
    fn is_poisoned(&self) -> bool {
        false
    }
}

/// Read-only compiler option hooks used by solver type operations.
///
/// Keeping option queries separate from [`crate::caches::db::TypeDatabase`]
/// lets helpers depend on configuration without also receiving construction,
/// cache, or provenance capabilities.
pub trait TypeCompilerOptions {
    /// Whether indexed access reads should include `undefined`.
    fn no_unchecked_indexed_access(&self) -> bool {
        false
    }

    /// Whether `exactOptionalPropertyTypes` is enabled.
    ///
    /// Inference matching uses this to distinguish synthetic `| undefined`
    /// from optional markers vs. explicit `| undefined` in user-written types.
    fn exact_optional_property_types(&self) -> bool {
        false
    }

    /// Whether `strictNullChecks` is enabled.
    ///
    /// Gates whether an optional member's access/call/inference type carries
    /// `| undefined` (tsc's `addOptionality` is strictNullChecks-gated). The
    /// default is `true` so a backend without wired options never wrongly
    /// strips `undefined` in strict code.
    fn strict_null_checks(&self) -> bool {
        true
    }
}

/// Storage capability for the per-parameter "arity-only optional" display
/// masks of JS untyped signatures (#17227).
///
/// A bare, unannotated parameter in a JS file is `optional` in its
/// `FunctionShape` only so call-arity checking stays lenient; `tsc` never
/// displays it with `?`. The mask records which parameters owe `optional` to
/// that rule so the printer renders them as required, while arity and
/// subtyping keep reading `optional` unchanged. Kept out of
/// [`crate::caches::db::TypeDatabase`]'s own method list as a narrower
/// facet, following the other capability supertraits.
pub trait JsSignatureDisplaySource {
    /// Intern a function type carrying an arity-only-optional display mask
    /// (`mask[i]` flags `shape.params[i]`). Implementations without mask
    /// storage fall back to the plain function intern (today's display).
    fn function_with_arity_optional_mask(
        &self,
        shape: crate::types::FunctionShape,
        mask: &[bool],
    ) -> TypeId;

    /// The mask recorded for `id`, or `None` for shapes interned without
    /// one. `Some(mask)[i] == true` means `params[i]`'s `optional` bit
    /// exists only for JS call-arity leniency and displays as required.
    fn function_shape_arity_optional_mask(
        &self,
        id: crate::types::FunctionShapeId,
    ) -> Option<std::sync::Arc<[bool]>>;
}

/// Per-file cache hooks for evaluated generic applications.
///
/// The application-eval cache is keyed by `(DefId, args,
/// no_unchecked_indexed_access, exact_optional_property_types)`. Use it only
/// from authoritative full-resolver contexts; limited/noop resolvers can skip
/// fallback behavior that is part of recursive and inference parity. Keeping
/// this separate from [`TypeDatabase`] avoids growing the storage interface.
pub trait TypeApplicationEvalCache {
    /// Project-wide (#14345) instantiation result lookup. Default `None`.
    fn lookup_proto_instantiation_cache(
        &self,
        _key: &crate::caches::instantiation_cache::InstantiationCacheKey,
    ) -> Option<TypeId> {
        None
    }

    /// Project-wide (#14345) instantiation result store. Default no-op.
    fn insert_proto_instantiation_cache(
        &self,
        _key: crate::caches::instantiation_cache::InstantiationCacheKey,
        _result: TypeId,
    ) {
    }

    /// Look up a shared cache entry for evaluated generic applications.
    ///
    /// The default returns `None` so raw `TypeInterner` backends and tests opt
    /// out.
    fn lookup_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store an evaluated generic application result in the shared per-file
    /// cache. The default is a no-op so non-cache backends opt out.
    fn insert_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Whether `type_id` is a registered provisional (mid-build) class
    /// instance snapshot (#16055; semantics in
    /// `intern/core/interner/provisional_registry.rs`). Default `None`.
    fn provisional_class_instance(
        &self,
        _type_id: TypeId,
    ) -> Option<(DefId, Arc<[TypeParamInfo]>)> {
        None
    }

    /// Register `type_id` as the provisional class-instance snapshot of
    /// `def_id` (#16055). Default no-op.
    fn register_provisional_class_instance(
        &self,
        _type_id: TypeId,
        _def_id: DefId,
        _params: Arc<[TypeParamInfo]>,
    ) {
    }

    /// Drop every provisional class-instance registration for `def_id` —
    /// called when the class publishes its completed instance. Default no-op.
    fn unregister_provisional_class_instances_for_def(&self, _def_id: DefId) {}

    /// Drop every cached eval-family entry that depends on `def_id`.
    ///
    /// This includes application-eval entries keyed by the def directly or by
    /// lazy refs in their args/results, ordinary eval-memo entries whose key or
    /// result closure mentions the def, and closed-eval entries with the same
    /// dependency. The concrete `QueryCache` implementation also invalidates
    /// shared eval-family entries when a shared cache is attached.
    ///
    /// Called when a definition body is (re)registered with different
    /// content: results computed under the previous body (or before any body
    /// existed) are stale and must be recomputed on the next query. The
    /// default is a no-op so raw `TypeInterner` backends and tests opt out.
    fn invalidate_application_eval_cache_for_def(&self, _def_id: DefId) {}

    /// Look up a persisted evaluation memo entry for `type_id`.
    ///
    /// Backed by the same per-file (plus shared cross-file) eval cache that
    /// `evaluate_type_with_options` consults at its top-level boundary; this
    /// hook lets a *plain* evaluator (`NoopResolver`, default mode flags)
    /// read those entries at nested nodes too, instead of re-walking
    /// subtrees an earlier evaluator in the same file scope already
    /// evaluated (issue #13097). Every stored entry is a clean
    /// (limit-untainted) result keyed by
    /// `(TypeId, no_unchecked_indexed_access, exact_optional_property_types)`
    /// and written only from plain evaluators, so serving it at a nested
    /// node is the same semantic operation the top-level boundary already
    /// performs. Default returns `None` so raw `TypeInterner` backends and
    /// tests opt out.
    fn lookup_eval_memo(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store a clean evaluation result in the persistent eval memo.
    ///
    /// Write-through counterpart of [`Self::lookup_eval_memo`], called from
    /// a plain evaluator's memo insert when the entry's evaluation window
    /// saw no limit event (the per-entry taint discrimination of issue
    /// #13241) and no union-complexity overflow. First write wins, matching
    /// the boundary drain's `or_insert`. Default is a no-op.
    fn insert_eval_memo(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Look up a cached evaluation result for a *closed* type (one with no free
    /// type parameters, `this`, `infer`, or type-query operands).
    ///
    /// Evaluating a closed type is resolver-independent and deterministic per
    /// interner, so the result can be memoized project-wide and reused across
    /// the many fresh `TypeEvaluator` instances created during instantiation
    /// (`evaluate_index_access`, `evaluate_keyof`, …). The key is keyed by the
    /// `TypeId` plus both option flags, which can change `T[K]` results.
    /// Default returns `None` so non-cache backends opt out.
    fn lookup_closed_eval_cache(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store an evaluation result for a closed type. Default is a no-op.
    fn insert_closed_eval_cache(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Look up a cached *conditional-branch* subtype verdict — whether
    /// `check <: extends` for the purpose of selecting a conditional type's
    /// branch (issues #8356 / #13097).
    ///
    /// This is a distinct relation from plain subtyping: the conditional
    /// branch probe applies tsc's conditional-only fast paths (the global
    /// `Function` intrinsic satisfying a callable target; primitives never
    /// extending `Function`), so its verdict must never be served from — or
    /// stored into — the ordinary subtype relation cache. Only *definitive*
    /// verdicts (`Holds`/`Fails`) that consumed no unregistered `Lazy` body,
    /// took no depth bail, and tripped no recursion/iteration limit are
    /// persisted, so a stored verdict is stable for `(check, extends,
    /// no_unchecked_indexed_access, exact_optional_property_types)` across the
    /// fresh `TypeEvaluator` instances instantiation spins up. Default returns
    /// `None` so non-cache backends opt out.
    fn lookup_conditional_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
    ) -> Option<bool> {
        None
    }

    /// Store a definitive conditional-branch subtype verdict. Default is a
    /// no-op. See [`Self::lookup_conditional_branch_verdict`] for the stability
    /// gates the caller must enforce before publishing.
    fn insert_conditional_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
        _verdict: bool,
    ) {
    }

    /// Look up a cached result for tsc's permissive-instantiation
    /// false-branch gate (`getConditionalType`).
    ///
    /// The key is the original `(check, extends)` pair plus both compiler
    /// option bits. The caller must publish only when the helper's instantiated
    /// permissive relation was certified by the conditional-branch verdict cache
    /// and the surrounding evaluation request stayed stable. Default returns
    /// `None` so raw-interner backends opt out.
    fn lookup_permissive_false_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
    ) -> Option<bool> {
        None
    }

    /// Store a cached result for tsc's permissive-instantiation false-branch
    /// gate. Default is a no-op. See
    /// [`Self::lookup_permissive_false_branch_verdict`] for the required
    /// publication gates.
    fn insert_permissive_false_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
        _verdict: bool,
    ) {
    }
}
