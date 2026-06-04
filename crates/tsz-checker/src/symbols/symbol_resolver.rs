use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use crate::symbols_domain::name_text::entity_name_text_in_arena;

use std::sync::Arc;

use tracing::trace;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::is_compiler_managed_type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSymbolResolution {
    Type(SymbolId),
    ValueOnly(SymbolId),
    NotFound,
}

include!("symbol_resolver_parts/part1.rs");
include!("symbol_resolver_parts/part2.rs");
