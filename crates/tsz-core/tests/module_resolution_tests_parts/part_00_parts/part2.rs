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

#[test]
fn test_import_default_and_named() {
    let source = r#"import React, { useState } from "./react";"#;
    let diags = check_with_resolved_modules(source, "app.tsx", vec!["./react"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Combined default + named import should resolve, got: {diags:?}"
    );
}

#[test]
fn test_import_default_and_namespace() {
    let source = r#"import React, * as ReactAll from "./react";"#;
    let mut parser = ParserState::new("app.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    // This is a parse error in TypeScript - can't combine default with namespace
    // Just verify it doesn't crash
}

#[test]
fn test_ambient_module_with_multiple_exports() {
    let source = r#"
declare module "my-lib" {
    export const VERSION: string;
    export function init(): void;
    export class Client {
        connect(): void;
    }
    export interface Config {
        apiKey: string;
    }
    export type Status = "active" | "inactive";
    export enum LogLevel { Debug, Info, Warn, Error }
}
"#;
    let mut parser = ParserState::new("types.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Ambient module with multiple exports should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.declared_modules.contains("my-lib"),
        "Declared module should be tracked"
    );
}

#[test]
fn test_resolution_maps_same_name_different_dirs() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec![
        "/project/src/utils.ts".to_string(),
        "/project/lib/utils.ts".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    // src/utils.ts -> ../lib/utils
    assert_eq!(
        paths.get(&(0, "../lib/utils".to_string())),
        Some(&1),
        "Same-name files in different dirs should resolve correctly"
    );
    // lib/utils.ts -> ../src/utils
    assert_eq!(
        paths.get(&(1, "../src/utils".to_string())),
        Some(&0),
        "Same-name files in different dirs should resolve correctly (reverse)"
    );
}

#[test]
fn test_resolution_maps_mixed_extensions() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec![
        "/project/main.ts".to_string(),
        "/project/lib.js".to_string(),
        "/project/types.d.ts".to_string(),
        "/project/component.tsx".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    // All should resolve with extensionless specifiers
    assert_eq!(paths.get(&(0, "./lib".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&2));
    assert_eq!(paths.get(&(0, "./component".to_string())), Some(&3));
}

#[test]
fn test_resolution_maps_only_single_file() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec!["/project/main.ts".to_string()];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert_eq!(paths.get(&(0, "./main".to_string())), Some(&0));
    assert_eq!(paths.get(&(0, "main".to_string())), Some(&0));
    assert!(modules.contains("./main"));
    assert!(modules.contains("main"));
}

#[test]
fn test_import_equals_require_extends_no_ts2304() {
    // Regression test: `class X extends Backbone.Model` should not produce
    // TS2304 when Backbone comes from `import Backbone = require("./backbone")`
    let source = r#"
import Backbone = require("./backbone");
class MyModel extends Backbone.Model {
    public age: number = 0;
}
"#;
    let module_source = r#"
export class Model {
    public name: string = "";
}
"#;
    let diags = check_with_module_sources(source, "main.ts", vec![("./backbone", module_source)]);
    let ts2304_errors: Vec<_> = diags.iter().filter(|(c, _)| *c == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should not emit TS2304 for 'extends Backbone.Model' with import = require, got: {ts2304_errors:?}"
    );
}
