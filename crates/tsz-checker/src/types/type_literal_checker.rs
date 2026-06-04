use super::type_node_helpers::type_node_includes_explicit_undefined;

use crate::state::{CheckerState, ParamTypeResolutionMode};

use crate::symbol_resolver::TypeSymbolResolution;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::FxHashMap;

use tsz_common::interner::Atom;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

use tsz_solver::Visibility;

include!("type_literal_checker_parts/part1.rs");
include!("type_literal_checker_parts/part2.rs");
