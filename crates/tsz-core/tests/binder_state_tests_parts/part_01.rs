#[test]
fn test_scope_chain_traversal() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    // Test that identifier resolution walks the scope chain correctly.
    // - Global symbols (globalX, foo) should be in file_locals
    // - Local function symbols (localX) should be in the function's scope
    let source = r#"
const globalX = 100;

function foo() {
    const localX = 200;
    return localX;
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Global symbols should be in file_locals
    assert!(
        binder.file_locals.has("globalX"),
        "globalX should be in file_locals"
    );
    assert!(
        binder.file_locals.has("foo"),
        "foo function should be in file_locals"
    );

    // localX should NOT be in file_locals (it's a function-local variable)
    assert!(
        !binder.file_locals.has("localX"),
        "localX should NOT be in file_locals - it's a function-local variable"
    );

    // localX should be in the function's scope (via persistent scopes)
    // Find the function's scope and verify localX is there
    let mut found_local_x_in_function_scope = false;
    for (scope_idx, scope) in binder.scopes.iter().enumerate() {
        if scope.table.has("localX") {
            found_local_x_in_function_scope = true;
            // Verify the symbol has correct flags
            let local_x_sym_id = scope.table.get("localX").expect("localX exists");
            let local_x_symbol = binder.get_symbol(local_x_sym_id).expect("localX symbol");
            assert!(
                local_x_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
                "localX should be a block-scoped variable (const)"
            );
            // Verify this is not the root scope
            assert!(
                scope_idx > 0,
                "localX should be in a nested scope, not the root scope"
            );
            break;
        }
    }
    assert!(
        found_local_x_in_function_scope,
        "localX should be found in a function scope"
    );

    // Verify the global symbol has correct flags
    let global_x_sym_id = binder.file_locals.get("globalX").expect("globalX exists");
    let global_x_symbol = binder.get_symbol(global_x_sym_id).expect("globalX symbol");
    assert!(
        global_x_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "globalX should be a block-scoped variable (const)"
    );
}

