use tsz_common::perf_counters::CrossArenaSymbolMissSource;
use tsz_parser::NodeArena;

pub(super) fn symbol_delegation_needs_parent_targets(
    delegate_arena_source: CrossArenaSymbolMissSource,
    symbol_arena: &NodeArena,
    needs_cross_file_delegation: bool,
) -> bool {
    if needs_cross_file_delegation
        || delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena
    {
        return true;
    }

    !symbol_arena
        .source_files
        .first()
        .is_some_and(|source_file| source_file.is_declaration_file)
}
