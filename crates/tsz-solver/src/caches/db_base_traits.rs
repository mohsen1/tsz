use crate::types::TypeId;

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

    /// Parameters as they should DISPLAY for the shape interned as `id`: a
    /// parameter whose `optional` bit exists only for JS call-arity leniency
    /// renders as required (`tree: any`), matching tsc, which reserves `?`
    /// for a written `?`, an initializer, or a JSDoc optional marker. Shapes
    /// without a recorded mask borrow their params untouched. Both the
    /// diagnostics formatter and the declaration-emit printers consult this
    /// provided method, so the two display surfaces cannot drift (#17238).
    fn display_params_for_function_shape<'p>(
        &self,
        id: crate::types::FunctionShapeId,
        params: &'p [crate::types::ParamInfo],
    ) -> std::borrow::Cow<'p, [crate::types::ParamInfo]> {
        match self.function_shape_arity_optional_mask(id) {
            Some(mask) if mask.len() == params.len() => std::borrow::Cow::Owned(
                params
                    .iter()
                    .zip(mask.iter())
                    .map(|(p, &arity_only)| {
                        if arity_only {
                            crate::types::ParamInfo {
                                optional: false,
                                ..*p
                            }
                        } else {
                            *p
                        }
                    })
                    .collect(),
            ),
            _ => std::borrow::Cow::Borrowed(params),
        }
    }
}
