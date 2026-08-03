//! Type checking validation modules.
//!
//! Organized into focused submodules:
//! - `core` — utility methods, AST traversal helpers, member/declaration validation
//! - `declarations` — declaration-specific type checking (variable, function, class)
//! - `declarations_utils` — shared utilities for declaration checking
//! - `duplicate_identifiers` — duplicate identifier/declaration conflict detection
//! - `global` — global-scope type checking
//! - `property_init` — property initializer validation
//! - `type_alias_checking` — type alias declaration checking, type node validation
//! - `unused` — unused variable/parameter detection

mod alias_defid_visited_pool;
mod commonjs_object_exports;
mod core;
mod core_statement_checks;
mod cross_file_conflicts;
mod declarations;
mod declarations_utils;
mod duplicate_identifier_conflict_kinds;
mod duplicate_identifier_relation_helpers;
mod duplicate_identifiers;
mod duplicate_identifiers_ambient_default;
mod duplicate_identifiers_constructor;
mod duplicate_identifiers_export_surface;
mod duplicate_identifiers_global_augmentation;
mod duplicate_identifiers_helpers;
mod duplicate_identifiers_remote_lib;
mod duplicate_identifiers_symbol_set;
mod duplicate_identifiers_variable_family;
mod duplicate_index_signatures;
mod duplicate_property_modifiers;
mod global;
mod indexed_access;
mod property_init;
mod type_alias_body_validation;
mod type_alias_checking;
mod type_alias_depth_helpers;
mod type_alias_missing_name_coverage;
mod type_alias_recursion_patterns;
mod type_alias_variance;
mod unused;
mod using_declaration_placement;

/// Release the type-alias resolution scratch pool owned by this module tree.
/// Called at independent-compilation boundaries (batch mode).
pub(crate) fn reset_alias_resolution_pools() {
    alias_defid_visited_pool::reset_alias_defid_visited_pool();
}

#[cfg(test)]
pub(crate) fn dirty_alias_resolution_pools_for_test() {
    alias_defid_visited_pool::set_alias_defid_visited_pool_dirty_for_test();
}

#[cfg(test)]
pub(crate) fn alias_resolution_pools_clear_for_test() -> bool {
    alias_defid_visited_pool::alias_defid_visited_pool_is_released_for_test()
}
