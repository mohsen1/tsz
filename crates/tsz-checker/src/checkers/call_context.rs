use crate::computation::complex::is_contextually_sensitive;

use crate::context::TypingRequest;

use crate::query_boundaries::checkers::call as call_checker;

use crate::query_boundaries::common;

use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("call_context_parts/part1.rs");
include!("call_context_parts/part2.rs");
