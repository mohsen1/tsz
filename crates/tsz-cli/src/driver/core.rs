#[path = "sources.rs"]
mod sources;

#[path = "check.rs"]
mod check;

#[path = "check_module_graph.rs"]
mod check_module_graph;

#[path = "check_utils.rs"]
mod check_utils;

#[path = "config_deprecation.rs"]
mod config_deprecation;

#[path = "plan.rs"]
mod plan;

#[cfg(test)]
#[path = "config_deprecation_tests.rs"]
mod config_deprecation_tests;

#[cfg(test)]
#[path = "cross_file_circular_alias_tests.rs"]
mod cross_file_circular_alias_tests;

#[cfg(test)]
#[path = "explain_files_reason_tests.rs"]
mod explain_files_reason_tests;

#[cfg(test)]
#[path = "core_merge_cache_tests.rs"]
mod merge_cache_tests;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[path = "diagnostic_source.rs"]
mod diagnostic_source;

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
include!("core_parts/part3.rs");
