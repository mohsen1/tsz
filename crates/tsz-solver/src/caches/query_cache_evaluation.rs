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
