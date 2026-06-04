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

pub(in crate::declaration_emitter) struct CallableDeclParts<'b> {
    pub(in crate::declaration_emitter) modifiers: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) type_parameters: Option<&'b NodeList>,
    pub(in crate::declaration_emitter) parameters: &'b NodeList,
    pub(in crate::declaration_emitter) type_annotation: NodeIndex,
    pub(in crate::declaration_emitter) body: NodeIndex,
}

struct ImportedMethodRef<'a> {
    imported_module: &'a str,
    imported_name: &'a str,
    method_name: &'a str,
}

include!("type_inference_parts/part1.rs");
include!("type_inference_parts/part2.rs");

#[cfg(test)]
#[path = "type_inference_tests.rs"]
mod tests;
