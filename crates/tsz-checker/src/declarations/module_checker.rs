use crate::state::CheckerState;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_parser::parser::NodeIndex;

use tsz_solver::Visibility;

mod verbatim_module_syntax;

include!("module_checker_parts/part1.rs");
include!("module_checker_parts/part2.rs");
