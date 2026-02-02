//! Native CLI support for the tsz binary.

pub mod args;
pub mod build;
pub mod config;
pub mod driver;
pub mod driver_resolution;
pub mod fs;
pub mod incremental;
pub mod locale;
pub mod project_refs;
pub mod reporter;
pub mod trace;
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
