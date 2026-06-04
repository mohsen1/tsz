use crate::context::TypingRequest;

use crate::query_boundaries::common::lazy_def_id;

use crate::state::CheckerState;

use tracing::trace;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use tsz_solver::{PropertyInfo, SymbolRef, TypeId};

type ImportQuerySegments = Vec<(NodeIndex, String)>;

include!("core_type_query_parts/part1.rs");
include!("core_type_query_parts/part2.rs");
