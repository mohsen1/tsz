//! Native CLI support for the tsz TypeScript compiler.
//!
//! This crate provides CLI binaries (`tsz`, `tsz-lsp`, `tsz-server`) and
//! all CLI-specific modules (argument parsing, file discovery, config loading,
//! compilation driver, watch mode, etc.).

pub mod commands;
pub use tsz::config;
pub mod driver;
pub mod localization;
#[cfg(feature = "perf-tools")]
pub mod perf_json;
pub mod project;
pub mod reporting;
pub mod try_tsz;
pub use commands::args;
pub use commands::build;
pub use commands::help;
pub use commands::watch;
pub use localization::locale;
pub use project::fs;
pub use project::incremental;
pub use project::refs as project_refs;
pub use reporting::reporter;
pub use reporting::trace;
pub use reporting::tracing_config;

#[cfg(test)]
#[path = "../tests/args_tests.rs"]
mod args_tests;
#[cfg(test)]
#[path = "../tests/build_tests.rs"]
mod build_tests;
#[cfg(test)]
#[path = "../tests/config_tests.rs"]
mod config_tests;
#[cfg(test)]
#[path = "../tests/const_using_uninitialized_ts1155_diagnostic_family_cli_tests.rs"]
mod const_using_uninitialized_ts1155_diagnostic_family_cli_tests;
#[cfg(test)]
#[path = "../tests/cross_file_imported_const_computed_key_identity_tests.rs"]
mod cross_file_imported_const_computed_key_identity_tests;
#[cfg(test)]
#[path = "../tests/cross_file_local_callee_symbol_identity_tests.rs"]
mod cross_file_local_callee_symbol_identity_tests;
#[cfg(test)]
#[path = "../tests/cross_module_generic_method_constraint_cli_tests.rs"]
mod cross_module_generic_method_constraint_cli_tests;
#[cfg(test)]
#[path = "../tests/cross_module_import_cycle_class_member_cli_tests.rs"]
mod cross_module_import_cycle_class_member_cli_tests;
#[cfg(test)]
#[path = "../tests/driver_tests.rs"]
mod driver_tests;
#[cfg(test)]
#[path = "../tests/driver_tests_ts2307.rs"]
mod driver_tests_ts2307;
#[cfg(test)]
#[path = "../tests/dual_package_exports_tests.rs"]
mod dual_package_exports_tests;
#[cfg(test)]
#[path = "../tests/file_casing_collision_ts1149_tests.rs"]
mod file_casing_collision_ts1149_tests;
#[cfg(test)]
#[path = "../tests/fs_tests.rs"]
mod fs_tests;
#[cfg(test)]
#[path = "../tests/generic_interface_bivariant_param_relation_tests.rs"]
mod generic_interface_bivariant_param_relation_tests;
#[cfg(test)]
#[path = "../tests/global_this_type_member_cli_tests.rs"]
mod global_this_type_member_cli_tests;
#[cfg(test)]
#[path = "../tests/imported_ambient_const_enum_ts2748_cli_tests.rs"]
mod imported_ambient_const_enum_ts2748_cli_tests;
#[cfg(test)]
#[path = "../tests/interface_extends_cross_module_class_cli_tests.rs"]
mod interface_extends_cross_module_class_cli_tests;
#[cfg(test)]
#[path = "../tests/interface_extends_generic_alias_cli_tests.rs"]
mod interface_extends_generic_alias_cli_tests;
#[cfg(test)]
#[path = "../tests/lib_heritage_import_order_cli_tests.rs"]
mod lib_heritage_import_order_cli_tests;
#[cfg(test)]
#[path = "../tests/lib_interface_merge_flatarray_cli_tests.rs"]
mod lib_interface_merge_flatarray_cli_tests;
#[cfg(test)]
#[path = "../tests/lib_shadow_cli_tests.rs"]
mod lib_shadow_cli_tests;
#[cfg(test)]
#[path = "../tests/parameter_list_grammar_one_per_list_tests.rs"]
mod parameter_list_grammar_one_per_list_tests;
#[cfg(test)]
#[path = "../tests/prettify_empty_object_intersection_cli_tests.rs"]
mod prettify_empty_object_intersection_cli_tests;
#[cfg(test)]
#[path = "../tests/reporter_tests.rs"]
mod reporter_tests;
#[cfg(test)]
#[path = "../tests/reserved_word_emit_tests.rs"]
mod reserved_word_emit_tests;
#[cfg(test)]
#[path = "../tests/symbol_keyed_member_cross_arena_cli_tests.rs"]
mod symbol_keyed_member_cross_arena_cli_tests;
#[cfg(test)]
#[path = "../tests/tsc_compat_tests.rs"]
mod tsc_compat_tests;
#[cfg(test)]
#[path = "../tests/tuple_interface_extends_array_numeric_member_cli_tests.rs"]
mod tuple_interface_extends_array_numeric_member_cli_tests;
#[cfg(test)]
#[path = "../tests/unresolved_import_type_application_cli_tests.rs"]
mod unresolved_import_type_application_cli_tests;
#[cfg(test)]
#[path = "../tests/untyped_module_resolution_tests.rs"]
mod untyped_module_resolution_tests;
#[cfg(test)]
#[path = "../tests/watch_tests.rs"]
mod watch_tests;
