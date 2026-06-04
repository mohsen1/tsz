use crate::context::TypingRequest;

use crate::query_boundaries::assignability::{
    get_function_return_type, replace_function_return_type,
};

use crate::query_boundaries::common as query_common;

use crate::state::CheckerState;

use crate::symbol_resolver::TypeSymbolResolution;

use rustc_hash::FxHashMap;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("display_formatting_parts/part1.rs");
include!("display_formatting_parts/part2.rs");
