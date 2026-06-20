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
#[path = "../tests/driver_tests.rs"]
mod driver_tests;
#[cfg(test)]
#[path = "../tests/driver_tests_ts2307.rs"]
mod driver_tests_ts2307;
#[cfg(test)]
#[path = "../tests/dual_package_exports_tests.rs"]
mod dual_package_exports_tests;
#[cfg(test)]
#[path = "../tests/fs_tests.rs"]
mod fs_tests;
#[cfg(test)]
#[path = "../tests/generic_interface_bivariant_param_relation_tests.rs"]
mod generic_interface_bivariant_param_relation_tests;
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
#[path = "../tests/lib_shadow_cli_tests.rs"]
mod lib_shadow_cli_tests;
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
#[path = "../tests/watch_tests.rs"]
mod watch_tests;
