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

use super::{DtsCacheResolver, DtsStructuralResolver};

pub(crate) struct ResolvedDeclarationTypeText {
    pub(crate) type_id: tsz_solver::types::TypeId,
    pub(crate) canonical_type_text: String,
    pub(crate) emitted_type_text: String,
}

include!("type_printing_parts/part1.rs");
include!("type_printing_parts/part2.rs");
