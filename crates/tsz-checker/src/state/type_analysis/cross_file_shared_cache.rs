//! Shared cross-file caches for actual library symbol delegation.

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::{TypeId, TypeParamInfo};

fn shared_actual_lib_delegation_cache_key(name: &str) -> String {
    format!("\0actual-lib-delegation:{name}")
}

fn shared_actual_lib_value_declaration_cache_key(
    name: &str,
    file_name: &str,
    decl_idx: tsz_parser::NodeIndex,
    mode: u8,
) -> String {
    format!("\0actual-lib-value-declaration:{file_name}:{decl_idx:?}:{mode}:{name}")
}

impl<'a> CheckerState<'a> {
    /// Compute the shared-cache name for a builtin lib CLASS symbol, scoping the
    /// `symbol_arenas` borrow inside this `&self` method so the caller can then
    /// make a `&mut self` call to check the cache without hitting E0502.
    pub(super) fn lib_class_shared_cache_name(
        &self,
        sym_id: SymbolId,
        needs_cross_file_delegation: bool,
    ) -> Option<String> {
        if needs_cross_file_delegation {
            return None;
        }
        let is_builtin = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .is_some_and(|arc| {
                let a = arc.as_ref();
                !std::ptr::eq(a, self.ctx.arena) && is_builtin_lib_declaration_arena(a)
            });
        is_builtin
            .then(|| {
                self.get_cross_file_symbol(sym_id)
                    .filter(|s| s.has_any_flags(symbol_flags::CLASS))
                    .map(|s| s.escaped_name.clone())
                    .filter(|n| !self.lib_name_locally_augmented(n))
            })
            .flatten()
    }

    pub(crate) fn shared_actual_lib_delegation_name(
        &self,
        sym_id: SymbolId,
        delegate_arena: Option<&tsz_parser::NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<String> {
        if needs_cross_file_delegation
            || !delegate_arena.is_some_and(is_builtin_lib_declaration_arena)
        {
            return None;
        }
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let name = symbol.escaped_name.clone();
        if self.lib_name_locally_augmented(&name) {
            return None;
        }
        Some(name)
    }

    fn lookup_shared_lib_type(&self, cache_key: &str) -> Option<TypeId> {
        let shared = self.ctx.shared_lib_type_cache.as_ref()?;
        let entry = shared.get(cache_key)?;
        let cached_type = (*entry)?;
        if !crate::query_boundaries::common::type_id_is_known_to_db(self.ctx.types, cached_type) {
            return None;
        }
        Some(cached_type)
    }

    pub(crate) fn cached_shared_actual_lib_delegation(
        &mut self,
        sym_id: SymbolId,
        shared_name: &str,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let cached_type =
            self.lookup_shared_lib_type(&shared_actual_lib_delegation_cache_key(shared_name))?;
        let params = self.get_type_params_for_symbol(sym_id);
        self.ctx.symbol_types.insert(sym_id, cached_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (cached_type, params.clone()));
        Some((cached_type, params))
    }

    pub(crate) fn cache_shared_actual_lib_delegation(&self, shared_name: &str, result: TypeId) {
        self.insert_to_shared_lib_cache(
            shared_actual_lib_delegation_cache_key(shared_name),
            result,
        );
    }

    pub(crate) fn shared_actual_lib_value_declaration_name(
        &self,
        sym_id: SymbolId,
        decl_arena: &tsz_parser::NodeArena,
    ) -> Option<String> {
        if !is_builtin_lib_declaration_arena(decl_arena) {
            return None;
        }
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let name = symbol.escaped_name.clone();
        if self.lib_name_locally_augmented(&name) || self.any_program_file_augments_lib_name(&name)
        {
            return None;
        }
        Some(name)
    }

    pub(crate) fn cached_shared_actual_lib_value_declaration(
        &mut self,
        shared_name: &str,
        decl_arena: &tsz_parser::NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        mode: u8,
    ) -> Option<TypeId> {
        let file_name = decl_arena.source_files.first()?.file_name.as_str();
        let cached_type = self.lookup_shared_lib_type(
            &shared_actual_lib_value_declaration_cache_key(shared_name, file_name, decl_idx, mode),
        )?;
        self.ctx.lib_delegation_cache.insert_declaration_node_type(
            decl_arena,
            decl_idx,
            mode,
            cached_type,
        );
        Some(cached_type)
    }

    pub(crate) fn cache_shared_actual_lib_value_declaration(
        &self,
        shared_name: &str,
        decl_arena: &tsz_parser::NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        mode: u8,
        result: TypeId,
    ) {
        let Some(file_name) = decl_arena
            .source_files
            .first()
            .map(|source_file| source_file.file_name.as_str())
        else {
            return;
        };
        self.insert_to_shared_lib_cache(
            shared_actual_lib_value_declaration_cache_key(shared_name, file_name, decl_idx, mode),
            result,
        );
    }

    /// Returns the shared cache name for a lib class instance-type delegation.
    ///
    /// Parallel to [`shared_actual_lib_delegation_name`] but restricted to CLASS
    /// symbols so the class-instance cache bucket does not overlap the
    /// alias/interface bucket. Exposed for tests; the production path in
    /// `delegate_cross_arena_class_instance_type` inlines equivalent logic to
    /// keep `symbol_arenas` borrows scoped before the `&mut self` cache call.
    pub(crate) fn shared_actual_lib_class_delegation_name(
        &self,
        sym_id: SymbolId,
        delegate_arena: Option<&tsz_parser::NodeArena>,
        needs_cross_file_delegation: bool,
    ) -> Option<String> {
        if needs_cross_file_delegation
            || !delegate_arena.is_some_and(is_builtin_lib_declaration_arena)
        {
            return None;
        }
        let symbol = self.get_cross_file_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }
        let name = symbol.escaped_name.clone();
        if self.lib_name_locally_augmented(&name) {
            return None;
        }
        Some(name)
    }

