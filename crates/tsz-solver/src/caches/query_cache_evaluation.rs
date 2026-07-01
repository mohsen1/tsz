//! Cache-aware evaluation helpers for `QueryCache`.

use crate::caches::db::TypeDatabase;
use crate::caches::query_cache::QueryCache;
use crate::def::DefId;
use crate::def::DefinitionStore;
use crate::def::resolver::NoopResolver;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled;
use crate::relations::subtype::TypeResolver;
use crate::types::{SymbolRef, TypeId, TypeParamInfo};
use tsz_binder::SymbolId;

/// A `query_db`-backed evaluator: a `TypeEvaluator` with the `NoopResolver`
/// (matching `evaluate_type_with_options`) whose cross-call caches are wired to
/// a `QueryCache`. See [`QueryCache::query_backed_evaluator`].
pub(crate) type QueryBackedEvaluator<'a> = TypeEvaluator<'a, NoopResolver>;

/// A resolver-backed query evaluator for the dormant instantiation re-reduce
/// path. It may read the resolver-independent application cache but must not
/// write resolver-dependent answers into it.
pub(crate) type StoreBackedQueryEvaluator<'eval, 'cache> = TypeEvaluator<'eval, QueryCache<'cache>>;

/// An evaluator whose resolver is the arena-invariant, store-only shim
/// [`StoreOnlyResolver`]. See [`QueryCache::store_resolver_backed_evaluator`].
pub(crate) type StoreResolverBackedEvaluator<'r> = TypeEvaluator<'r, StoreOnlyResolver<'r>>;

/// An arena-INVARIANT resolver shim backed only by the program-global
/// [`DefinitionStore`] (issue #14344 / #14345, default-OFF behind
/// `TSZ_INST_RESOLVER_REREDUCE`).
///
/// It overrides ONLY the `DefId`-keyed resolution methods whose answer is a
/// pure function of `(DefId)` against the shared store — `resolve_lazy`
/// (`store.get_body(canonical_def_id(def))`), `canonical_def_id`,
/// `defs_are_equivalent`, and `augmented_base_body_for_symbol`. It implements
/// NONE of the per-arena `TypeEnvironment` maps (`symbol_to_def`,
/// `typeof_value_types`, `class_instance_types`, `def_to_symbol`, …), which are
/// arena-DEPENDENT and would reintroduce the exact cross-arena divergence the
/// re-reduce pin guards (see #14344 decl-identity-through-arena-copy). The
/// `DefId -> body` lookup is a flat, arena-invariant store read, so a
/// `resolve_lazy` answer is the same regardless of which arena's evaluator asks.
///
/// Used ONLY at the instantiation-time re-reduce of a cross-arena
/// `Lazy`/`Application` base (`instantiate_index_access`): the cross-arena
/// `URItoKindN` registry interface materializes to its frozen empty-Object
/// snapshot here, at which point the published home-symbol redirect
/// (`redirect_empty_augmented_base_index`) re-indexes the populated home body.
pub(crate) struct StoreOnlyResolver<'a> {
    store: &'a DefinitionStore,
}

impl<'a> StoreOnlyResolver<'a> {
    pub(crate) const fn new(store: &'a DefinitionStore) -> Self {
        Self { store }
    }
}

