use crate::query_boundaries::state::type_resolution as query;

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::node::{NodeAccess, NodeArena};

use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("reference_helpers_parts/part1.rs");
include!("reference_helpers_parts/part2.rs");