    /// Check the shared lib class instance-type cache keyed by the symbol's
    /// escaped name. On hit, warms the per-file `lib_delegation_cache` and
    /// reconstructs type parameters from the symbol's own declarations.
    pub(crate) fn cached_shared_actual_lib_class_delegation(
        &mut self,
        sym_id: SymbolId,
        shared_name: &str,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let cached_type = self
            .lookup_shared_lib_type(&shared_actual_lib_class_delegation_cache_key(shared_name))?;
        let params = self.get_type_params_for_symbol(sym_id);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (cached_type, params.clone()));
        Some((cached_type, params))
    }

    /// Write a lib class instance `TypeId` to the shared cache keyed by name.
    /// Idempotent: the first writer wins; later checkers that computed the same
    /// type are no-ops.
    pub(crate) fn cache_shared_actual_lib_class_delegation(
        &self,
        shared_name: &str,
        result: TypeId,
    ) {
        self.insert_to_shared_lib_cache(
            shared_actual_lib_class_delegation_cache_key(shared_name),
            result,
        );
    }

    fn insert_to_shared_lib_cache(&self, key: String, result: TypeId) {
        if matches!(result, TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }
        if let Some(shared) = self.ctx.shared_lib_type_cache.as_ref() {
            shared.entry(key).or_insert(Some(result));
        }
    }
}

