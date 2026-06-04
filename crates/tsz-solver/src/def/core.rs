#[cfg(test)]
#[path = "../../tests/def_tests.rs"]
mod tests;

mod content_addressed;

mod definition_info;

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