#[test]
fn test_variable_shadowing() {
    use crate::binder::BinderState;

    let source = r#"
let x = "global";

function test() {
    let x = "local";
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let mut x_count = 0;
    let symbols = binder.get_symbols();
    for i in 0..symbols.len() {
        let id = crate::binder::SymbolId(i as u32);
        if let Some(sym) = symbols.get(id)
            && sym.escaped_name == "x"
        {
            x_count += 1;
        }
    }

    assert_eq!(x_count, 2);
}

/// Test that deeply nested functions create proper scope chains.
/// This verifies scope chain traversal works correctly through multiple levels.
#[test]
fn test_deeply_nested_scope_chain() {
    use crate::binder::BinderState;
    use crate::binder::ContainerKind;

    let source = r#"
const outerVar = 1;

function outer() {
    const middleVar = 2;

    function middle() {
        const innerVar = 3;

        function inner() {
            // This function should be able to reference all outer variables
            const localVar = outerVar + middleVar + innerVar;
            return localVar;
        }

        return inner();
    }

    return middle();
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Verify file_locals has the expected symbols
    assert!(
        binder.file_locals.has("outerVar"),
        "outerVar should be in file_locals"
    );
    assert!(
        binder.file_locals.has("outer"),
        "outer function should be in file_locals"
    );

    // Verify middleVar, innerVar, and localVar are NOT in file_locals (they're nested)
    assert!(
        !binder.file_locals.has("middleVar"),
        "middleVar should NOT be in file_locals"
    );
    assert!(
        !binder.file_locals.has("innerVar"),
        "innerVar should NOT be in file_locals"
    );
    assert!(
        !binder.file_locals.has("localVar"),
        "localVar should NOT be in file_locals"
    );
    assert!(
        !binder.file_locals.has("middle"),
        "middle function should NOT be in file_locals"
    );
    assert!(
        !binder.file_locals.has("inner"),
        "inner function should NOT be in file_locals"
    );

    // Verify we have multiple function scopes created
    let function_scope_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Function)
        .count();
    assert!(
        function_scope_count >= 3,
        "Should have at least 3 function scopes (outer, middle, inner)"
    );

    // Find the innermost scope and verify its parent chain exists
    let mut found_inner_scope = false;
    for scope in binder.scopes.iter() {
        if scope.table.has("localVar") {
            found_inner_scope = true;
            // Verify this scope has a parent
            assert!(
                scope.parent.is_some(),
                "innermost scope should have a parent"
            );
            break;
        }
    }
    assert!(
        found_inner_scope,
        "Should find the innermost scope with localVar"
    );

    // Verify each nested variable is in a different scope
    let mut scopes_with_vars = Vec::new();
    for (idx, scope) in binder.scopes.iter().enumerate() {
        if scope.table.has("middleVar")
            || scope.table.has("innerVar")
            || scope.table.has("localVar")
        {
            scopes_with_vars.push(idx);
        }
    }
    assert_eq!(
        scopes_with_vars.len(),
        3,
        "middleVar, innerVar, and localVar should each be in different scopes"
    );
}

/// Test that class methods can access outer scope variables.
#[test]
fn test_class_method_outer_scope_access() {
    use crate::binder::BinderState;

    let source = r#"
const globalConfig = { debug: true };

class MyClass {
    private value: number;

    constructor() {
        this.value = 0;
    }

    doSomething() {
        // Should be able to reference globalConfig from outer scope
        if (globalConfig.debug) {
            console.log(this.value);
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // globalConfig should be in file_locals
    assert!(
        binder.file_locals.has("globalConfig"),
        "globalConfig should be in file_locals"
    );
    assert!(
        binder.file_locals.has("MyClass"),
        "MyClass should be in file_locals"
    );

    // The class methods should create their own scopes
    // Verify we have class/constructor/method scopes
    assert!(
        binder.scopes.len() >= 2,
        "Should have multiple scopes for class and methods"
    );
}

#[test]
fn test_block_scope_let_const() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
{
    let blockScoped = "inside";
    const alsoScoped = "const";
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(!binder.file_locals.has("blockScoped"));
    assert!(!binder.file_locals.has("alsoScoped"));

    let mut found_block = false;
    let mut found_const = false;
    let symbols = binder.get_symbols();
    for i in 0..symbols.len() {
        let id = crate::binder::SymbolId(i as u32);
        if let Some(sym) = symbols.get(id) {
            if sym.escaped_name == "blockScoped" {
                found_block = true;
                assert!(sym.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0);
            }
            if sym.escaped_name == "alsoScoped" {
                found_const = true;
                assert!(sym.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0);
            }
        }
    }

    assert!(found_block);
    assert!(found_const);
}

#[test]
fn test_var_hoisting() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
function test() {
    {
        var x = "hoisted";
    }
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // x should NOT be in file_locals (it's a function-local variable)
    assert!(
        !binder.file_locals.has("x"),
        "x should NOT be in file_locals - it's a function-local variable"
    );

    // x should be in the function's scope (via persistent scopes)
    // Find the function's scope and verify x is there
    let mut found_x_in_function_scope = false;
    for (scope_idx, scope) in binder.scopes.iter().enumerate() {
        if scope.table.has("x") {
            found_x_in_function_scope = true;
            // Verify the symbol has correct flags
            let x_sym_id = scope.table.get("x").expect("x exists");
            let x_symbol = binder.get_symbol(x_sym_id).expect("x symbol");

            assert!(
                x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
                "x should be a function-scoped variable (var)"
            );
            assert!(
                x_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0,
                "x should NOT be block-scoped"
            );

            // Verify this is not the root scope
            assert!(
                scope_idx > 0,
                "x should be in a nested scope, not the root scope"
            );
            break;
        }
    }
    assert!(
        found_x_in_function_scope,
        "x should be found in a function scope"
    );
}

#[test]
fn test_imported_symbol_visibility() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
import { foo, bar as baz } from 'module';

function test() {
    return foo + baz;
}
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("foo"));
    assert!(binder.file_locals.has("baz"));
    assert!(!binder.file_locals.has("bar"));

    let foo_sym_id = binder.file_locals.get("foo").expect("foo should exist");
    let foo_symbol = binder
        .get_symbol(foo_sym_id)
        .expect("foo symbol should exist");
    assert!(foo_symbol.flags & symbol_flags::ALIAS != 0);
}

#[test]
fn test_default_import_visibility() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
import defaultExport from 'module';
const value = defaultExport;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("defaultExport"));

    let sym_id = binder
        .file_locals
        .get("defaultExport")
        .expect("defaultExport should exist");
    let symbol = binder.get_symbol(sym_id).expect("symbol should exist");
    assert!(symbol.flags & symbol_flags::ALIAS != 0);
}

