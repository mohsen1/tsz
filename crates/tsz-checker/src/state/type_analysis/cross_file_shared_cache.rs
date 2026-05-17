//! Shared cross-file caches for actual library symbol delegation.

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena;
use tsz_binder::SymbolId;
use tsz_solver::{TypeId, TypeParamInfo};

fn shared_actual_lib_delegation_cache_key(name: &str) -> String {
    format!("\0actual-lib-delegation:{name}")
}

impl<'a> CheckerState<'a> {
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

    pub(crate) fn cached_shared_actual_lib_delegation(
        &mut self,
        sym_id: SymbolId,
        shared_name: &str,
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        let cached_type = {
            let shared = self.ctx.shared_lib_type_cache.as_ref()?;
            let entry = shared.get(&shared_actual_lib_delegation_cache_key(shared_name))?;
            let cached_type = (*entry)?;
            if !crate::query_boundaries::common::type_id_is_known_to_db(self.ctx.types, cached_type)
            {
                return None;
            }
            cached_type
        };
        let params = self.get_type_params_for_symbol(sym_id);
        self.ctx.symbol_types.insert(sym_id, cached_type);
        self.ctx
            .lib_delegation_cache
            .insert_symbol_type(sym_id, (cached_type, params.clone()));
        Some((cached_type, params))
    }

    pub(crate) fn cache_shared_actual_lib_delegation(&self, shared_name: &str, result: TypeId) {
        if matches!(result, TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }
        if let Some(shared) = self.ctx.shared_lib_type_cache.as_ref() {
            shared
                .entry(shared_actual_lib_delegation_cache_key(shared_name))
                .or_insert(Some(result));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerContext, CheckerOptions, LibContext};
    use crate::test_utils::load_lib_files;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

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
            state.ctx.symbol_types.get(&sym_id).copied(),
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
    }
}
