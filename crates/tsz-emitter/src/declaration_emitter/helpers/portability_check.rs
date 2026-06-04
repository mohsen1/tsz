#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};

#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;

#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};

#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};

#[allow(unused_imports)]
use std::sync::Arc;

#[allow(unused_imports)]
use tracing::debug;

#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};

#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

#[allow(unused_imports)]
use tsz_parser::parser::ParserState;

#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};

#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;

#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};

#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

/// Tracks visited nodes/types/symbols during portability traversal to prevent infinite recursion.
pub(in crate::declaration_emitter) struct PortabilityVisitState<'v> {
    pub visited_types: &'v mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
    pub visited_symbols: &'v mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    pub visited_declaration_symbols: &'v mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    pub visited_nodes: &'v mut rustc_hash::FxHashSet<(usize, u32)>,
}

/// Accumulates portability issues and deduplicates entries during traversal.
pub(in crate::declaration_emitter) struct PortabilityCollectState<'v> {
    pub results: &'v mut Vec<(String, String)>,
    pub seen: &'v mut rustc_hash::FxHashSet<(String, String)>,
}

include!("portability_check_parts/part1.rs");
include!("portability_check_parts/part2.rs");
