#[cfg(test)]
#[path = "../../tests/compat_tests.rs"]
mod tests;

#[path = "compat_mapped.rs"]
mod compat_mapped;

#[path = "compat_weak.rs"]
mod compat_weak;

include!("compat_parts/part1.rs");
include!("compat_parts/part2.rs");
