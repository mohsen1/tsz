use crate::query_boundaries::diagnostics as query;

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_solver::TypeId;

include!("type_display_parts/part1.rs");
include!("type_display_parts/part2.rs");
