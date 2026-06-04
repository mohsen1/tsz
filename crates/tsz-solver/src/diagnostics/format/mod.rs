mod array;

mod compound;

mod display_simplification;

mod intrinsic;

mod key;

mod property_names;

#[cfg(test)]
mod keyof_alias_display_tests;

#[cfg(all(test, debug_assertions))]
pub mod test_tracing;

#[cfg(test)]
mod tests;

pub mod tracing_helpers;

include!("mod_parts/part1.rs");
include!("mod_parts/part2.rs");
