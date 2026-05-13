//! Cross-file symbol-type cache guard helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::perf_counters::{
    CrossArenaSymbolMissSource, SourceFileSymbolArenaCacheEligibility,
    record_source_file_symbol_arena_cache_eligibility,
};

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
        if delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::NonSymbolArenaSource,
            );
            return None;
        }
        if self.ctx.program_has_module_augmentations() {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::ModuleAugmentation,
            );
            return None;
        }

        let Some(arena) = delegate_arena else {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::MissingArena,
            );
            return None;
        };
        if std::ptr::eq(arena, self.ctx.arena) {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::CurrentArena,
            );
            return None;
        }
        let Some(source_file) = arena.source_files.first() else {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::MissingSourceFile,
            );
            return None;
        };
        if source_file.is_declaration_file {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::DeclarationFile,
            );
            return None;
        }
        if !self
            .ctx
            .symbol_arena_symbol_type_cache_is_stable(sym_id, arena)
        {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::UnstableSymbol,
            );
            return None;
        }
        let Some(file_idx) = self.ctx.get_file_idx_for_arena(arena) else {
            record_source_file_symbol_arena_cache_eligibility(
                SourceFileSymbolArenaCacheEligibility::MissingFileIndex,
            );
            return None;
        };
        record_source_file_symbol_arena_cache_eligibility(
            SourceFileSymbolArenaCacheEligibility::Eligible,
        );
        Some(file_idx)
    }
}
