use super::super::*;

use tsz_parser::parser::NodeList;

include!("class_members_parts/part1.rs");
include!("class_members_parts/part2.rs");

#[cfg(test)]
#[path = "class_members_tests.rs"]
mod class_members_tests;
