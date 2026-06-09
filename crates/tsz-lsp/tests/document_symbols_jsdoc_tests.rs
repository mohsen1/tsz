//! Document-symbol coverage for JSDoc `@typedef` / `@callback` type aliases.
//!
//! These exercise `jsdoc::collect_jsdoc_type_aliases` through the public
//! `DocumentSymbolProvider` surface, mirroring the harness in
//! `document_symbols_tests.rs`.

use super::*;
use tsz_common::position::LineMap;

fn parse_jsdoc_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn symbols_for(source: &str) -> Vec<DocumentSymbol> {
    let (parser, root) = parse_jsdoc_source(source);
    let line_map = LineMap::build(source);
    let provider = DocumentSymbolProvider::new(parser.get_arena(), &line_map, source);
    provider.get_document_symbols(root)
}

/// The text covered by `selection_range` must be exactly the symbol's name —
/// this validates that the byte-offset → position math is correct.
fn assert_selection_matches_name(source: &str, symbol: &DocumentSymbol) {
    let line_map = LineMap::build(source);
    let start = line_map
        .position_to_offset(symbol.selection_range.start, source)
        .expect("selection start maps to an offset");
    let end = line_map
        .position_to_offset(symbol.selection_range.end, source)
        .expect("selection end maps to an offset");
    let slice = &source[start as usize..end as usize];
    assert_eq!(
        slice, symbol.name,
        "selection_range should cover the name token"
    );
}

#[test]
fn typedef_with_braced_type_surfaces_as_type() {
    let source = "/** @typedef {number} Meters */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Meters");
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
    assert_eq!(symbols[0].kind.to_script_element_kind(), "type");
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn callback_surfaces_as_type() {
    let source = "/**\n * @callback OnClick\n * @param {number} x\n */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "OnClick");
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn typedef_without_type_uses_trailing_name() {
    // `@typedef Name` with the shape described via @property tags.
    let source = "/**\n * @typedef Point\n * @property {number} x\n * @property {number} y\n */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Point");
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn generic_typedef_name_drops_type_parameters() {
    let source = "/** @typedef {Array<T>} Box<T> */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Box");
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn typedef_type_then_name_on_following_line() {
    let source = "/**\n * @typedef {{ a: number, b: string }}\n * NamedShape\n */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "NamedShape");
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn object_wrapper_typedef_spanning_lines() {
    let source = "/**\n * @typedef {{\n *   a: number,\n *   b: string,\n * }} Wrapped\n */\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Wrapped");
    assert_selection_matches_name(source, &symbols[0]);
}

#[test]
fn multiple_typedefs_in_one_comment() {
    let source = "/**\n * @typedef {number} A\n * @typedef {string} B\n */\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
    assert!(symbols.iter().all(|s| s.kind == SymbolKind::Struct));
}

#[test]
fn typedef_inserted_in_source_order_with_declarations() {
    let source = "/** @typedef {number} Id */\nconst x = 1;\nfunction f() {}\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Id", "x", "f"]);
    assert_eq!(symbols[0].kind, SymbolKind::Struct);
}

#[test]
fn typedef_after_declaration_keeps_source_order() {
    let source = "const x = 1;\n/** @typedef {number} Id */\nfunction f() {}\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["x", "Id", "f"]);
}

#[test]
fn non_type_alias_tags_do_not_surface() {
    // A doc comment with only @param / @returns must not create symbols.
    let source =
        "/**\n * @param {number} x\n * @returns {number}\n */\nfunction f(x) { return x; }\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["f"]);
}

#[test]
fn typedef_lookalike_in_string_is_ignored() {
    // `@typedef` appearing in a string literal is not a comment, so no symbol.
    let source = "const s = \"@typedef {number} Nope\";\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["s"]);
}

#[test]
fn single_line_block_comment_is_not_jsdoc() {
    // `/* ... */` (single asterisk) is not a JSDoc comment.
    let source = "/* @typedef {number} Nope */\nconst x = 1;\n";
    let symbols = symbols_for(source);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["x"]);
}

#[test]
fn typedef_with_trailing_comment_terminator_glued_to_name() {
    let source = "/** @typedef {number} Glued*/\n";
    let symbols = symbols_for(source);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Glued");
    assert_selection_matches_name(source, &symbols[0]);
}
