use super::super::DeclarationEmitter;

use rustc_hash::FxHashMap;

use tsz_binder::{SymbolId, symbol_flags};

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

use tsz_solver::type_queries;

use tsz_solver::types::TypeId;

include!("type_inference_source_callables_parts/part1.rs");
include!("type_inference_source_callables_parts/part2.rs");
