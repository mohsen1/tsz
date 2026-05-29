use super::*;
#[cfg(debug_assertions)]
use crate::output::source_writer::DelimiterKind;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

/// Parse, lower, and print a source string with the given options.
///
/// Convenience wrapper for tests that don't need access to the parser
/// arena. Uses `"test.ts"` as the file name and returns the printed code.
fn parse_lower_print(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

include!("printer/basic_downlevel.rs");
include!("printer/modules_namespaces_recovery.rs");
include!("printer/private_fields_and_helpers.rs");
