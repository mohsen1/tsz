#[cfg(test)]
#[path = "emit_tests.rs"]
mod emit_tests;

mod emit_output_helpers;

include!("emit_parts/part1.rs");
include!("emit_parts/part2.rs");
