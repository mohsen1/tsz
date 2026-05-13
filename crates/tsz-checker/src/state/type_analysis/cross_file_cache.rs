//! Cross-file symbol-type cache guard helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::perf_counters::{
    CrossArenaSymbolMissSource, SourceFileSymbolArenaCacheEligibilityOutcome,
    record_source_file_symbol_arena_cache_eligibility_outcome,
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
        use SourceFileSymbolArenaCacheEligibilityOutcome as Outcome;
        let record = |outcome: Outcome| {
            record_source_file_symbol_arena_cache_eligibility_outcome(outcome);
        };

        if needs_cross_file_delegation {
            record(Outcome::CrossFileTarget);
            return cross_file_idx;
        }
        if delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena {
            record(Outcome::NonSymbolArena);
            return None;
        }
        if self.ctx.program_has_module_augmentations() {
            record(Outcome::ModuleAugmentation);
            return None;
        }

        let Some(arena) = delegate_arena else {
            record(Outcome::MissingDelegateArena);
            return None;
        };
        if std::ptr::eq(arena, self.ctx.arena) {
            record(Outcome::CurrentArena);
            return None;
        }
        let Some(source_file) = arena.source_files.first() else {
            record(Outcome::MissingSourceFile);
            return None;
        };
        if source_file.is_declaration_file {
            record(Outcome::TargetDeclarationFile);
            return None;
        }

        let outcome = self
            .ctx
            .source_file_symbol_arena_cache_stability_outcome(sym_id, arena);
        if outcome != Outcome::Cacheable {
            record(outcome);
            return None;
        }

        let file_idx = self.ctx.get_file_idx_for_arena(arena);
        if file_idx.is_some() {
            record(Outcome::Cacheable);
        } else {
            record(Outcome::MissingFileIndex);
        }
        file_idx
    }
}
