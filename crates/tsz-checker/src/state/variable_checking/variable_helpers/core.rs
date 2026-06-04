use crate::state::CheckerState;

use rustc_hash::FxHashSet;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("core_parts/part1.rs");
include!("core_parts/part2.rs");
