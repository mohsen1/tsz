//! Perf-counter attribution for cross-file child-checker residue.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_common::perf_counters::{self, CrossArenaSymbolMissKind, CrossArenaSymbolMissSource};

impl<'a> CheckerState<'a> {
    pub(super) fn record_cross_arena_symbol_miss_residue(
        &self,
        sym_id: SymbolId,
        miss_source: CrossArenaSymbolMissSource,
        miss_kind: CrossArenaSymbolMissKind,
        miss_target_is_declaration_file: bool,
        miss_target_file: Option<&str>,
    ) {
        perf_counters::record_cross_arena_symbol_miss(
            miss_source,
            miss_kind,
            miss_target_is_declaration_file,
        );
        if !perf_counters::enabled_fast() {
            return;
        }
        let Some(symbol) = self.get_cross_file_symbol(sym_id) else {
            return;
        };
        if miss_target_is_declaration_file {
            perf_counters::record_cross_arena_declaration_file_miss_residue(
                miss_source,
                miss_kind,
                symbol.escaped_name.as_str(),
                miss_target_file,
            );
        } else {
            perf_counters::record_cross_arena_source_file_miss_residue(
                miss_source,
                miss_kind,
                symbol.escaped_name.as_str(),
                miss_target_file,
            );
        }
    }
}
