//! Native CLI support for the tsz binary.

pub mod args;
pub mod config;
pub mod driver;
pub mod fs;
pub mod reporter;
pub mod watch;

#[cfg(test)]
mod args_tests;
#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod driver_tests;
#[cfg(test)]
mod fs_tests;
#[cfg(test)]
mod reporter_tests;
#[cfg(test)]
mod tsc_compat_tests;
#[cfg(test)]
mod watch_tests;
