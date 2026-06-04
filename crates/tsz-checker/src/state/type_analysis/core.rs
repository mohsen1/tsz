use crate::context::TypingRequest;

use crate::query_boundaries::checkers::generic as generic_query;

use crate::query_boundaries::common::{lazy_def_id, type_param_info};

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use rustc_hash::FxHashSet;

use tracing::trace;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

type TypeParamPushResult = (
    Vec<tsz_solver::TypeParamInfo>,
    Vec<(String, Option<TypeId>, bool)>,
);

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
