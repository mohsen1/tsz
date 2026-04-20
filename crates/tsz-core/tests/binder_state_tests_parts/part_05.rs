#[test]
fn test_type_only_import_merges_with_local_value() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
import type { A } from 'module';
const A: A = "a";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let a_sym_id = binder.file_locals.get("A").expect("A should exist");
    let a_symbol = binder.get_symbol(a_sym_id).expect("A symbol should exist");

    // The merged symbol should have both ALIAS and VALUE flags
    assert!(
        (a_symbol.flags & symbol_flags::ALIAS) != 0,
        "A should have ALIAS flag from import type, flags: {:#x}",
        a_symbol.flags
    );
    assert!(
        (a_symbol.flags & symbol_flags::VALUE) != 0,
        "A should have VALUE flag from const declaration, flags: {:#x}",
        a_symbol.flags
    );
    // is_type_only should still be true (from the import), but the VALUE flag
    // means the checker will not treat it as type-only in value contexts
    assert!(a_symbol.is_type_only);
}

#[test]
fn test_re_export_from_module() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
export { foo, bar as baz } from 'module';
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("foo"));
    assert!(binder.file_locals.has("baz"));

    let foo_sym_id = binder.file_locals.get("foo").expect("foo should exist");
    let foo_symbol = binder
        .get_symbol(foo_sym_id)
        .expect("foo symbol should exist");
    assert!(foo_symbol.flags & symbol_flags::ALIAS != 0);
}

#[test]
fn test_import_and_export_in_same_file() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
import { importedFunc } from 'module';
export const localValue = importedFunc();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("importedFunc"));
    assert!(binder.file_locals.has("localValue"));

    let local_value_sym_id = binder
        .file_locals
        .get("localValue")
        .expect("localValue should exist");
    let local_value_symbol = binder
        .get_symbol(local_value_sym_id)
        .expect("localValue symbol should exist");
    assert!(local_value_symbol.is_exported);
}

#[test]
fn test_symbol_table_validation_valid_code() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
const x = 1;
function foo() { return x; }
class MyClass { value: number = 42; }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let errors = binder.validate_symbol_table();
    assert!(
        errors.is_empty(),
        "Valid code should have no validation errors: {errors:?}"
    );
    assert!(binder.is_symbol_table_valid());
}

#[test]
fn test_symbol_table_validation_detects_orphans() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let _orphan_id = binder.symbols.alloc(
        crate::binder::symbol_flags::BLOCK_SCOPED_VARIABLE,
        "orphan".to_string(),
    );

    let errors = binder.validate_symbol_table();
    assert!(!errors.is_empty());

    let orphan_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, crate::binder::ValidationError::OrphanedSymbol { .. }))
        .collect();
    assert_eq!(orphan_errors.len(), 1);
}

#[test]
fn test_symbol_table_validation_broken_links() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let fake_sym_id = crate::binder::SymbolId(99999);
    binder.node_symbols.insert(100, fake_sym_id);

    let errors = binder.validate_symbol_table();
    assert!(!errors.is_empty());

    let link_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, crate::binder::ValidationError::BrokenSymbolLink { .. }))
        .collect();
    assert_eq!(link_errors.len(), 1);
}

#[test]
fn test_closure_variable_capture() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    // Test that variables declared in outer scopes are resolvable inside closures.
    // This is the core issue for Task 2.
    let source = r#"
const outerX = 100;

const arrowFn = () => {
    return outerX;
};

const functionExpr = function() {
    return outerX;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // outerX should be in file_locals (module scope)
    assert!(
        binder.file_locals.has("outerX"),
        "outerX should be in file_locals"
    );

    // Verify scope chain is set up correctly
    // There should be multiple scopes: module scope, arrow function scope, function expression scope
    assert!(
        binder.scopes.len() >= 3,
        "Should have at least module scope + 2 closure scopes"
    );

    // Find the arrow function scope
    let mut arrow_fn_scope = None;
    let mut function_expr_scope = None;

    for (idx, scope) in binder.scopes.iter().enumerate() {
        if scope.kind == crate::binder::ContainerKind::Function {
            if arrow_fn_scope.is_none() {
                arrow_fn_scope = Some(idx);
            } else {
                function_expr_scope = Some(idx);
            }
        }
    }

    // Both closures should have scopes
    assert!(
        arrow_fn_scope.is_some(),
        "Arrow function should have a scope"
    );
    assert!(
        function_expr_scope.is_some(),
        "Function expression should have a scope"
    );

    // Verify that the parent chain points to module scope
    if let Some(arrow_idx) = arrow_fn_scope {
        let arrow_scope = &binder.scopes[arrow_idx];
        // The parent of arrow function should be module scope (index 0)
        assert_eq!(
            arrow_scope.parent,
            crate::binder::ScopeId(0),
            "Arrow function parent should be module scope"
        );
    }
}

// =============================================================================
// Module Import Resolution Tests (Worker-7 Task)
// =============================================================================

#[test]
fn test_module_import_resolution_basic_named_import() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    // Simulate file1.ts which exports symbols
    let exporter_source = r#"
export const foo = 42;
export const bar = "hello";
"#;

    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();
    let exporter_arena = exporter_parser.get_arena();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_arena, exporter_root);

    // Get the exported symbol IDs from file1
    let foo_export_sym_id = exporter_binder
        .file_locals
        .get("foo")
        .expect("foo should exist in exporter");
    let bar_export_sym_id = exporter_binder
        .file_locals
        .get("bar")
        .expect("bar should exist in exporter");

    // Simulate file2.ts which imports from file1
    let importer_source = r#"
