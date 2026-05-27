use crate::resolver::ScopeWalker;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_scanner::SyntaxKind;
fn bind_test_source(
    source: &str,
) -> (
    tsz_parser::ParserState,
    tsz_parser::parser::NodeIndex,
    BinderState,
) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    (parser, root, binder)
}

include!("resolver_tests_parts/part_00.rs");
include!("resolver_tests_parts/part_01.rs");
