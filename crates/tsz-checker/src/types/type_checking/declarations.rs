use crate::query_boundaries::type_checking as query;

use crate::state::CheckerState;

use tsz_binder::SymbolId;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("declarations_parts/part1.rs");
include!("declarations_parts/part2.rs");