#[test]
fn test_namespace_import_visibility() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
import * as ns from 'module';
const value = ns.foo;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("ns"));

    let sym_id = binder.file_locals.get("ns").expect("ns should exist");
    let symbol = binder.get_symbol(sym_id).expect("symbol should exist");
    assert!(symbol.flags & symbol_flags::ALIAS != 0);
}

#[test]
fn test_type_only_imports() {
    use crate::binder::BinderState;

    let source = r#"
import type { Type1, Type2 as Alias2 } from 'module';
import type Type3 from 'module';
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert!(binder.file_locals.has("Type1"));
    assert!(binder.file_locals.has("Alias2"));
    assert!(binder.file_locals.has("Type3"));

    let type1_sym_id = binder.file_locals.get("Type1").expect("Type1 should exist");
    let type1_symbol = binder
        .get_symbol(type1_sym_id)
        .expect("Type1 symbol should exist");
    assert!(type1_symbol.is_type_only);
}

#[test]
fn test_type_only_import_merges_with_local_value() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;

    let source = r#"
import type { A } from 'module';
const A: A = "a";
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
export { foo, bar as baz } from 'module';
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
import { importedFunc } from 'module';
export const localValue = importedFunc();
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
const x = 1;
function foo() { return x; }
class MyClass { value: number = 42; }
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = "const x = 1;";

    let (parser, root) = parse_test_source(source);

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

    let source = "const x = 1;";

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let fake_sym_id = crate::binder::SymbolId(99999);
    std::sync::Arc::make_mut(&mut binder.node_symbols).insert(100, fake_sym_id);

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

    let (parser, root) = parse_test_source(source);

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
    std::sync::Arc::make_mut(&mut importer_binder.module_exports).insert("./file1".to_string(), {
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
    std::sync::Arc::make_mut(&mut importer_binder.module_exports).insert("./file1".to_string(), {
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

    // Test that regular (non-import) symbols are not affected by import resolution
    let source = r#"
const localValue = 42;
function localFunction() { return localValue; }
"#;

    let (parser, root) = parse_test_source(source);
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

/// Test that multiple export * from statements are properly tracked.
/// This ensures we can resolve exports from files that re-export from multiple sources.
#[test]
fn test_multiple_wildcard_reexports() {
    use crate::binder::BinderState;

    // This file re-exports from multiple modules
    let source = r#"
export * from './moduleA';
export * from './moduleB';
export * from './moduleC';
"#;

    let mut parser = ParserState::new("index.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_debug_file("index.ts");
    binder.bind_source_file(arena, root);

    // Verify that all wildcard re-exports are tracked
    let reexports = binder.wildcard_reexports.get("index.ts");
    assert!(
        reexports.is_some(),
        "index.ts should have wildcard_reexports"
    );

    let reexports = reexports.unwrap();
    assert_eq!(
        reexports.len(),
        3,
        "Should have 3 wildcard re-exports, got: {reexports:?}"
    );

    assert!(
        reexports.contains(&"./moduleA".to_string()),
        "Should re-export from ./moduleA"
    );
    assert!(
        reexports.contains(&"./moduleB".to_string()),
        "Should re-export from ./moduleB"
    );
    assert!(
        reexports.contains(&"./moduleC".to_string()),
        "Should re-export from ./moduleC"
    );
}

/// Test that named re-exports and wildcard re-exports can coexist.
#[test]
fn test_mixed_reexports() {
    use crate::binder::BinderState;

    let source = r#"
export { foo, bar } from './named';
export * from './wildcard1';
export { baz as qux } from './renamed';
export * from './wildcard2';
"#;

    let mut parser = ParserState::new("mixed.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.set_debug_file("mixed.ts");
    binder.bind_source_file(arena, root);

    // Check wildcard re-exports
    let wildcards = binder.wildcard_reexports.get("mixed.ts");
    assert!(
        wildcards.is_some(),
        "mixed.ts should have wildcard_reexports"
    );
    let wildcards = wildcards.unwrap();
    assert_eq!(wildcards.len(), 2, "Should have 2 wildcard re-exports");
    assert!(wildcards.contains(&"./wildcard1".to_string()));
    assert!(wildcards.contains(&"./wildcard2".to_string()));

    // Check named re-exports
    let named = binder.reexports.get("mixed.ts");
    assert!(named.is_some(), "mixed.ts should have named reexports");
    let named = named.unwrap();
    assert!(named.contains_key("foo"), "Should re-export 'foo'");
    assert!(named.contains_key("bar"), "Should re-export 'bar'");
    assert!(
        named.contains_key("qux"),
        "Should re-export 'qux' (renamed from baz)"
    );
}

/// Test that export resolution follows multiple wildcard re-export chains.
#[test]
fn test_export_resolution_multiple_wildcards() {
    use crate::binder::{BinderState, SymbolTable, symbol_flags};
    use std::sync::Arc;

    // Setup: We'll create module_exports and wildcard_reexports manually
    // to test the resolution logic without parsing multiple files
    let source = "const x = 1;"; // Minimal source just to create a binder

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Setup module_exports for two modules
    let mut module_a_exports = SymbolTable::new();
    let sym_a = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcA".to_string());
    module_a_exports.set("funcA".to_string(), sym_a);
    std::sync::Arc::make_mut(&mut binder.module_exports)
        .insert("./moduleA".to_string(), module_a_exports);

    let mut module_b_exports = SymbolTable::new();
    let sym_b = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcB".to_string());
    module_b_exports.set("funcB".to_string(), sym_b);
    std::sync::Arc::make_mut(&mut binder.module_exports)
        .insert("./moduleB".to_string(), module_b_exports);

    // Setup index.ts to re-export from both
    Arc::make_mut(&mut binder.wildcard_reexports).insert(
        "./index".to_string(),
        vec!["./moduleA".to_string(), "./moduleB".to_string()],
    );

    // Test resolution: funcA should be found via index -> moduleA
    let result = binder.resolve_import_if_needed_public("./index", "funcA");
    assert!(result.is_some(), "Should resolve funcA from index");
    assert_eq!(result.unwrap(), sym_a);

    // Test resolution: funcB should be found via index -> moduleB
    let result = binder.resolve_import_if_needed_public("./index", "funcB");
    assert!(result.is_some(), "Should resolve funcB from index");
    assert_eq!(result.unwrap(), sym_b);

    // Test resolution: nonExistent should NOT be found
    let result = binder.resolve_import_if_needed_public("./index", "nonExistent");
    assert!(result.is_none(), "Should not resolve nonExistent");
}

// =============================================================================
// Lib Symbol Merging Tests (SymbolId Collision Fix)
// =============================================================================

/// Regression test: Two lib files with overlapping local `SymbolIds` must resolve
/// to the correct global symbols after merge.
///
/// This tests the fix for the SymbolId collision bug where lib files had
/// overlapping indices (both starting from SymbolId(0)), causing incorrect
/// symbol lookups.
#[test]
fn test_lib_symbol_merge_avoids_id_collision() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create two "lib" binders with overlapping SymbolIds
    // Lib 1: has "Object" at SymbolId(0) and "Array" at SymbolId(1)
    let mut lib1_binder = BinderState::new();
    let lib1_object_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Object".to_string());
    let lib1_array_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib1_binder
        .file_locals
        .set("Object".to_string(), lib1_object_id);
    lib1_binder
        .file_locals
        .set("Array".to_string(), lib1_array_id);

    // Lib 2: has "Promise" at SymbolId(0) and "Map" at SymbolId(1)
    // These IDs intentionally overlap with lib1's IDs!
    let mut lib2_binder = BinderState::new();
    let lib2_promise_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Promise".to_string());
    let lib2_map_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Map".to_string());
    lib2_binder
        .file_locals
        .set("Promise".to_string(), lib2_promise_id);
    lib2_binder.file_locals.set("Map".to_string(), lib2_map_id);

    // Verify the collision exists (both start at 0)
    assert_eq!(lib1_object_id.0, 0, "lib1 Object should be at ID 0");
    assert_eq!(
        lib2_promise_id.0, 0,
        "lib2 Promise should be at ID 0 (collision!)"
    );

    // Create a dummy NodeArena (needed for LibContext)
    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());

    // Create lib contexts
    let lib_contexts = vec![
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib1_binder),
        },
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib2_binder),
        },
    ];

    // Create main binder and merge lib symbols
    let mut main_binder = BinderState::new();
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // The flag should be set
    assert!(
        main_binder.lib_symbols_are_merged(),
        "lib_symbols_merged flag should be true"
    );

    // All four symbols should be in file_locals with UNIQUE IDs
    let object_id = main_binder
        .file_locals
        .get("Object")
        .expect("Object should exist");
    let array_id = main_binder
        .file_locals
        .get("Array")
        .expect("Array should exist");
    let promise_id = main_binder
        .file_locals
        .get("Promise")
        .expect("Promise should exist");
    let map_id = main_binder
        .file_locals
        .get("Map")
        .expect("Map should exist");

    // All IDs must be unique (the fix)
    let ids = [object_id, array_id, promise_id, map_id];
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        4,
        "All four symbols must have unique IDs after merge"
    );

    // Each symbol must resolve to correct data via get_symbol
    let object_sym = main_binder
        .get_symbol(object_id)
        .expect("Object symbol must resolve");
    assert_eq!(
        object_sym.escaped_name, "Object",
        "Object symbol name mismatch"
    );

    let promise_sym = main_binder
        .get_symbol(promise_id)
        .expect("Promise symbol must resolve");
    assert_eq!(
        promise_sym.escaped_name, "Promise",
        "Promise symbol name mismatch"
    );

    let array_sym = main_binder
        .get_symbol(array_id)
        .expect("Array symbol must resolve");
    assert_eq!(
        array_sym.escaped_name, "Array",
        "Array symbol name mismatch"
    );

    let map_sym = main_binder
        .get_symbol(map_id)
        .expect("Map symbol must resolve");
    assert_eq!(map_sym.escaped_name, "Map", "Map symbol name mismatch");
}

