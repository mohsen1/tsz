use crate::query_boundaries::common::{collect_referenced_types, lazy_def_id};

use crate::query_boundaries::state::checking as query;

use crate::state::CheckerState;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;

use rustc_hash::FxHashSet;

use tsz_binder::SymbolId;

use tsz_common::comments::is_jsdoc_comment;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

include!("declaration_emit_parts/part1.rs");
include!("declaration_emit_parts/part2.rs");
