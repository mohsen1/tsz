use crate::context::TypingRequest;

use crate::query_boundaries::common::PropertyAccessResult;

use crate::query_boundaries::property_access as access_query;

use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};

use tsz_binder::symbol_flags;

use tsz_common::common::Visibility;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::{AccessExprData, NodeAccess};

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("helpers_parts/part1.rs");
include!("helpers_parts/part2.rs");
