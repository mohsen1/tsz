mod argument_reports;

mod display_types;

mod explicit_any_annotations;

mod generic_argument_suppression;

mod type_comparability;

include!("assignability_diagnostics_parts/part1.rs");
include!("assignability_diagnostics_parts/part2.rs");
