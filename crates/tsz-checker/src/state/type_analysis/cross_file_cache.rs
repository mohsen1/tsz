//! Cross-file cache guard helpers.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::CrossArenaSymbolMissSource;

impl<'a> CheckerState<'a> {
    pub(super) fn symbol_arena_symbol_type_cache_file_idx(
        &self,
        needs_cross_file_delegation: bool,
        cross_file_idx: Option<usize>,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&tsz_parser::NodeArena>,
        sym_id: SymbolId,
    ) -> Option<usize> {
        if needs_cross_file_delegation {
            return cross_file_idx;
        }
        if delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
            || self.program_has_module_augmentations()
        {
            return None;
        }

        delegate_arena
            .filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))
            .filter(|arena| {
                arena
                    .source_files
                    .first()
                    .is_some_and(|source_file| !source_file.is_declaration_file)
            })
            .filter(|arena| self.symbol_arena_symbol_type_cache_is_stable(sym_id, arena))
            .and_then(|arena| self.ctx.get_file_idx_for_arena(arena))
    }

    pub(super) fn program_has_module_augmentations(&self) -> bool {
        // Module augmentation can make a source-file symbol type depend on the
        // importer graph, so the shared `(file_idx, SymbolId)` cache key is
        // only valid when the program has no module augmentation metadata.
        if self
            .ctx
            .global_module_augmentations_index
            .as_ref()
            .is_some_and(|index| !index.is_empty())
            || self
                .ctx
                .global_augmentation_targets_index
                .as_ref()
                .is_some_and(|index| !index.is_empty())
        {
            return true;
        }

        self.ctx.all_binders.as_ref().is_some_and(|binders| {
            binders.iter().any(|binder| {
                !binder.module_augmentations.is_empty()
                    || !binder.augmentation_target_modules.is_empty()
            })
        }) || !self.ctx.binder.module_augmentations.is_empty()
            || !self.ctx.binder.augmentation_target_modules.is_empty()
    }

    pub(super) fn symbol_arena_symbol_type_cache_is_stable(
        &self,
        sym_id: SymbolId,
        delegate_arena: &tsz_parser::NodeArena,
    ) -> bool {
        let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
            || symbol.declarations.len() != 1
        {
            return false;
        }

        // The `symbol_arenas` map stores one arena for the symbol, but merged
        // or augmented symbols can also have declarations in other arenas. A
        // cached symbol type is reusable only for the small stable slice where
        // the single class/interface declaration is proven to belong solely to
        // the delegated source-file arena.
        symbol.declarations.iter().all(|&decl_idx| {
            self.ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .is_some_and(|arenas| {
                    arenas.len() == 1 && std::ptr::eq(arenas[0].as_ref(), delegate_arena)
                })
        })
    }
}