impl TypeResolver for StoreOnlyResolver<'_> {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        // No per-arena symbol→type map: a `SymbolRef` is an arena-local handle
        // with no stable home in the shared store, so this shim never answers it.
        None
    }

    /// Resolve a `Lazy(DefId)` to its registered body through the shared store.
    ///
    /// `store.get_body(canonical_def_id(def))` is a flat lookup keyed on the
    /// arena-invariant `DefId` home (alias-forward-canonicalized), so the
    /// answer is the same in every arena. This is what lets the cross-arena
    /// `URItoKindN` base materialize to its empty-Object snapshot at the
    /// re-reduce site.
    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.store.get_body(self.store.canonical_def_id(def_id))
    }

    /// No on-demand forcing side effect, so the pure-lookup entry point is the
    /// same flat store read as [`Self::resolve_lazy`].
    fn resolve_lazy_lookup_only(
        &self,
        def_id: DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.resolve_lazy(def_id, interner)
    }

    /// Type parameters for a `DefId`, read flat from the shared store under the
    /// alias-forward-canonicalized home key.
    ///
    /// `store.get_type_params(canonical_def_id(def))` is the parameter-list
    /// analogue of [`Self::resolve_lazy`]'s body lookup: both are pure functions
    /// of the arena-invariant `DefId` home, so the answer is the same in every
    /// arena. Overriding this lets `try_expand_application`
    /// (`infer_matching.rs`) reach its `instantiate_generic_cached` step for a
    /// cross-arena `Lazy(DefId)` base — without it the trait default returns
    /// `None`, so expansion bails BEFORE the body is even resolved and the
    /// HKT-carried inner parameter positions never get an inference candidate
    /// (issue #14344 / #14345, default-OFF behind `TSZ_INFER_HKT_REDUCE`).
    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.store
            .get_type_params(self.store.canonical_def_id(def_id))
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        self.store.canonical_def_id(def_id)
    }

    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        if a == b {
            return true;
        }
        self.store.defs_have_same_decl_site(a, b)
            || self
                .store
                .get_symbol_id(a)
                .zip(self.store.get_symbol_id(b))
                .is_some_and(|(sa, sb)| sa == sb)
    }

    fn augmented_base_body_for_symbol(&self, symbol_id: u32) -> Option<TypeId> {
        // Same published-edge redirect as `QueryCache`/`TypeEnvironment`: map a
        // frozen empty pre-merge snapshot's home symbol to the home `DefId`
        // whose merged body is published. Gated structurally on the edge (only
        // recorded when the redirect flag is ON), so flag-OFF returns `None`.
        let home_def = self.store.augmented_base_body_def_for_symbol(symbol_id)?;
        self.store.get_body(home_def)
    }
}

/// A DELEGATING resolver used ONLY at the generic-call inference site: it
/// AUGMENTS the original per-arena `query_db` resolver with the arena-invariant
/// store-backed `Lazy(DefId)` reduction, instead of SWAPPING it out (issue
/// #14344 / #14345, default-OFF behind `TSZ_INFER_HKT_REDUCE`).
///
/// The earlier inference shim set `infer_ctx.resolver` to a bare
/// [`StoreOnlyResolver`]. That dropped EVERY per-arena `TypeResolver` answer
/// for the whole inference pass — `symbol_to_def_id`, `resolve_ref`,
/// `get_type_param_variance` (the source of `compute_application_variances`),
/// `canonical_def_id`, etc. all fell to the trait defaults returning `None` /
/// identity. Losing the variance answer collapsed inference variance to
/// COVARIANT, so under parallel evaluation a not-yet-collapsed wide HKT union
/// (`Kind2<keyof URItoKind2<any, any>, R, B>`) could leak non-deterministically
/// as a false positive (`src/ReaderIO.ts:428`, `src/ReaderTask.ts:606`,
/// RAYON=4 only).
///
/// This wrapper instead overrides ONLY the three `DefId`-keyed store reductions
/// (`resolve_lazy`, `resolve_lazy_lookup_only`, `get_lazy_type_params`) — the
/// arena-invariant flat store reads that the cross-arena `Lazy(URItoKindN)`
/// base needs to expand — and DELEGATES every other `TypeResolver` method to
/// the wrapped original `query_db` resolver. Per-arena state and correct
/// variance are therefore preserved, eliminating the parallel wide-union leak
/// while keeping the HKT-`Lazy` reduction that produces the genuine clears.
///
/// Distinct from [`StoreOnlyResolver`], which is the eval-memo-sound store-only
/// path used at the index-access re-reduce (Option B); that site intentionally
/// keeps the store-only + limited-resolver discipline and is NOT changed here.
pub(crate) struct DelegatingHktResolver<'a> {
    store: &'a DefinitionStore,
    inner: &'a dyn TypeResolver,
}

impl<'a> DelegatingHktResolver<'a> {
    pub(crate) const fn new(store: &'a DefinitionStore, inner: &'a dyn TypeResolver) -> Self {
        Self { store, inner }
    }
}

