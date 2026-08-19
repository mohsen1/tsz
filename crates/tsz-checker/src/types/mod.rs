pub(crate) mod binding_pattern_padding;
pub mod class_type;
pub mod computation;
pub(crate) mod computed_names;
pub mod function_iife_inference;
pub mod function_type;
pub(crate) mod function_type_circular;
pub(crate) mod function_type_helpers;
pub(crate) mod function_type_signature_display;
mod interface_heritage_class;
mod interface_member_merge;
pub mod interface_type;
pub mod module_augmentation;
mod module_augmentation_prime;
mod module_augmentation_redirect;
pub(crate) mod module_augmentation_value;
pub mod object_type;
mod property_access_augmentation;
pub(crate) mod property_access_helpers;
mod property_access_narrowing_skip;
pub(crate) mod property_access_type;
pub(crate) mod queries;
pub(crate) mod signature_binding_scope;
pub mod type_checking;
pub mod type_literal_checker;
pub mod type_node;
mod type_node_advanced;
mod type_node_cache_policy;
mod type_node_context;
mod type_node_declared_params;
pub(crate) mod type_node_helpers;
mod type_node_lowering;
mod type_node_merged_value_query;
mod type_node_property_names;
mod type_node_query_members;
mod type_node_resolution;
mod type_node_signature;
mod type_node_value_symbols;
pub(crate) mod unique_symbol_arena;
pub(crate) mod unique_symbol_construction;
pub(crate) mod utilities;
pub(crate) mod window_global_this_annotation;

/// Reset every type-resolution recursion-guard and scratch-pool thread-local
/// owned by this module tree to its empty state.
///
/// Called from `clear_all_thread_local_state` at independent-compilation
/// boundaries (batch mode) so a project that bails out mid alias-resolution
/// cannot leave a non-zero depth counter, a stale active-set entry, or a
/// retained scratch pool that would affect the next project on the same worker.
pub(crate) fn reset_type_resolution_guards() {
    type_node_resolution::reset_alias_resolve_state();
    type_checking::reset_alias_resolution_pools();
}

#[cfg(test)]
pub(crate) fn dirty_type_resolution_guards_for_test() {
    type_node_resolution::dirty_alias_resolve_state_for_test();
    type_checking::dirty_alias_resolution_pools_for_test();
}

#[cfg(test)]
pub(crate) fn type_resolution_guards_clear_for_test() -> bool {
    type_node_resolution::alias_resolve_state_is_clear_for_test()
        && type_checking::alias_resolution_pools_clear_for_test()
}
