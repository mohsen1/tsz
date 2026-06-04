#[path = "tsz/show_config.rs"]
mod show_config;

#[path = "tsz/arg_preprocess.rs"]
mod arg_preprocess;

#[path = "tsz/clap_errors.rs"]
mod clap_errors;

#[path = "tsz/diagnostics_report.rs"]
mod diagnostics_report;

#[cfg(test)]
#[path = "tsz/tests.rs"]
mod tests;

include!("tsz_parts/part1.rs");
include!("tsz_parts/part2.rs");
