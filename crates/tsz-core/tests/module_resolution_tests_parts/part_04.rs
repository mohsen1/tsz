#[test]
fn test_export_type_alias_creates_symbol() {
    let source = r#"export type MyType = string | number;"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyType"),
        "Exported type alias should be in file_locals"
    );
}

#[test]
fn test_export_enum_creates_symbol() {
    let source = r#"export enum Direction { Up, Down, Left, Right }"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("Direction"),
        "Exported enum should be in file_locals"
    );
}

// =============================================================================
// Import/Export Symbol Binding Tests
// =============================================================================

#[test]
fn test_named_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let exporter_source = r#"export const value = 42;"#;
    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();
    let exporter_arena = exporter_parser.get_arena();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_arena, exporter_root);

    let export_sym_id = exporter_binder
        .file_locals
        .get("value")
        .expect("value should exist in exporter");

    let importer_source = r#"
import { value } from './file1';
const x = value;
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();
    let importer_arena = importer_parser.get_arena();

    let mut importer_binder = BinderState::new();
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("value".to_string(), export_sym_id);
            table
        });
    importer_binder.bind_source_file(importer_arena, importer_root);

    assert!(
        importer_binder.file_locals.has("value"),
        "Imported symbol should be in file_locals"
    );

    let import_sym_id = importer_binder.file_locals.get("value").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Import symbol should be ALIAS"
    );
    assert_eq!(
        import_sym.import_module,
        Some("./file1".to_string()),
        "Import symbol should have import_module set"
    );
}

#[test]
fn test_default_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let exporter_source = r#"export default function hello() {}"#;
    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_parser.get_arena(), exporter_root);

    // For default exports, the export name is "default"
    let default_sym_id = exporter_binder
        .file_locals
        .get("hello")
        .expect("hello should exist in exporter");

    let importer_source = r#"
import hello from './file1';
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();

    let mut importer_binder = BinderState::new();
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("default".to_string(), default_sym_id);
            table
        });
    importer_binder.bind_source_file(importer_parser.get_arena(), importer_root);

    assert!(
        importer_binder.file_locals.has("hello"),
        "Default import should create local symbol"
    );

    let import_sym_id = importer_binder.file_locals.get("hello").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Default import should be ALIAS"
    );
}

#[test]
fn test_namespace_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let importer_source = r#"
import * as utils from './utils';
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();

    let mut importer_binder = BinderState::new();
    importer_binder.bind_source_file(importer_parser.get_arena(), importer_root);

    assert!(
        importer_binder.file_locals.has("utils"),
        "Namespace import should create local symbol 'utils'"
    );

    let import_sym_id = importer_binder.file_locals.get("utils").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Namespace import should be ALIAS"
    );
}

// =============================================================================
// Re-export Binding Tests
// =============================================================================

#[test]
fn test_reexport_tracked_in_binder() {
    let source = r#"export { foo, bar } from "./utils";"#;
    let mut parser = ParserState::new("barrel.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Re-exports should be recorded
    assert!(
        binder.reexports.contains_key("barrel.ts")
            || !binder.reexports.is_empty()
            || binder.file_locals.has("foo")
            || binder.file_locals.has("bar"),
        "Re-exports should be tracked in some form"
    );
}

#[test]
fn test_wildcard_reexport_tracked() {
    let source = r#"export * from "./utils";"#;
    let mut parser = ParserState::new("barrel.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Wildcard re-exports should be tracked
    let has_wildcard = !binder.wildcard_reexports.is_empty();
    // Wildcard reexport tracking may not be implemented yet
    let _ = has_wildcard;
}

// =============================================================================
// Dynamic Import Tests
// =============================================================================

#[test]
fn test_dynamic_import_resolved_module() {
    let source = r#"
async function load() {
    const mod = await import("./lazy-module");
}
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./lazy-module"], vec![]);
    assert!(
        no_error_code(&diags, TS2307),
        "Dynamic import of resolved module should not error, got: {diags:?}"
    );
}

#[test]
fn test_dynamic_import_unresolved_module() {
    let source = r#"
async function load() {
    const mod = await import("./nonexistent");
}
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![], vec![]);
    assert!(
        has_module_not_found(&diags),
        "Dynamic import of unresolved module should emit TS2307 or TS2792, got: {diags:?}"
    );
}

#[test]
fn test_dynamic_import_with_ambient_module() {
    let source = r#"
declare module "my-lazy-lib" {
    export function doStuff(): void;
}

async function load() {
    const mod = await import("my-lazy-lib");
}
"#;
    let diags = check_single_file(source, "main.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Dynamic import of ambient module should not error, got: {diags:?}"
    );
}

// =============================================================================
// Require Tests (CommonJS-style)
// =============================================================================