/// Test that symbol merging works correctly for interfaces with the same name
/// across lib files (declaration merging).
#[test]
fn test_lib_symbol_merge_declaration_merging() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create two lib binders, both declaring "Array" interface
    let mut lib1_binder = BinderState::new();
    let lib1_array_id = lib1_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib1_binder
        .file_locals
        .set("Array".to_string(), lib1_array_id);

    let mut lib2_binder = BinderState::new();
    let lib2_array_id = lib2_binder
        .symbols
        .alloc(symbol_flags::INTERFACE, "Array".to_string());
    lib2_binder
        .file_locals
        .set("Array".to_string(), lib2_array_id);

    // Create lib contexts
    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());
    let lib_contexts = vec![
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib1_binder),
        },
        LibContext {
            arena: Arc::clone(&dummy_arena),
            binder: Arc::new(lib2_binder),
        },
    ];

    // Merge
    let mut main_binder = BinderState::new();
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // Should have only ONE "Array" symbol (merged)
    let array_id = main_binder
        .file_locals
        .get("Array")
        .expect("Array should exist");
    let array_sym = main_binder
        .get_symbol(array_id)
        .expect("Array symbol must resolve");

    // The merged symbol should still be an interface
    assert!(
        (array_sym.flags & symbol_flags::INTERFACE) != 0,
        "Merged Array should be an interface"
    );
    assert_eq!(array_sym.escaped_name, "Array");
}

