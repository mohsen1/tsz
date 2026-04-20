#[test]
fn test_import_default_and_namespace() {
    let source = r#"import React, * as ReactAll from "./react";"#;
    let mut parser = ParserState::new("app.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    // This is a parse error in TypeScript - can't combine default with namespace
    // Just verify it doesn't crash
}

// =============================================================================
// Module Declaration with Body Tests
// =============================================================================

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

// =============================================================================
// build_module_resolution_maps edge cases
// =============================================================================

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

// =============================================================================
// Heritage Clause with Import = Require Tests
// =============================================================================

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

#[test]
fn test_import_equals_require_new_expression_no_ts2304() {
    // Test that `new Backbone.Model()` works when Backbone is from import = require
    let source = r#"
import Backbone = require("./backbone");
const m = new Backbone.Model();
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
        "Should not emit TS2304 for 'new Backbone.Model()' with import = require, got: {ts2304_errors:?}"
    );
}

// TODO: Emit TS2304/TS2339 for extends with non-existent property on import=require namespace.
#[test]
fn test_import_equals_require_extends_nonexistent_still_errors() {
    // Negative test: extends with non-existent export should still produce an error
    let source = r#"
import Backbone = require("./backbone");
class Bad extends Backbone.NonExistent {
    x: number = 0;
}
"#;
    let module_source = r#"
export class Model {
    public name: string = "";
}
"#;
    let diags = check_with_module_sources(source, "main.ts", vec![("./backbone", module_source)]);
    // Should have some error (TS2304 for unresolved name or TS2339 for missing property)
    assert!(
        !diags.is_empty(),
        "Should emit error for extends Backbone.NonExistent, got no diagnostics"
    );
}
