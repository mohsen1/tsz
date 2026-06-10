//! Type analysis: qualified name resolution, symbol type computation,
//! type queries, and contextual literal type analysis.

mod circular_partial_ctor;
pub(crate) mod computed;
mod computed_alias;
mod computed_commonjs;
pub(crate) mod computed_helpers;
mod computed_helpers_binding;
mod computed_helpers_circular;
mod computed_helpers_namespace_display;
mod computed_helpers_private;
mod computed_loops;
mod core;
mod core_type_query;
pub(crate) mod cross_file;
mod cross_file_alias_cycle;
mod cross_file_alias_shortcut;
mod cross_file_cache;
mod cross_file_delegation;
pub(crate) mod cross_file_direct;
mod cross_file_direct_actual_lib;
mod cross_file_direct_alias_chain;
#[cfg(test)]
mod cross_file_direct_alias_chain_concrete_tests;
#[cfg(test)]
mod cross_file_direct_alias_chain_globals_tests;
#[cfg(test)]
mod cross_file_direct_alias_chain_tests;
mod cross_file_direct_declaration_alias;
mod cross_file_direct_functions;
mod cross_file_env_merge;
mod cross_file_globals;
mod cross_file_import_alias_pin;
mod cross_file_interface_depth;
mod cross_file_overlay_gate;
pub(crate) mod cross_file_query_types;
mod cross_file_residue;
mod cross_file_shared_cache;
mod qualified_names;
mod source_alias_attribution;
mod symbol_type_helpers;
mod syntactic_defaults;
mod type_param_defaults;

/// Reset every cross-file/cross-arena recursion-guard thread-local owned by
/// this module tree to its empty state.
///
/// Called from `clear_all_thread_local_state` at independent-compilation
/// boundaries (batch mode) so a project that bails out mid-delegation cannot
/// leave a non-zero depth counter or a non-empty resolution stack that would
/// suppress cross-file resolution in the next project on the same worker.
pub(crate) fn reset_cross_file_recursion_guards() {
    cross_file_interface_depth::reset_cross_arena_interface_depth();
    cross_file_alias_cycle::reset_cross_arena_alias_stack();
}

#[cfg(test)]
pub(crate) fn dirty_cross_file_recursion_guards_for_test() {
    cross_file_interface_depth::set_cross_arena_interface_depth_for_test(2);
    cross_file_alias_cycle::push_cross_arena_alias_for_test(tsz_solver::def::DefId::INVALID);
}

#[cfg(test)]
pub(crate) fn cross_file_recursion_guards_clear_for_test() -> bool {
    cross_file_interface_depth::cross_arena_interface_depth_for_test() == 0
        && cross_file_alias_cycle::cross_arena_alias_stack_len_for_test() == 0
}
