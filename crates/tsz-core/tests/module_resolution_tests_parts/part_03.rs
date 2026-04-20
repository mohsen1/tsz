#[test]
fn test_import_renamed_nonexistent_member() {
    let source = r#"import { nonexistent as alias } from "./utils";"#;
    let diags =
        check_with_module_exports(source, "main.ts", vec![("./utils", vec![("foo", 0)])], true);
    assert!(
        has_error_code(&diags, TS2305),
        "Renamed import of nonexistent member should emit TS2305, got: {diags:?}"
    );
}

// =============================================================================
// Declared / Ambient Module Tests
// =============================================================================

#[test]
fn test_declared_module_prevents_ts2307() {
    let source = r#"
declare module "my-lib" {
    export const value: number;
}
import { value } from "my-lib";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Declared module should prevent TS2307, got: {diags:?}"
    );
}

#[test]
fn test_declared_module_with_interface() {
    let source = r#"
declare module "my-lib" {
    export interface Config {
        name: string;
    }
}
import { Config } from "my-lib";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Declared module with interface should prevent TS2307, got: {diags:?}"
    );
}

#[test]
fn test_declared_module_recorded_in_binder() {
    let source = r#"
declare module "external-lib" {
    export const api: string;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.declared_modules.contains("external-lib"),
        "Binder should record declared modules"
    );
}

#[test]
fn test_shorthand_ambient_module_declaration() {
    // Shorthand ambient module (no body) - e.g., `declare module "*.css"`
    let source = r#"declare module "*.css";"#;
    let mut parser = ParserState::new("globals.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.shorthand_ambient_modules.contains("*.css"),
        "Binder should record shorthand ambient module '*.css'"
    );
}

#[test]
fn test_multiple_declared_modules() {
    let source = r#"
declare module "lib-a" {
    export const a: number;
}
declare module "lib-b" {
    export const b: string;
}
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.declared_modules.contains("lib-a"));
    assert!(binder.declared_modules.contains("lib-b"));
}

// =============================================================================
// Export Declaration Tests
// =============================================================================

#[test]
fn test_export_const_creates_symbol() {
    let source = r#"export const foo = 42;"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "Exported const should be in file_locals"
    );
}

#[test]
fn test_export_function_creates_symbol() {
    let source = r#"export function myFunc(): void {}"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("myFunc"),
        "Exported function should be in file_locals"
    );
}

#[test]
fn test_export_class_creates_symbol() {
    let source = r#"export class MyClass {}"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyClass"),
        "Exported class should be in file_locals"
    );
}

#[test]
fn test_export_interface_creates_symbol() {
    let source = r#"export interface MyInterface { value: number; }"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyInterface"),
        "Exported interface should be in file_locals"
    );
}

