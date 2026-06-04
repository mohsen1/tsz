use crate::query_boundaries::checkers::promise as query;

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{BinderState, Symbol, SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{NodeAccess, NodeArena};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::narrowing as solver_narrowing;

#[derive(Default)]
struct ThenableAwaitInfo {
    awaited_type: Option<TypeId>,
    rejected_this_type: Option<TypeId>,
    has_callable_then: bool,
}

const MAX_THENABLE_THIS_VALIDATION_DEPTH: u8 = 10;

include!("promise_checker_parts/part1.rs");
include!("promise_checker_parts/part2.rs");
