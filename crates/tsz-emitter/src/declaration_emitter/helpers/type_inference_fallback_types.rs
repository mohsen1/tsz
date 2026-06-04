use super::super::DeclarationEmitter;

use serde_json::Value;

use std::path::{Path, PathBuf};

use std::sync::Arc;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy)]
enum JsonImportBindingKind {
    Default,
    Namespace,
}

struct JsonImportBinding {
    module_specifier: String,
    kind: JsonImportBindingKind,
}

include!("type_inference_fallback_types_parts/part1.rs");
include!("type_inference_fallback_types_parts/part2.rs");
