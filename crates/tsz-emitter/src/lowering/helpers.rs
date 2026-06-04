use super::*;

use crate::emitter::JsxEmit;

use crate::transforms::emit_utils;

use tsz_common::ScriptTarget;

use tsz_parser::parser::node::NodeAccess;

include!("helpers_parts/part1.rs");
include!("helpers_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/lowering_helpers.rs"]
mod tests;