import { foo, bar } from './file1';
const x = foo;
const y = bar;
"#;

    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();
    let importer_arena = importer_parser.get_arena();

    let mut importer_binder = BinderState::new();

    // Populate module_exports with the exported symbols from file1
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("foo".to_string(), foo_export_sym_id);
            table.set("bar".to_string(), bar_export_sym_id);
            table
        });

    importer_binder.bind_source_file(importer_arena, importer_root);

    // Verify that import symbols were created
    assert!(
        importer_binder.file_locals.has("foo"),
        "foo should be in importer's file_locals"
    );
    assert!(
        importer_binder.file_locals.has("bar"),
        "bar should be in importer's file_locals"
    );

    // Get the import symbol IDs
    let foo_import_sym_id = importer_binder
        .file_locals
        .get("foo")
        .expect("foo import should exist");
    let bar_import_sym_id = importer_binder
        .file_locals
        .get("bar")
        .expect("bar import should exist");

    // Verify they are ALIAS symbols (import symbols)
    let foo_import_sym = importer_binder
        .get_symbol(foo_import_sym_id)
        .expect("foo import symbol should exist");
    let bar_import_sym = importer_binder
        .get_symbol(bar_import_sym_id)
        .expect("bar import symbol should exist");

    assert!(
        foo_import_sym.flags & symbol_flags::ALIAS != 0,
        "foo import should be an ALIAS symbol"
    );
    assert!(
        bar_import_sym.flags & symbol_flags::ALIAS != 0,
        "bar import should be an ALIAS symbol"
    );

    // Verify import_module is set correctly
    assert_eq!(
        foo_import_sym.import_module,
        Some("./file1".to_string()),
        "foo import should have import_module set"
    );
    assert_eq!(
        bar_import_sym.import_module,
        Some("./file1".to_string()),
        "bar import should have import_module set"
    );
}

#[test]
fn test_module_import_resolution_renamed_import() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    // Simulate file1.ts which exports a symbol
    let exporter_source = r#"
export const originalValue = 42;
"#;

    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();
    let exporter_arena = exporter_parser.get_arena();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_arena, exporter_root);

    // Get the exported symbol ID from file1
    let export_sym_id = exporter_binder
        .file_locals
        .get("originalValue")
        .expect("originalValue should exist in exporter");

    // Simulate file2.ts which imports with a renamed alias
    let importer_source = r#"
import { originalValue as aliasedValue } from './file1';
const x = aliasedValue;
"#;

    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();
    let importer_arena = importer_parser.get_arena();

    let mut importer_binder = BinderState::new();

    // Populate module_exports with the exported symbol from file1
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("originalValue".to_string(), export_sym_id);
            table
        });

    importer_binder.bind_source_file(importer_arena, importer_root);

    // Verify that the renamed import symbol was created
    assert!(
        importer_binder.file_locals.has("aliasedValue"),
        "aliasedValue should be in importer's file_locals"
    );

    // Get the import symbol ID
    let import_sym_id = importer_binder
        .file_locals
        .get("aliasedValue")
        .expect("aliasedValue import should exist");

    // Verify it's an ALIAS symbol
    let import_sym = importer_binder
        .get_symbol(import_sym_id)
        .expect("aliasedValue import symbol should exist");
    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "aliasedValue import should be an ALIAS symbol"
    );

    // Verify import_module and import_name are set correctly
    assert_eq!(
        import_sym.import_module,
        Some("./file1".to_string()),
        "import should have import_module set"
    );
    assert_eq!(
        import_sym.import_name,
        Some("originalValue".to_string()),
        "import should have import_name set to original name"
    );
    assert_eq!(
        import_sym.escaped_name, "aliasedValue",
        "symbol's escaped_name should be the local alias"
    );
}

#[test]
fn test_module_import_resolution_non_import_symbol_unchanged() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    // Test that regular (non-import) symbols are not affected by import resolution
    let source = r#"
const localValue = 42;
function localFunction() { return localValue; }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Verify local symbols exist
    assert!(
        binder.file_locals.has("localValue"),
        "localValue should exist"
    );
    assert!(
        binder.file_locals.has("localFunction"),
        "localFunction should exist"
    );

    // Get the symbol IDs
    let value_sym_id = binder
        .file_locals
        .get("localValue")
        .expect("localValue should exist");
    let func_sym_id = binder
        .file_locals
        .get("localFunction")
        .expect("localFunction should exist");

    // Verify they are NOT import symbols (no import_module set)
    let value_sym = binder
        .get_symbol(value_sym_id)
        .expect("localValue symbol should exist");
    let func_sym = binder
        .get_symbol(func_sym_id)
        .expect("localFunction symbol should exist");

    assert_eq!(
        value_sym.import_module, None,
        "localValue should not have import_module set"
    );
    assert_eq!(
        func_sym.import_module, None,
        "localFunction should not have import_module set"
    );

    assert!(
        value_sym.flags & symbol_flags::ALIAS == 0,
        "localValue should not be an ALIAS symbol"
    );
    assert!(
        func_sym.flags & symbol_flags::ALIAS == 0,
        "localFunction should not be an ALIAS symbol"
    );
}

// =============================================================================
// Wildcard Re-export Tests
// =============================================================================

