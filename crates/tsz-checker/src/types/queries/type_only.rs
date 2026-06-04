use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("type_only_parts/part1.rs");
include!("type_only_parts/part2.rs");
