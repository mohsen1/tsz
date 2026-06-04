use crate::context::TypingRequest;

use crate::state::CheckerState;

use crate::symbols_domain::name_text::entity_name_text_in_arena;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_solver::TypeId;

include!("resolution_parts/part1.rs");
include!("resolution_parts/part2.rs");
