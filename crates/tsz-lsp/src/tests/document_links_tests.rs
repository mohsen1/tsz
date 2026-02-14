use super::*;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

/// Helper: parse source, build line map, and collect document links.
fn get_links(source: &str) -> Vec<DocumentLink> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);
    let provider = DocumentLinkProvider::new(arena, &line_map, source);
    provider.provide_document_links(root)
}

#[test]
fn test_simple_import() {
    let source = r#"import { foo } from './utils';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1, "Should find one document link");
    assert_eq!(links[0].target.as_deref(), Some("./utils"));
    assert!(links[0].tooltip.is_some());
}

#[test]
fn test_default_import() {
    let source = r#"import React from 'react';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("react"));
}

#[test]
fn test_namespace_import() {
    let source = r#"import * as fs from 'fs';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("fs"));
}

#[test]
fn test_side_effect_import() {
    let source = r#"import './polyfills';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./polyfills"));
}

#[test]
fn test_export_from() {
    let source = r#"export { foo } from './bar';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./bar"));
}

#[test]
fn test_export_star() {
    let source = r#"export * from './all';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./all"));
}

#[test]
fn test_multiple_imports() {
    let source = r#"import { a } from './a';
import { b } from './b';
export { c } from './c';
"#;
    let links = get_links(source);

    assert_eq!(links.len(), 3, "Should find three document links");
    assert_eq!(links[0].target.as_deref(), Some("./a"));
    assert_eq!(links[1].target.as_deref(), Some("./b"));
    assert_eq!(links[2].target.as_deref(), Some("./c"));
}

#[test]
fn test_no_imports() {
    let source = "const x = 1;\nlet y = 2;\n";
    let links = get_links(source);

    assert!(links.is_empty(), "Should find no document links");
}

#[test]
fn test_empty_source() {
    let source = "";
    let links = get_links(source);

    assert!(
        links.is_empty(),
        "Should find no document links in empty source"
    );
}

#[test]
fn test_link_range_excludes_quotes() {
    let source = r#"import './utils';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    let link = &links[0];

    // The specifier './utils' starts at column 8 (after the quote)
    // and ends before the closing quote.
    // "import './utils';"
    //  0123456789...
    // The string literal node spans from col 7 to col 16 (includes quotes)
    // The inner text range should be col 8 to col 15
    assert_eq!(link.range.start.line, 0);
    assert_eq!(link.range.start.character, 8);
    assert_eq!(link.range.end.line, 0);
    assert_eq!(link.range.end.character, 15);
}

#[test]
fn test_export_without_from() {
    // `export { foo }` with no `from` clause should have no link
    let source = r#"export { foo };"#;
    let links = get_links(source);

    assert!(
        links.is_empty(),
        "Should find no links for export without from"
    );
}

#[test]
fn test_export_default_no_link() {
    // `export default ...` has no module specifier
    let source = "export default function foo() {}";
    let links = get_links(source);

    assert!(
        links.is_empty(),
        "Should find no links for export default declaration"
    );
}

#[test]
fn test_require_call() {
    let source = r#"const fs = require('fs');"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("fs"));
}

#[test]
fn test_double_quoted_import() {
    let source = r#"import { foo } from "./utils";"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./utils"));
}

#[test]
fn test_type_import() {
    let source = r#"import type { MyType } from './types';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./types"));
}

#[test]
fn test_re_export_with_rename() {
    let source = r#"export { foo as bar } from './module';"#;
    let links = get_links(source);

    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./module"));
}
