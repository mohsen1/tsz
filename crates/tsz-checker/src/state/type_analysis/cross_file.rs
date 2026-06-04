use crate::state::CheckerState;

use crate::symbols_domain::name_text::expression_name_text_in_arena;

use crate::types_domain::queries::lib_resolution::keyword_syntax_to_type_id;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_common::perf_counters::{
    CrossArenaAliasShortcutOutcome, CrossArenaSymbolMissKind, CrossArenaSymbolMissSource,
};

use tsz_parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

pub(crate) use super::cross_file_query_types::CrossFileQueryKind;

include!("cross_file_parts/part1.rs");
include!("cross_file_parts/part2.rs");

include!("cross_file_miss_kind.rs");

#[cfg(test)]
#[path = "cross_file_query_kind_tests.rs"]
mod cross_file_query_kind_tests;

#[cfg(test)]
#[path = "cross_file_cache_tests.rs"]
mod tests;
