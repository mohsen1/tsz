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

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_dynamic_import() {
    let source = r#"const m = import('./dynamic');"#;
    let links = get_links(source);
    // Dynamic imports may or may not produce document links depending on implementation
    // At minimum, should not panic
    let _ = links;
}

#[test]
fn test_import_with_extension() {
    let source = r#"import { foo } from './utils.ts';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./utils.ts"));
}

#[test]
fn test_import_with_index() {
    let source = r#"import { foo } from './components/index';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./components/index"));
}

#[test]
fn test_import_scoped_package() {
    let source = r#"import { something } from '@scope/package';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("@scope/package"));
}

#[test]
fn test_mixed_imports_and_code() {
    let source = r#"import { a } from './a';
const x = 1;
import { b } from './b';
const y = 2;
"#;
    let links = get_links(source);
    assert_eq!(links.len(), 2, "Should find imports mixed with code");
    assert_eq!(links[0].target.as_deref(), Some("./a"));
    assert_eq!(links[1].target.as_deref(), Some("./b"));
}

#[test]
fn test_link_tooltip_present() {
    let source = r#"import { foo } from './bar';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert!(
        links[0].tooltip.is_some(),
        "Link should have a tooltip for the module specifier"
    );
}

// =========================================================================
// Additional tests for broader coverage
// =========================================================================

#[test]
fn test_export_star_as_namespace() {
    let source = r#"export * as ns from './namespace';"#;
    let links = get_links(source);
    // Should find the module specifier link regardless of 'as ns'
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./namespace"));
}

#[test]
fn test_import_with_deep_relative_path() {
    let source = r#"import { util } from '../../../shared/utils';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("../../../shared/utils"));
}

#[test]
fn test_import_json_extension() {
    let source = r#"import config from './config.json';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./config.json"));
}

#[test]
fn test_import_with_js_extension() {
    let source = r#"import { helper } from './helper.js';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./helper.js"));
}

#[test]
fn test_require_with_relative_path() {
    let source = r#"const m = require('./local-module');"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./local-module"));
}

#[test]
fn test_multiple_named_exports_from() {
    let source = r#"export { a, b, c, d } from './multi';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./multi"));
}

#[test]
fn test_import_only_whitespace_source() {
    let source = "   \n  \n   ";
    let links = get_links(source);
    assert!(
        links.is_empty(),
        "Whitespace-only source should have no links"
    );
}

#[test]
fn test_import_with_multiline_formatting() {
    let source = r#"import {
    foo,
    bar,
    baz
} from './module';
"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./module"));
}

#[test]
fn test_export_type_from() {
    let source = r#"export type { MyInterface } from './interfaces';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("./interfaces"));
}

#[test]
fn test_combined_default_and_named_import() {
    let source = r#"import React, { useState } from 'react';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("react"));
}

#[test]
fn test_import_node_protocol() {
    let source = r#"import { readFile } from 'node:fs';"#;
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_deref(), Some("node:fs"));
}

#[test]
fn test_link_range_on_second_line() {
    let source = "const x = 1;\nimport { foo } from './bar';";
    let links = get_links(source);
    assert_eq!(links.len(), 1);
    // The link should be on line 1, not line 0
    assert_eq!(links[0].range.start.line, 1);
}

#[test]
fn test_many_imports_ordering() {
    let source = r#"import { a } from './a';
import { b } from './b';
import { c } from './c';
import { d } from './d';
import { e } from './e';
"#;
    let links = get_links(source);
    assert_eq!(links.len(), 5, "Should find five document links");
    assert_eq!(links[0].target.as_deref(), Some("./a"));
    assert_eq!(links[4].target.as_deref(), Some("./e"));
}

#[test]
fn test_export_default_expression_no_link() {
    let source = "export default 42;";
    let links = get_links(source);
    assert!(
        links.is_empty(),
        "export default expression should have no link"
    );
}

#[test]
fn test_dynamic_import_with_template_literal() {
    // Template literal dynamic import - should not panic
    let source = "const m = import(`./dynamic`);";
    let links = get_links(source);
    // May or may not produce a link depending on implementation
    let _ = links;
}
