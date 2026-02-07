//! Native CLI support for the tsz TypeScript compiler.
//!
//! This crate provides CLI binaries (`tsz`, `tsz-lsp`, `tsz-server`) and
//! all CLI-specific modules (argument parsing, file discovery, config loading,
//! compilation driver, watch mode, etc.).

pub mod args;
pub mod build;
pub use tsz::config;
pub mod driver;
pub mod driver_resolution;
pub mod fs;
pub mod incremental;
pub mod locale;
pub mod project_refs;
pub mod reporter;
pub mod trace;
pub mod tracing_config;
pub mod watch;

#[cfg(test)]
#[path = "tests/args_tests.rs"]
mod args_tests;
#[cfg(test)]
#[path = "tests/build_tests.rs"]
mod build_tests;
#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod config_tests;
#[cfg(test)]
#[path = "tests/driver_tests.rs"]
mod driver_tests;
#[cfg(test)]
#[path = "tests/fs_tests.rs"]
mod fs_tests;
#[cfg(test)]
#[path = "tests/reporter_tests.rs"]
mod reporter_tests;
#[cfg(test)]
#[path = "tests/tsc_compat_tests.rs"]
mod tsc_compat_tests;
#[cfg(test)]
#[path = "tests/watch_tests.rs"]
mod watch_tests;
