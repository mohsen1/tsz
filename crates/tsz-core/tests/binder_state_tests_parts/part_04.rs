#[test]
fn test_scope_chain_traversal() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
let x = "global";

function test() {
    let x = "local";
    return x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
{
    let blockScoped = "inside";
    const alsoScoped = "const";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
function test() {
    {
        var x = "hoisted";
    }
    return x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
import { foo, bar as baz } from 'module';

function test() {
    return foo + baz;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
import defaultExport from 'module';
const value = defaultExport;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
import * as ns from 'module';
const value = ns.foo;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
import type { Type1, Type2 as Alias2 } from 'module';
import type Type3 from 'module';
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

