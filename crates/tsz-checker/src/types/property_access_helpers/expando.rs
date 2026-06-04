use crate::context::is_js_file_name;

use crate::state::CheckerState;

use crate::symbols_domain::name_text::static_element_access_key_text_in_arena;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::NodeArena;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("expando_parts/part1.rs");
include!("expando_parts/part2.rs");
