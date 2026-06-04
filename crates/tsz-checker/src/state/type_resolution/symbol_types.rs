use crate::query_boundaries::state::type_resolution as query;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use crate::symbols_domain::name_text::{entity_name_text_in_arena, expression_name_text_in_arena};

use crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::node::{NodeAccess, NodeArena};

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

use tsz_solver::is_compiler_managed_type;

include!("symbol_types_parts/part1.rs");
include!("symbol_types_parts/part2.rs");