/// Test that user symbols take precedence over lib symbols.
#[test]
fn test_lib_symbol_merge_user_precedence() {
    use crate::binder::LibContext;
    use std::sync::Arc;

    // Create a lib binder with "myGlobal"
    let mut lib_binder = BinderState::new();
    let lib_sym_id = lib_binder
        .symbols
        .alloc(symbol_flags::VARIABLE, "myGlobal".to_string());
    lib_binder
        .file_locals
        .set("myGlobal".to_string(), lib_sym_id);

    let dummy_arena = Arc::new(crate::parser::node::NodeArena::new());
    let lib_contexts = vec![LibContext {
        arena: Arc::clone(&dummy_arena),
        binder: Arc::new(lib_binder),
    }];

    // Create main binder with a user-defined "myGlobal"
    let mut parser = ParserState::new("test.ts".to_string(), "const myGlobal = 42;".to_string());
    let root = parser.parse_source_file();
    let mut main_binder = BinderState::new();
    main_binder.bind_source_file(parser.get_arena(), root);

    // Capture user symbol ID
    let user_sym_id = main_binder
        .file_locals
        .get("myGlobal")
        .expect("User myGlobal should exist");

    // Now merge lib symbols
    main_binder.merge_lib_contexts_into_binder(&lib_contexts);

    // User symbol should still be in file_locals (not replaced by lib)
    let final_sym_id = main_binder
        .file_locals
        .get("myGlobal")
        .expect("myGlobal should exist");
    assert_eq!(
        final_sym_id, user_sym_id,
        "User symbol should take precedence over lib symbol"
    );
}
