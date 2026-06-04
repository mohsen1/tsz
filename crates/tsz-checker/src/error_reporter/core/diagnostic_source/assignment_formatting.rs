use super::literal_widening_helpers::{
    literal_display_appropriate_for_undefined_null_target, target_accepts_literal_primitive_kind,
};

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_solver::TypeId;

include!("assignment_formatting_parts/part1.rs");
include!("assignment_formatting_parts/part2.rs");
