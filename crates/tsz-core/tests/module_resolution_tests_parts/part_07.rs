#[test]
fn test_mixed_import_require_same_file() {
    // Using both ES imports and require in the same file
    let source = r#"
import { foo } from "./utils";
const bar = require("./utils");
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Mixed import/require should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "ES import binding should exist"
    );
    assert!(
        binder.file_locals.has("bar"),
        "require binding should exist"
    );
}

#[test]
fn test_import_and_reexport_same_module() {
    let source = r#"
import { foo } from "./utils";
export { bar } from "./utils";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import and re-export from same module should resolve, got: {diags:?}"
    );
}

// =============================================================================
// Module with Different Extension Specifiers
// =============================================================================

#[test]
fn test_import_with_js_extension() {
    // TypeScript allows importing with .js extension (resolves to .ts)
    let source = r#"import { foo } from "./utils.js";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils.js"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import with .js extension should resolve, got: {diags:?}"
    );
}

#[test]
fn test_import_with_ts_extension() {
    // Importing with .ts extension is unusual but parseable
    let source = r#"import { foo } from "./utils.ts";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils.ts"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import with .ts extension should resolve when in resolved set, got: {diags:?}"
    );
}

// =============================================================================
// Re-export Chain Tests
// =============================================================================

#[test]
fn test_barrel_file_exports() {
    let source = r#"
export { Button } from "./components/Button";
export { Input } from "./components/Input";
export { Form } from "./components/Form";
"#;
    let diags = check_with_resolved_modules(
        source,
        "index.ts",
        vec![
            "./components/Button",
            "./components/Input",
            "./components/Form",
        ],
        vec![],
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Barrel file re-exports should resolve, got: {diags:?}"
    );
}

#[test]
fn test_wildcard_reexport_with_named_reexport() {
    let source = r#"
export * from "./base";
export { special } from "./special";
"#;
    let diags =
        check_with_resolved_modules(source, "index.ts", vec!["./base", "./special"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Mixed wildcard and named re-exports should resolve, got: {diags:?}"
    );
}

// =============================================================================
// Ambient Module Wildcard Pattern Tests
// =============================================================================

#[test]
fn test_wildcard_ambient_module_css() {
    let source = r#"
declare module "*.css" {
    const styles: { [key: string]: string };
    export default styles;
}
import styles from "./app.css";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .css imports, got: {diags:?}"
    );
}

#[test]
fn test_wildcard_ambient_module_svg() {
    let source = r#"
declare module "*.svg" {
    const content: string;
    export default content;
}
import logo from "./logo.svg";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .svg imports, got: {diags:?}"
    );
}

#[test]
fn test_wildcard_ambient_module_json() {
    let source = r#"
declare module "*.json" {
    const data: any;
    export default data;
}
import data from "./config.json";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .json imports, got: {diags:?}"
    );
}

// =============================================================================
// Import with Complex Clauses
// =============================================================================

#[test]
fn test_import_default_and_named() {
    let source = r#"import React, { useState } from "./react";"#;
    let diags = check_with_resolved_modules(source, "app.tsx", vec!["./react"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Combined default + named import should resolve, got: {diags:?}"
    );
}

