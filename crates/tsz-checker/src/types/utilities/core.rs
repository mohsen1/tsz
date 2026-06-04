use crate::query_boundaries::type_checking_utilities as query;

use crate::state::{CheckerState, EnumKind};

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

/// Result from resolving literal string keys against an object type.
pub(crate) struct LiteralKeysResult {
    /// The computed result type (union/intersection of found key types).
    /// `None` only when the lookup itself failed (e.g., object was unknown).
    pub result_type: Option<TypeId>,
    /// Keys that were not found as properties on the object type.
    /// When non-empty, the caller should emit TS2339 for each.
    pub missing_keys: Vec<String>,
}

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
