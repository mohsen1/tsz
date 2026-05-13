//! Cross-file symbol-type cache guard helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
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
            || self.ctx.program_has_module_augmentations()
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
            .filter(|arena| {
                self.ctx
                    .symbol_arena_symbol_type_cache_is_stable(sym_id, arena)
            })
            .and_then(|arena| self.ctx.get_file_idx_for_arena(arena))
    }
}
