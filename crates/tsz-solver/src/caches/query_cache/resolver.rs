//! `TypeResolver` implementation for `QueryCache`.

use super::QueryCache;
use crate::caches::db::TypeDatabase;
use crate::def::DefId;
use crate::instantiation::instantiate::flags::inst_resolver_rereduce_enabled;
use crate::relations::subtype::TypeResolver;
use crate::types::{IntrinsicKind, SymbolRef, TypeId, TypeParamInfo};
use tsz_binder::SymbolId;

/// Symbol references remain unresolved here: the cache has no binder/type-env
/// symbol scope. When a shared `DefinitionStore` is attached, DefId-backed
/// lazy bodies and type parameters can be read through the store so the staged
/// instantiation re-reduce path can reduce already-published declarations.
/// Default evaluator construction still uses `NoopResolver`; these methods are
/// only consulted by the explicit store-backed evaluator.
impl TypeResolver for QueryCache<'_> {
    fn resolver_generation(&self) -> u64 {
        if !inst_resolver_rereduce_enabled() {
            return 0;
        }
        self.definition_store
            .map_or(0, crate::DefinitionStore::generation)
    }

    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        if !inst_resolver_rereduce_enabled() {
            return None;
        }
        self.definition_store?.get_body(def_id)
    }

    fn resolve_lazy_lookup_only(
        &self,
        def_id: DefId,
        interner: &dyn TypeDatabase,
    ) -> Option<TypeId> {
        self.resolve_lazy(def_id, interner)
    }

    fn get_lazy_type_params(&self, def_id: DefId) -> Option<Vec<TypeParamInfo>> {
        if !inst_resolver_rereduce_enabled() {
            return None;
        }
        self.definition_store?.get_type_params(def_id)
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.interner.get_boxed_type(kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.interner.get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_readonly_array_base_type()
    }

    /// Resolve `DefId` identity/metadata through the attached `DefinitionStore`.
    ///
    /// The `QueryCache` is the `&dyn QueryDatabase` resolver used by generic-call
    /// inference. Historically it had NO `DefinitionStore`, so these `DefId`-keyed
    /// resolver methods silently returned trait defaults in inference: the
    /// `shared_application_base_def_id` cross-arena base unification (via
    /// `defs_are_equivalent` declaration-site/`SymbolId` equality) and the
    /// variance-computation paths that depend on them were dead, leaving
    /// cross-arena generic-call inference unable to pair type-args (issue #14344;
    /// the fp-ts `unknown`-widening FP family). Wiring the store re-enables them.
    ///
    /// Gated behind `TSZ_XARENA_BASE_DECL` (default-OFF) so flag-OFF stays
    /// byte-parity with the historical store-less behavior until the change is
    /// proven on full conformance.
    fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        if !crate::inference::xarena_base::xarena_base_decl_enabled() {
            return None;
        }
        self.definition_store?.get_symbol_id(def_id).map(SymbolId)
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<crate::def::DefKind> {
        if !crate::inference::xarena_base::xarena_base_decl_enabled() {
            return None;
        }
        self.definition_store?.get_kind(def_id)
    }

    fn get_def_name(&self, def_id: DefId) -> Option<tsz_common::interner::Atom> {
        if !crate::inference::xarena_base::xarena_base_decl_enabled() {
            return None;
        }
        self.definition_store?.get(def_id).map(|info| info.name)
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        if !crate::inference::xarena_base::xarena_base_decl_enabled() {
            return def_id;
        }
        self.definition_store
            .map_or(def_id, |store| store.canonical_def_id(def_id))
    }

    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        if a == b {
            return true;
        }
        if !crate::inference::xarena_base::xarena_base_decl_enabled() {
            return false;
        }
        let Some(store) = self.definition_store else {
            return false;
        };
        store.defs_have_same_decl_site(a, b)
            || store
                .get_symbol_id(a)
                .zip(store.get_symbol_id(b))
                .is_some_and(|(sa, sb)| sa == sb)
    }

    fn canonical_decl_site_def_for_symbol(&self, symbol: SymbolRef) -> Option<DefId> {
        self.definition_store?
            .canonical_decl_site_def_for_symbol(symbol.0)
    }
}