impl TypeResolver for DelegatingHktResolver<'_> {
    // ---- Store-backed overrides (the only methods that diverge from `inner`) ----

    /// Resolve a `Lazy(DefId)` to its registered body through the shared store,
    /// keyed on the alias-forward-canonicalized arena-invariant home. This is
    /// what lets the cross-arena `URItoKindN` base materialize at the inference
    /// site so HKT expansion produces an inference candidate.
    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.store.get_body(self.store.canonical_def_id(def_id))
    }

    /// No on-demand forcing side effect, so the pure-lookup entry point is the
    /// same flat store read as [`Self::resolve_lazy`].
    fn resolve_lazy_lookup_only(
        &self,
        def_id: DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.resolve_lazy(def_id, interner)
    }

    /// Type parameters for a `DefId`, read flat from the shared store under the
    /// alias-forward-canonicalized home key, so `try_expand_application` reaches
    /// its `instantiate_generic_cached` step for a cross-arena `Lazy(DefId)`
    /// base instead of bailing before the body is resolved.
    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        self.store
            .get_type_params(self.store.canonical_def_id(def_id))
    }

    // ---- Everything else delegates to the wrapped per-arena resolver ----

    fn resolver_generation(&self) -> u64 {
        self.inner.resolver_generation()
    }

    fn is_noop(&self) -> bool {
        self.inner.is_noop()
    }

    fn resolve_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.resolve_ref(symbol, interner)
    }

    fn resolve_symbol_ref(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.resolve_symbol_ref(symbol, interner)
    }

    fn resolve_type_query(&self, symbol: SymbolRef, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.resolve_type_query(symbol, interner)
    }

    fn get_type_params(&self, symbol: SymbolRef) -> Option<Vec<TypeParamInfo>> {
        self.inner.get_type_params(symbol)
    }

    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.inner.def_to_symbol_id(def_id)
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        self.inner.canonical_def_id(def_id)
    }

    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        self.inner.defs_are_equivalent(a, b)
    }

    fn symbol_to_def_id(&self, symbol: SymbolRef) -> Option<DefId> {
        self.inner.symbol_to_def_id(symbol)
    }

    fn augmented_base_body_for_symbol(&self, symbol_id: u32) -> Option<TypeId> {
        self.inner.augmented_base_body_for_symbol(symbol_id)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        self.inner.get_def_kind(def_id)
    }

    fn get_def_name(&self, def_id: DefId) -> Option<tsz_common::interner::Atom> {
        self.inner.get_def_name(def_id)
    }

    fn is_builtin_readonly_array_def(&self, def_id: DefId) -> bool {
        self.inner.is_builtin_readonly_array_def(def_id)
    }

    fn is_actual_or_cloned_lib_def(&self, def_id: DefId) -> bool {
        self.inner.is_actual_or_cloned_lib_def(def_id)
    }

    fn is_unresolved_import_def(&self, def_id: DefId) -> bool {
        self.inner.is_unresolved_import_def(def_id)
    }

    fn resolve_unresolved_type_name(&self, name: &str) -> Option<DefId> {
        self.inner.resolve_unresolved_type_name(name)
    }

    fn resolve_well_known_symbol_name(&self, name: &str) -> Option<SymbolRef> {
        self.inner.resolve_well_known_symbol_name(name)
    }

    fn well_known_symbol_name_for_ref(&self, symbol: SymbolRef) -> Option<&str> {
        self.inner.well_known_symbol_name_for_ref(symbol)
    }

    fn get_boxed_type(&self, kind: crate::types::IntrinsicKind) -> Option<TypeId> {
        self.inner.get_boxed_type(kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: crate::types::IntrinsicKind) -> bool {
        self.inner.is_boxed_def_id(def_id, kind)
    }

    fn is_boxed_type_id(&self, type_id: TypeId, kind: crate::types::IntrinsicKind) -> bool {
        self.inner.is_boxed_type_id(type_id, kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.inner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.inner.get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.inner.get_readonly_array_base_type()
    }

    fn is_numeric_enum(&self, def_id: DefId) -> bool {
        self.inner.is_numeric_enum(def_id)
    }

    fn is_enum_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> bool {
        self.inner.is_enum_type(type_id, interner)
    }

    fn get_enum_parent_def_id(&self, member_def_id: DefId) -> Option<DefId> {
        self.inner.get_enum_parent_def_id(member_def_id)
    }

    fn get_enum_member_def_ids(&self, parent_def_id: DefId) -> Vec<DefId> {
        self.inner.get_enum_member_def_ids(parent_def_id)
    }

    fn is_user_enum_def(&self, def_id: DefId) -> bool {
        self.inner.is_user_enum_def(def_id)
    }

    fn get_enum_namespace_type(&self, def_id: DefId) -> Option<TypeId> {
        self.inner.get_enum_namespace_type(def_id)
    }

    fn get_class_extends(&self, def_id: DefId) -> Option<DefId> {
        self.inner.get_class_extends(def_id)
    }

    fn resolve_this_type(&self, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.resolve_this_type(interner)
    }

    fn class_def_for_instance_type(&self, type_id: TypeId) -> Option<DefId> {
        self.inner.class_def_for_instance_type(type_id)
    }

    fn def_for_type(&self, type_id: TypeId) -> Option<DefId> {
        self.inner.def_for_type(type_id)
    }

    fn get_base_type(&self, type_id: TypeId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.get_base_type(type_id, interner)
    }

    fn get_heritage_instantiation(&self, derived: DefId, target: DefId) -> Option<TypeId> {
        self.inner.get_heritage_instantiation(derived, target)
    }

    fn get_type_param_variance(
        &self,
        def_id: DefId,
    ) -> Option<std::sync::Arc<[crate::types::Variance]>> {
        self.inner.get_type_param_variance(def_id)
    }

    fn get_def_raw_body(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        self.inner.get_def_raw_body(def_id, interner)
    }

    fn is_genuine_unknown_alias_body(&self, def_id: DefId, interner: &dyn TypeDatabase) -> bool {
        self.inner.is_genuine_unknown_alias_body(def_id, interner)
    }

    fn def_is_non_program(&self, def_id: DefId) -> bool {
        self.inner.def_is_non_program(def_id)
    }
}

impl<'a> QueryCache<'a> {
    /// Build a `TypeEvaluator` wired to this cache's cross-call instantiation
    /// and application-eval caches.
    ///
    /// The sub-evaluation entry points (`evaluate_conditional`, `evaluate_keyof`,
    /// `evaluate_mapped`, `evaluate_index_access_with_options`) otherwise fall
    /// through to the `QueryDatabase` trait defaults, which construct a fresh
    /// `TypeEvaluator` with `query_db = None`. That strips the cross-call
    /// instantiation cache (`#12019`) at the entry boundary, so recursive
    /// utility expansion re-walks the same `(body, substitution)` pairs on every
    /// call. Threading `self` as the `query_db` lets those entry points share the
    /// same memoized walks the top-level `evaluate_type_with_options` path
    /// already uses. The resolver stays `Noop` to match that path exactly; only
    /// caching behavior changes, never the computed result.
    pub(crate) fn query_backed_evaluator(&self) -> QueryBackedEvaluator<'_> {
        TypeEvaluator::new(self as &dyn TypeDatabase).with_query_db(self)
    }

    /// Build a resolver-backed evaluator only when the staged
    /// instantiation-time re-reduce gate is active and this cache carries a
    /// shared `DefinitionStore`.
    pub(crate) fn store_backed_rereduce_evaluator<'eval>(
        &'eval self,
    ) -> Option<StoreBackedQueryEvaluator<'eval, 'a>> {
        if !inst_resolver_rereduce_enabled() || !self.has_definition_store() {
            return None;
        }
        Some(
            TypeEvaluator::with_resolver(self as &dyn TypeDatabase, self)
                .with_query_db(self)
                .with_limited_resolver(),
        )
    }

    /// Build a `TypeEvaluator` whose resolver is the arena-invariant
    /// [`StoreOnlyResolver`], for the instantiation-time re-reduce of a
    /// cross-arena `Lazy`/`Application` base (issue #14344 / #14345, default-OFF
    /// behind `TSZ_INST_RESOLVER_REREDUCE`).
    ///
    /// Returns `None` when no `DefinitionStore` is attached (the shim has
    /// nothing to resolve against). When `Some`, the evaluator:
    /// - resolves a cross-arena `Lazy(URItoKindN)` base to its empty-Object
    ///   snapshot via the store-only `resolve_lazy`, so the published
    ///   home-symbol redirect can re-index the populated home body;
    /// - keeps the same cross-call instantiation/application caches via
    ///   `query_db = self`;
    /// - adopts the *limited-resolver* discipline (`with_limited_resolver`):
    ///   it READS the resolver-independent cross-call caches but never WRITES
    ///   the `application_eval_cache`, and `with_resolver` construction already
    ///   leaves `persistent_memo_reads = false`, so it never persists a
    ///   resolver-flavored entry into the program-global `(TypeId, options)`
    ///   eval memo. The global memo therefore stays populated only by the true
    ///   `NoopResolver` path, keeping the cache key resolver-independent and
    ///   the result arena-consistent.
    pub(crate) fn store_resolver_backed_evaluator<'r>(
        &'r self,
        resolver: &'r StoreOnlyResolver<'r>,
    ) -> StoreResolverBackedEvaluator<'r> {
        TypeEvaluator::with_resolver(self as &dyn TypeDatabase, resolver)
            .with_query_db(self)
            .with_limited_resolver()
    }
}