fn shared_actual_lib_class_delegation_cache_key(name: &str) -> String {
    format!("\0actual-lib-class-delegation:{name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerContext, CheckerOptions, LibContext};
    use crate::test_utils::load_lib_files;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    #[test]
    fn shared_actual_lib_delegation_hit_populates_file_local_symbol_cache() {
        let lib_files = load_lib_files(&[
            "es2015.iterable.d.ts",
            "es2020.symbol.wellknown.d.ts",
            "esnext.iterator.d.ts",
        ]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        state.ctx.set_lib_contexts(lib_contexts);
        state.ctx.set_actual_lib_file_count(lib_files.len());

        let array_iterator_type = state
            .resolve_lib_type_by_name("ArrayIterator")
            .expect("ArrayIterator should resolve through lib contexts");
        let shared = Arc::new(dashmap::DashMap::new());
        shared.insert(
            shared_actual_lib_delegation_cache_key("ArrayIterator"),
            Some(array_iterator_type),
        );
        state.ctx.shared_lib_type_cache = Some(shared);

        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("ArrayIterator")
            .expect("ArrayIterator should resolve to a lib symbol");
        let (cached_type, params) = state
            .cached_shared_actual_lib_delegation(sym_id, "ArrayIterator")
            .expect("shared actual-lib cache should return known TypeIds");

        assert_eq!(cached_type, array_iterator_type);
        assert!(
            !params.is_empty(),
            "shared actual-lib cache hits must preserve generic metadata"
        );
        assert_eq!(
            state.ctx.symbol_types.get(&sym_id),
            Some(array_iterator_type)
        );
        assert!(
            state.ctx.lib_delegation_cache.contains_symbol_type(sym_id),
            "shared hits should warm the file-local delegation cache"
        );
    }

    #[test]
    fn shared_actual_lib_delegation_name_accepts_dom_builtin_libs() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLElement")
            .expect("HTMLElement should resolve to a DOM lib symbol");

        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(dom_arena), false),
            Some("HTMLElement".to_string())
        );
        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(dom_arena), true),
            None,
            "requests that still need cross-file delegation must not use the shared name cache",
        );
    }

    #[test]
    fn shared_actual_lib_delegation_name_rejects_external_package_declarations() {
        let mut parser = ParserState::new(
            "node_modules/pkg/index.d.ts".to_string(),
            "export interface ExternalFixture { value: string; }".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "node_modules/pkg/index.d.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("ExternalFixture")
            .expect("external package interface symbol");

        assert_eq!(
            state.shared_actual_lib_delegation_name(sym_id, Some(arena.as_ref()), false),
            None,
            "only built-in TypeScript libs may use the shared actual-lib name cache",
        );
    }

    #[test]
    fn shared_actual_lib_value_declaration_cache_roundtrip_warms_local_cache() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        state
            .ctx
            .set_all_binders(Arc::new(vec![Arc::clone(&binder)]));
        let shared: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());
        state.ctx.shared_lib_type_cache = Some(shared);

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");
        let decl_idx = state
            .get_cross_file_symbol(sym_id)
            .expect("Window symbol should be visible")
            .value_declaration;
        let shared_name = state
            .shared_actual_lib_value_declaration_name(sym_id, dom_arena)
            .expect("DOM value declarations should be shared-cache eligible");

        assert!(
            state
                .cached_shared_actual_lib_value_declaration(&shared_name, dom_arena, decl_idx, 1)
                .is_none(),
            "cache should be empty before the first write"
        );

        state.cache_shared_actual_lib_value_declaration(
            &shared_name,
            dom_arena,
            decl_idx,
            1,
            tsz_solver::TypeId::STRING,
        );

        assert_eq!(
            state.cached_shared_actual_lib_value_declaration(&shared_name, dom_arena, decl_idx, 1),
            Some(tsz_solver::TypeId::STRING)
        );
        assert_eq!(
            state
                .ctx
                .lib_delegation_cache
                .declaration_node_type(dom_arena, decl_idx, 1),
            Some(tsz_solver::TypeId::STRING),
            "shared hits should warm the file-local declaration-node cache"
        );
    }

    #[test]
    fn shared_actual_lib_value_declaration_name_rejects_program_augmentations() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);

        let mut augment_parser = ParserState::new(
            "augment.ts".to_string(),
            "export {}; declare global { interface Window { tszAugment: string; } }".to_string(),
        );
        let augment_root = augment_parser.parse_source_file();
        let mut augment_binder = BinderState::new();
        augment_binder.bind_source_file(augment_parser.get_arena(), augment_root);
        let augment_binder = Arc::new(augment_binder);

        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        state
            .ctx
            .set_all_binders(Arc::new(vec![Arc::clone(&binder), augment_binder]));

        let dom_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");

        assert_eq!(
            state.shared_actual_lib_value_declaration_name(sym_id, dom_arena),
            None,
            "program-wide global augmentations must keep value declarations out of the shared cache",
        );
    }

    #[test]
    fn shared_actual_lib_class_delegation_name_accepts_scripthost_class() {
        // `scripthost.d.ts` uses `declare class SafeArray<T>` – one of the few
        // builtin lib files that actually uses the `class` keyword, so its
        // symbols carry `symbol_flags::CLASS` from the binder.
        let lib_files = load_lib_files(&["scripthost.d.ts"]);
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "let s: SafeArray<number>;".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };

        let scripthost_arena = lib_files[0].arena.as_ref();
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("SafeArray")
            .expect("SafeArray should be in lib symbol table from scripthost.d.ts");

        assert!(
            state
                .shared_actual_lib_class_delegation_name(sym_id, Some(scripthost_arena), false)
                .is_some(),
            "CLASS symbols in builtin lib arenas should produce a cache name"
        );
        assert_eq!(
            state.shared_actual_lib_class_delegation_name(sym_id, Some(scripthost_arena), true),
            None,
            "needs_cross_file_delegation=true must skip the lib class cache"
        );
    }

    #[test]
    fn shared_actual_lib_class_delegation_cache_roundtrip() {
        let lib_files = load_lib_files(&["scripthost.d.ts"]);
        let mut parser = ParserState::new(
            "fixture.ts".to_string(),
            "let s: SafeArray<number>;".to_string(),
        );
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let mut state = CheckerState { ctx };
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        state.ctx.set_lib_contexts(lib_contexts);
        state.ctx.set_actual_lib_file_count(lib_files.len());

        let shared: Arc<dashmap::DashMap<String, Option<tsz_solver::TypeId>>> =
            Arc::new(dashmap::DashMap::new());
        state.ctx.shared_lib_type_cache = Some(shared.clone());

        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("SafeArray")
            .expect("SafeArray should be in lib symbol table");

        // Cache miss before write
        assert!(
            state
                .cached_shared_actual_lib_class_delegation(sym_id, "SafeArray")
                .is_none(),
            "cache should be empty initially"
        );

        // Write a sentinel type (raw id; well above the built-in reservation range)
        let sentinel = tsz_solver::TypeId(9000);
        state.cache_shared_actual_lib_class_delegation("SafeArray", sentinel);

        // Verify DashMap has the entry with the correct key prefix
        assert!(
            shared.contains_key("\0actual-lib-class-delegation:SafeArray"),
            "cache write must use the expected key prefix"
        );

        // A second write is a no-op (or_insert semantics — first writer wins)
        let sentinel2 = tsz_solver::TypeId(9001);
        state.cache_shared_actual_lib_class_delegation("SafeArray", sentinel2);
        let stored = shared
            .get("\0actual-lib-class-delegation:SafeArray")
            .expect("entry must exist after write");
        assert_eq!(
            *stored,
            Some(sentinel),
            "first writer wins; second write must not overwrite"
        );
    }

    #[test]
    fn shared_actual_lib_class_delegation_name_rejects_non_class() {
        let lib_files = load_lib_files(&["dom.d.ts"]);
        let mut parser = ParserState::new("fixture.ts".to_string(), "let x: Window;".to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
        let arena = Arc::new(parser.get_arena().clone());
        let binder = Arc::new(binder);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena.as_ref(),
            binder.as_ref(),
            &types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        );
        let state = CheckerState { ctx };
        let dom_arena = lib_files[0].arena.as_ref();

        // `Window` in DOM is an interface, not a class — should not enter
        // the class-instance shared cache.
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Window")
            .expect("Window should be a DOM lib symbol");
        assert_eq!(
            state.shared_actual_lib_class_delegation_name(sym_id, Some(dom_arena), false),
            None,
            "interface symbols must not match the CLASS-only lib class cache"
        );
    }
}
