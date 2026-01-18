//! Native CLI support for the tsz binary.

pub mod args;

#[allow(clippy::collapsible_if)]
pub mod config;

#[allow(clippy::collapsible_if)]
#[allow(clippy::collapsible_else_if)]
#[allow(clippy::question_mark)]
#[allow(clippy::manual_strip)]
#[allow(clippy::unnecessary_filter_map)]
#[allow(clippy::manual_is_ascii_check)]
#[allow(clippy::match_like_matches_macro)]
#[allow(clippy::option_map_or_none)]
pub mod driver;

#[allow(clippy::collapsible_if)]
#[allow(clippy::match_like_matches_macro)]
pub mod fs;

#[allow(clippy::unnecessary_cast)]
pub mod reporter;

#[allow(clippy::question_mark)]
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
mod watch_tests;
