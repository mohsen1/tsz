use super::handlers_code_fixes_utils::{
    class_body_has_member, find_first_implements_class, parse_interface_properties,
    parse_named_import_map, positions_overlap, resolve_module_path,
};

use super::{Server, TsServerRequest, TsServerResponse};

use tsz::checker::diagnostics::DiagnosticCategory;

use tsz::lsp::code_actions::{
    CodeActionContext, CodeActionKind, CodeActionProvider, CodeFixRegistry,
};

use tsz::lsp::position::LineMap;

const FIX_MISSING_TYPE_ANNOTATION_FIX_ID: &str = "fixMissingTypeAnnotationOnExports";

include!("handlers_code_fixes_parts/part1.rs");
include!("handlers_code_fixes_parts/part2.rs");

#[cfg(test)]
#[path = "handlers_code_fixes_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "handlers_code_fixes_tests_part2.rs"]
mod tests_part2;

#[cfg(test)]
#[path = "handlers_code_fixes_nested_pkg_tests.rs"]
mod nested_pkg_tests;
