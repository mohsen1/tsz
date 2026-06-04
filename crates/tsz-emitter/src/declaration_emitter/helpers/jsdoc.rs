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

use super::jsdoc_function_signature::{
    JsdocFunctionTypeSignature, parse_jsdoc_function_type_signature,
};

use super::{
    JsdocOverloadSignature, JsdocParamDecl, JsdocTypeAliasDecl, escape_string_for_double_quote,
};

include!("jsdoc_parts/part1.rs");
include!("jsdoc_parts/part2.rs");

#[path = "jsdoc/function_facts.rs"]
mod function_facts;

#[path = "jsdoc/type_aliases.rs"]
mod type_aliases;

#[cfg(test)]
#[path = "jsdoc_tests.rs"]
mod jsdoc_tests;
