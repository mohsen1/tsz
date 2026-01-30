//! Tests for Binder
//!
//! This module contains tests for the binder implementation, organized into sections:
//! - Basic declarations (variables, functions, classes, interfaces, etc.)
//! - Import/export binding
//! - Scope resolution and parameter binding
//! - Namespace and enum exports
//! - Symbol merging (namespace/class/function/enum merging)
//! - Scope chain traversal
//! - Module import resolution

use crate::binder::{BinderState, symbol_flags};
use crate::parser::ParserState;

// =============================================================================
// Basic Declaration Binding Tests
// =============================================================================

#[test]
fn test_binder_variable_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x = 1; const y = 2; var z = 3;".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that symbols were created
    assert!(binder.file_locals.has("x"));
    assert!(binder.file_locals.has("y"));
    assert!(binder.file_locals.has("z"));
}

#[test]
fn test_binder_reset_clears_state() {
    let mut parser = ParserState::new("test.ts".to_string(), "const a = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.has("a"));
    assert!(!binder.symbols.is_empty());
    assert!(!binder.node_symbols.is_empty());

    binder.reset();

    assert!(binder.file_locals.is_empty());
    assert!(binder.symbols.is_empty());
    assert!(binder.node_symbols.is_empty());
    assert_eq!(binder.flow_nodes.len(), 1);

    let mut parser = ParserState::new("test.ts".to_string(), "const b = 2;".to_string());
    let root = parser.parse_source_file();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.has("b"));
    assert!(!binder.file_locals.has("a"));
}

#[test]
fn test_binder_function_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function foo(a: number, b: string) { return a; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that function symbol was created
    assert!(binder.file_locals.has("foo"));
}

#[test]
fn test_binder_class_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class MyClass { x: number; foo() {} }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that class symbol was created
    assert!(binder.file_locals.has("MyClass"));
}

#[test]
fn test_binder_interface_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface IFoo { x: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that interface symbol was created
    assert!(binder.file_locals.has("IFoo"));
}

#[test]
fn test_binder_type_alias() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type MyType = string | number;".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that type alias symbol was created
    assert!(binder.file_locals.has("MyType"));
}

#[test]
fn test_binder_enum_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "enum Color { Red, Green, Blue }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Check that enum symbol was created
    assert!(binder.file_locals.has("Color"));
}

// =============================================================================
// Import/Export Binding Tests
// =============================================================================

#[test]
fn test_binder_import_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import foo from 'module'; import { bar, baz as qux } from 'other';"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Default import creates alias symbol
    assert!(binder.file_locals.has("foo"));
    // Named imports create alias symbols
    assert!(binder.file_locals.has("bar"));
    assert!(binder.file_locals.has("qux")); // aliased from baz
}

#[test]
fn test_binder_export_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"const x = 1; export { x, x as y };"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Variable should be bound
    assert!(binder.file_locals.has("x"));

    // Export specifiers should have symbols (marked via node_symbols, not file_locals)
    // This ensures the binding runs without errors
    assert!(
        binder.symbols.len() > 1,
        "Should have created export symbols"
    );
}

#[test]
fn test_binder_exported_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export function foo() { return 1; }"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Exported function should be bound to file_locals
    assert!(
        binder.file_locals.has("foo"),
        "Exported function 'foo' should be in file_locals"
    );
}

#[test]
fn test_binder_exported_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export class MyClass { x: number; }"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Exported class should be bound to file_locals
    assert!(
        binder.file_locals.has("MyClass"),
        "Exported class 'MyClass' should be in file_locals"
    );
}

#[test]
fn test_binder_exported_const() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export const x = 1, y = 2;"#.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Exported variables should be bound to file_locals
    assert!(
        binder.file_locals.has("x"),
        "Exported const 'x' should be in file_locals"
    );
    assert!(
        binder.file_locals.has("y"),
        "Exported const 'y' should be in file_locals"
    );
}

// =============================================================================
// Scope Resolution and Parameter Binding Tests
// =============================================================================

/// Helper function to verify that a parameter reference in a function body
/// correctly resolves to the parameter's symbol.
///
/// This is a core test for scope chain resolution - when we reference `node`
/// inside a function body, it should resolve to the `node` parameter, not
/// a global or any other symbol.
///
/// # Arguments
/// * `source` - TypeScript source code containing a function
/// * `function_name` - Name of the function to analyze
/// * `param_name` - Name of the parameter to verify resolution for
/// * `include_scopes` - Whether to include scope information in the binder
fn assert_bound_state_resolves_param_impl(
    source: &str,
    function_name: &str,
    param_name: &str,
    include_scopes: bool,
) {
    use crate::binder::SymbolTable;
    use crate::parallel;
    use crate::parser::{NodeIndex, syntax_kind_ext};

    let program = parallel::compile_files(vec![("test.ts".to_string(), source.to_string())]);
    let file = &program.files[0];

    let mut file_locals = SymbolTable::new();
    for (name, &sym_id) in program.file_locals[0].iter() {
        file_locals.set(name.clone(), sym_id);
    }
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let binder = if include_scopes {
        BinderState::from_bound_state_with_scopes(
            program.symbols.clone(),
            file_locals,
            file.node_symbols.clone(),
            file.scopes.clone(),
            file.node_scope_ids.clone(),
        )
    } else {
        BinderState::from_bound_state(
            program.symbols.clone(),
            file_locals,
            file.node_symbols.clone(),
        )
    };

    let arena = &file.arena;
    let mut param_name_idx = NodeIndex::NONE;
    let mut param_symbol = None;
    let mut function_body = NodeIndex::NONE;

    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            continue;
        }
        let Some(func) = arena.get_function(node) else {
            continue;
        };
        let name = arena
            .get(func.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str());
        if name != Some(function_name) {
            continue;
        }
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };
            let param_text = arena
                .get(param.name)
                .and_then(|param_name_node| arena.get_identifier(param_name_node))
                .map(|ident| ident.escaped_text.as_str());
            if param_text != Some(param_name) {
                continue;
            }
            param_name_idx = param.name;
            param_symbol = binder.get_node_symbol(param.name);
            function_body = func.body;
            break;
        }
        break;
    }

    assert!(
        !param_name_idx.is_none(),
        "Expected to find parameter name for {function_name}"
    );
    assert!(
        param_symbol.is_some(),
        "Expected parameter symbol for {function_name}"
    );
    assert!(
        !function_body.is_none(),
        "Expected function body for {function_name}"
    );

    let mut usage_idx = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if idx == param_name_idx {
            continue;
        }
        let Some(node) = arena.get(idx) else {
            continue;
        };
        let Some(ident) = arena.get_identifier(node) else {
            continue;
        };
        if ident.escaped_text != param_name {
            continue;
        }
        let mut current = idx;
        let mut in_body = false;
        while !current.is_none() {
            if current == function_body {
                in_body = true;
                break;
            }
            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }
        if in_body {
            usage_idx = idx;
            break;
        }
    }

    assert!(
        !usage_idx.is_none(),
        "Expected a '{param_name}' identifier inside the function body"
    );

    let resolved = binder.resolve_identifier(arena, usage_idx);
    assert_eq!(
        resolved, param_symbol,
        "Expected body identifier to resolve to the parameter symbol"
    );
}

/// Wrapper for `assert_bound_state_resolves_param_impl` with scopes enabled.
fn assert_bound_state_resolves_param(source: &str, function_name: &str, param_name: &str) {
    assert_bound_state_resolves_param_impl(source, function_name, param_name, true);
}

/// Wrapper for `assert_bound_state_resolves_param_impl` without scopes.
/// Used to test fallback resolution when scope information is not available.
fn assert_bound_state_resolves_param_without_scopes(
    source: &str,
    function_name: &str,
    param_name: &str,
) {
    assert_bound_state_resolves_param_impl(source, function_name, param_name, false);
}

/// Checks if a node is within a container by walking up the parent chain.
fn node_is_within(
    arena: &crate::parser::node::NodeArena,
    node_idx: crate::parser::NodeIndex,
    container: crate::parser::NodeIndex,
) -> bool {
    let mut current = node_idx;
    while !current.is_none() {
        if current == container {
            return true;
        }
        let Some(ext) = arena.get_extended(current) else {
            break;
        };
        current = ext.parent;
    }
    false
}

#[test]
fn test_binder_resolves_parameter_from_bound_state() {
    let source = r#"
export function f(node: number) {
    return node;
}
"#;

    assert_bound_state_resolves_param(source, "f", "node");
}

#[test]
fn test_binder_resolves_parameter_from_bound_state_module_instance_state() {
    let source = r#"
export function getModuleInstanceState(node: { body?: { parent?: {} } }) {
    if (node.body && !node.body.parent) {
        return node.body;
    }
    return node.body;
}
"#;

    assert_bound_state_resolves_param(source, "getModuleInstanceState", "node");
}

#[test]
fn test_binder_resolves_parameter_from_bound_state_module_instance_state_with_visited() {
    let source = r#"
export function getModuleInstanceState(
    node: { body?: { parent?: {} } },
    visited?: Map<number, unknown>
) {
    if (node.body && !node.body.parent) {
        return node.body;
    }
    return node.body;
}
"#;

    assert_bound_state_resolves_param(source, "getModuleInstanceState", "node");
}

#[test]
fn test_binder_resolves_parameter_from_bound_state_binder_ts_331() {
    let source = r#"
export function getModuleInstanceState(node: ModuleDeclaration, visited?: Map<number, ModuleInstanceState | undefined>): ModuleInstanceState {
    if (node.body && !node.body.parent) {
        setParent(node.body, node);
        setParentRecursive(node.body, /*incremental*/ false);
    }
    return node.body ? getModuleInstanceStateCached(node.body, visited) : ModuleInstanceState.Instantiated;
}
"#;

    assert_bound_state_resolves_param(source, "getModuleInstanceState", "node");
}

#[test]
fn test_binder_resolves_parameter_from_bound_state_binder_ts_331_without_scopes() {
    let source = r#"
export function getModuleInstanceState(node: ModuleDeclaration, visited?: Map<number, ModuleInstanceState | undefined>): ModuleInstanceState {
    if (node.body && !node.body.parent) {
        setParent(node.body, node);
        setParentRecursive(node.body, /*incremental*/ false);
    }
    return node.body ? getModuleInstanceStateCached(node.body, visited) : ModuleInstanceState.Instantiated;
}
"#;

    assert_bound_state_resolves_param_without_scopes(source, "getModuleInstanceState", "node");
}

#[test]
fn test_binder_resolves_block_local_from_bound_state_binder_ts_432() {
    use crate::binder::SymbolTable;
    use crate::parallel;
    use crate::parser::{NodeIndex, syntax_kind_ext};

    let source = r#"
export function getModuleInstanceStateForAliasTarget(
    specifier: ExportSpecifier,
    visited: Map<number, ModuleInstanceState | undefined>
) {
    const name = specifier.propertyName || specifier.name;
    let p: Node | undefined = specifier.parent;
    while (p) {
        if (isBlock(p) || isModuleBlock(p) || isSourceFile(p)) {
            const statements = p.statements;
            let found: ModuleInstanceState | undefined;
            for (const statement of statements) {
                if (nodeHasName(statement, name)) {
                    return found;
                }
            }
        }
        p = p.parent;
    }
    return ModuleInstanceState.Instantiated;
}
"#;

    let program = parallel::compile_files(vec![("test.ts".to_string(), source.to_string())]);
    let file = &program.files[0];

    let mut file_locals = SymbolTable::new();
    for (name, &sym_id) in program.file_locals[0].iter() {
        file_locals.set(name.clone(), sym_id);
    }
    for (name, &sym_id) in program.globals.iter() {
        if !file_locals.has(name) {
            file_locals.set(name.clone(), sym_id);
        }
    }

    let binder = BinderState::from_bound_state_with_scopes(
        program.symbols.clone(),
        file_locals,
        file.node_symbols.clone(),
        file.scopes.clone(),
        file.node_scope_ids.clone(),
    );

    let arena = &file.arena;
    let mut function_body = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            continue;
        }
        let Some(func) = arena.get_function(node) else {
            continue;
        };
        let name = arena
            .get(func.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str());
        if name == Some("getModuleInstanceStateForAliasTarget") {
            function_body = func.body;
            break;
        }
    }

    assert!(
        !function_body.is_none(),
        "Expected function body for getModuleInstanceStateForAliasTarget"
    );

    let mut decl_name_idx = NodeIndex::NONE;
    let mut decl_symbol = None;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            continue;
        }
        let Some(decl) = arena.get_variable_declaration(node) else {
            continue;
        };
        let name = arena
            .get(decl.name)
            .and_then(|name_node| arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str());
        if name != Some("statements") {
            continue;
        }
        if !node_is_within(arena, idx, function_body) {
            continue;
        }
        decl_name_idx = decl.name;
        decl_symbol = binder.get_node_symbol(decl.name);
        break;
    }

    assert!(
        !decl_name_idx.is_none(),
        "Expected declaration for statements"
    );
    assert!(
        decl_symbol.is_some(),
        "Expected symbol for statements declaration"
    );

    let mut usage_idx = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            continue;
        }
        if !node_is_within(arena, idx, function_body) {
            continue;
        }
        let Some(for_data) = arena.get_for_in_of(node) else {
            continue;
        };
        let Some(expr_node) = arena.get(for_data.expression) else {
            continue;
        };
        let Some(ident) = arena.get_identifier(expr_node) else {
            continue;
        };
        if ident.escaped_text == "statements" {
            usage_idx = for_data.expression;
            break;
        }
    }

    assert!(
        !usage_idx.is_none(),
        "Expected for-of expression to reference statements"
    );

    let resolved = binder.resolve_identifier(arena, usage_idx);
    assert_eq!(
        resolved, decl_symbol,
        "Expected for-of expression to resolve to statements declaration"
    );
}

// =============================================================================
// Namespace and Enum Export Tests
// =============================================================================

#[test]
fn test_namespace_binding_debug() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace foo {
    export class Provide {}
    class NotExported {}
    export function bar() {}
    function baz() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Check if 'foo' was bound
    let foo_sym_id = binder
        .file_locals
        .get("foo")
        .expect("'foo' should be in file_locals");
    let foo_symbol = binder
        .get_symbol(foo_sym_id)
        .expect("foo symbol should exist");

    // Check if exports were captured
    assert!(foo_symbol.exports.is_some(), "foo should have exports");

    // Check that ONLY exported members are in exports
    let exports = foo_symbol.exports.as_ref().unwrap();

    // Exported members should be present
    assert!(
        exports.get("Provide").is_some(),
        "Provide should be in foo's exports"
    );
    assert!(
        exports.get("bar").is_some(),
        "bar should be in foo's exports"
    );

    // Non-exported members should NOT be in exports
    assert!(
        exports.get("NotExported").is_none(),
        "NotExported should NOT be in foo's exports"
    );
    assert!(
        exports.get("baz").is_none(),
        "baz should NOT be in foo's exports"
    );

    // Should have exactly 2 exports
    assert_eq!(exports.len(), 2, "foo should have exactly 2 exports");
}

#[test]
fn test_import_alias_binding() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export class C {}
}
import Alias = NS.C;
var x: Alias;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Check that 'Alias' was bound as an ALIAS symbol
    let alias_sym_id = binder
        .file_locals
        .get("Alias")
        .expect("'Alias' should be in file_locals");
    let alias_symbol = binder
        .get_symbol(alias_sym_id)
        .expect("Alias symbol should exist");

    // Verify it has the ALIAS flag
    assert_eq!(
        alias_symbol.flags & symbol_flags::ALIAS,
        symbol_flags::ALIAS,
        "Alias should have ALIAS flag"
    );

    // Verify it has a declaration
    assert!(
        !alias_symbol.declarations.is_empty(),
        "Alias should have declarations"
    );
}

// =============================================================================
// Symbol Merging Tests (Namespace/Class/Function/Enum)
// =============================================================================

#[test]
fn test_namespace_exports_merge_across_decls() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const a = 1;
    const hidden = 2;
}
namespace Merge {
    export function foo() {}
    export const b = 3;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(exports.get("a").is_some(), "a should be in Merge exports");
    assert!(exports.get("b").is_some(), "b should be in Merge exports");
    assert!(
        exports.get("foo").is_some(),
        "foo should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 3, "Merge should have exactly 3 exports");
}

#[test]
fn test_class_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
class Merge {
    constructor() {}
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_namespace_class_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
class Merge {
    constructor() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_function_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
function Merge() {}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_namespace_function_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
function Merge() {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    assert_eq!(exports.len(), 1, "Merge should have exactly 1 export");
}

#[test]
fn test_enum_namespace_merge_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    // Enum members should be in exports when merging with namespace (TypeScript behavior)
    assert!(exports.get("A").is_some(), "A should be in Merge exports");
    assert_eq!(
        exports.len(),
        2,
        "Merge should have exactly 2 exports (enum member A + namespace export extra)"
    );
}

#[test]
fn test_namespace_enum_merge_exports_reverse_order() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
enum Merge {
    A,
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let merge_sym_id = binder
        .file_locals
        .get("Merge")
        .expect("'Merge' should be in file_locals");
    let merge_symbol = binder
        .get_symbol(merge_sym_id)
        .expect("Merge symbol should exist");

    let exports = merge_symbol
        .exports
        .as_ref()
        .expect("Merge should have exports");
    assert!(
        exports.get("extra").is_some(),
        "extra should be in Merge exports"
    );
    assert!(
        exports.get("hidden").is_none(),
        "hidden should not be in Merge exports"
    );
    // Enum members should be in exports when merging with namespace (TypeScript behavior)
    assert!(exports.get("A").is_some(), "A should be in Merge exports");
    assert_eq!(
        exports.len(),
        2,
        "Merge should have exactly 2 exports (namespace export extra + enum member A)"
    );
}

// =============================================================================
// Performance and Edge Case Tests
// =============================================================================

/// Tests that deeply nested binary expressions don't cause stack overflow.
/// Uses 50,000 chained additions to stress test the binding walk.
#[test]
fn test_binder_deep_binary_expression() {
    const COUNT: usize = 50000;
    let mut source = String::with_capacity(COUNT * 4);
    for i in 0..COUNT {
        if i > 0 {
            source.push_str(" + ");
        }
        source.push('0');
    }
    source.push(';');

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.file_locals.is_empty());
}

// =============================================================================
// Namespace Member Resolution Tests
// =============================================================================

#[test]
fn test_namespace_member_resolution_basic() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const x = 1;
    export function foo() { return 2; }
    export class Bar { value: number = 3; }
    export enum Color { Red, Green }
}

const a = NS.x;
const b = NS.foo();
const c = new NS.Bar();
const d = NS.Color.Red;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Namespace should be bound
    assert!(
        binder.file_locals.has("NS"),
        "NS namespace should be in file_locals"
    );

    let ns_sym_id = binder.file_locals.get("NS").expect("NS should exist");
    let ns_symbol = binder
        .get_symbol(ns_sym_id)
        .expect("NS symbol should exist");

    // Namespace should have exports
    assert!(ns_symbol.exports.is_some(), "NS should have exports");
    let exports = ns_symbol.exports.as_ref().unwrap();

    // All exported members should be in exports
    assert!(exports.get("x").is_some(), "x should be in NS exports");
    assert!(exports.get("foo").is_some(), "foo should be in NS exports");
    assert!(exports.get("Bar").is_some(), "Bar should be in NS exports");
    assert!(
        exports.get("Color").is_some(),
        "Color should be in NS exports"
    );
}

#[test]
fn test_namespace_member_resolution_nested() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export const value = 42;
        export function getDouble() { return value * 2; }
    }
}

const a = Outer.Inner.value;
const b = Outer.Inner.getDouble();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Outer namespace should be bound
    assert!(
        binder.file_locals.has("Outer"),
        "Outer namespace should be in file_locals"
    );

    let outer_sym_id = binder.file_locals.get("Outer").expect("Outer should exist");
    let outer_symbol = binder
        .get_symbol(outer_sym_id)
        .expect("Outer symbol should exist");

    // Outer should have Inner in its exports
    assert!(outer_symbol.exports.is_some(), "Outer should have exports");
    let outer_exports = outer_symbol.exports.as_ref().unwrap();
    assert!(
        outer_exports.get("Inner").is_some(),
        "Inner should be in Outer exports"
    );
}

#[test]
fn test_namespace_member_resolution_non_exported() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const exported = 1;
    const notExported = 2;
}

const a = NS.exported;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let ns_sym_id = binder.file_locals.get("NS").expect("NS should exist");
    let ns_symbol = binder
        .get_symbol(ns_sym_id)
        .expect("NS symbol should exist");

    // Only exported members should be in exports
    let exports = ns_symbol.exports.as_ref().expect("NS should have exports");
    assert!(
        exports.get("exported").is_some(),
        "exported should be in NS exports"
    );
    assert!(
        exports.get("notExported").is_none(),
        "notExported should NOT be in NS exports"
    );
}

#[test]
fn test_namespace_deep_chain_resolution() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace A {
    export namespace B {
        export namespace C {
            export const deepValue = "deep";
            export function deepFunc() { return "func"; }
        }
    }
}

const a = A.B.C.deepValue;
const b = A.B.C.deepFunc();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // A namespace should be bound
    assert!(
        binder.file_locals.has("A"),
        "A namespace should be in file_locals"
    );

    let a_sym_id = binder.file_locals.get("A").expect("A should exist");
    let a_symbol = binder.get_symbol(a_sym_id).expect("A symbol should exist");

    // A should have B in exports
    let a_exports = a_symbol.exports.as_ref().expect("A should have exports");
    assert!(a_exports.get("B").is_some(), "B should be in A exports");
}

#[test]
fn test_enum_member_access() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Color {
    Red,
    Green,
    Blue
}

const a = Color.Red;
const b = Color.Green;
const c = Color.Blue;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Color enum should be bound
    assert!(
        binder.file_locals.has("Color"),
        "Color enum should be in file_locals"
    );

    let color_sym_id = binder.file_locals.get("Color").expect("Color should exist");
    let color_symbol = binder
        .get_symbol(color_sym_id)
        .expect("Color symbol should exist");

    // Enum should have members in exports
    assert!(color_symbol.exports.is_some(), "Color should have exports");
    let exports = color_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Red").is_some(),
        "Red should be in Color exports"
    );
    assert!(
        exports.get("Green").is_some(),
        "Green should be in Color exports"
    );
    assert!(
        exports.get("Blue").is_some(),
        "Blue should be in Color exports"
    );
}

#[test]
fn test_enum_namespace_merging_access() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Direction {
    Up = 1,
    Down = 2
}
namespace Direction {
    export function getName(d: Direction): string {
        return d === Direction.Up ? "Up" : "Down";
    }
    export const helperValue = 99;
}

const a = Direction.Up;
const b = Direction.getName(Direction.Down);
const c = Direction.helperValue;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Direction should be bound (merged enum and namespace)
    assert!(
        binder.file_locals.has("Direction"),
        "Direction should be in file_locals"
    );

    let dir_sym_id = binder
        .file_locals
        .get("Direction")
        .expect("Direction should exist");
    let dir_symbol = binder
        .get_symbol(dir_sym_id)
        .expect("Direction symbol should exist");

    // Direction should have both enum members and namespace exports
    assert!(
        dir_symbol.exports.is_some(),
        "Direction should have exports"
    );
    let exports = dir_symbol.exports.as_ref().unwrap();

    // Enum members
    assert!(
        exports.get("Up").is_some(),
        "Up should be in Direction exports"
    );
    assert!(
        exports.get("Down").is_some(),
        "Down should be in Direction exports"
    );

    // Namespace exports
    assert!(
        exports.get("getName").is_some(),
        "getName should be in Direction exports"
    );
    assert!(
        exports.get("helperValue").is_some(),
        "helperValue should be in Direction exports"
    );
}

#[test]
fn test_enum_with_initialized_members() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum Status {
    Pending = 0,
    Active = 1,
    Done = 2
}

const a = Status.Pending;
const b = Status.Active;
const c = Status.Done;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Status enum should be bound
    assert!(
        binder.file_locals.has("Status"),
        "Status enum should be in file_locals"
    );

    let status_sym_id = binder
        .file_locals
        .get("Status")
        .expect("Status should exist");
    let status_symbol = binder
        .get_symbol(status_sym_id)
        .expect("Status symbol should exist");

    // Enum should have all members in exports
    assert!(
        status_symbol.exports.is_some(),
        "Status should have exports"
    );
    let exports = status_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Pending").is_some(),
        "Pending should be in Status exports"
    );
    assert!(
        exports.get("Active").is_some(),
        "Active should be in Status exports"
    );
    assert!(
        exports.get("Done").is_some(),
        "Done should be in Status exports"
    );
}

#[test]
fn test_const_enum_declaration() {
    use crate::binder::BinderState;
    use crate::binder::symbol_flags;
    use crate::parser::ParserState;

    let source = r#"
const enum Priority {
    Low = 1,
    Medium = 2,
    High = 3
}

const a = Priority.Low;
const b = Priority.Medium;
const c = Priority.High;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Priority const enum should be bound
    assert!(
        binder.file_locals.has("Priority"),
        "Priority const enum should be in file_locals"
    );

    let priority_sym_id = binder
        .file_locals
        .get("Priority")
        .expect("Priority should exist");
    let priority_symbol = binder
        .get_symbol(priority_sym_id)
        .expect("Priority symbol should exist");

    // Should have CONST_ENUM flag
    assert_eq!(
        priority_symbol.flags & symbol_flags::CONST_ENUM,
        symbol_flags::CONST_ENUM,
        "Priority should have CONST_ENUM flag"
    );

    // Should have exports
    assert!(
        priority_symbol.exports.is_some(),
        "Priority should have exports"
    );
    let exports = priority_symbol.exports.as_ref().unwrap();

    assert!(
        exports.get("Low").is_some(),
        "Low should be in Priority exports"
    );
    assert!(
        exports.get("Medium").is_some(),
        "Medium should be in Priority exports"
    );
    assert!(
        exports.get("High").is_some(),
        "High should be in Priority exports"
    );
}

#[test]
fn test_namespace_reopening_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
namespace Reopened {
    export const first = 1;
}
namespace Reopened {
    export const second = 2;
    export function combined() { return first + second; }
}

const a = Reopened.first;
const b = Reopened.second;
const c = Reopened.combined();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let reopened_sym_id = binder
        .file_locals
        .get("Reopened")
        .expect("Reopened should exist");
    let reopened_symbol = binder
        .get_symbol(reopened_sym_id)
        .expect("Reopened symbol should exist");

    // Should have all exports from both declarations
    let exports = reopened_symbol
        .exports
        .as_ref()
        .expect("Reopened should have exports");

    assert!(
        exports.get("first").is_some(),
        "first should be in Reopened exports"
    );
    assert!(
        exports.get("second").is_some(),
        "second should be in Reopened exports"
    );
    assert!(
        exports.get("combined").is_some(),
        "combined should be in Reopened exports"
    );
    assert_eq!(exports.len(), 3, "Reopened should have exactly 3 exports");
}

#[test]
fn test_enum_namespace_merging_with_exports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

    let source = r#"
enum ErrorCode {
    NotFound = 404,
    ServerError = 500
}
namespace ErrorCode {
    export function getMessage(code: ErrorCode): string {
        if (code === ErrorCode.NotFound) return "Not Found";
        if (code === ErrorCode.ServerError) return "Server Error";
        return "Unknown";
    }
}

const err1 = ErrorCode.NotFound;
const msg1 = ErrorCode.getMessage(ErrorCode.NotFound);
const err2 = ErrorCode.ServerError;
const msg2 = ErrorCode.getMessage(ErrorCode.ServerError);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let error_code_sym_id = binder
        .file_locals
        .get("ErrorCode")
        .expect("ErrorCode should exist");
    let error_code_symbol = binder
        .get_symbol(error_code_sym_id)
        .expect("ErrorCode symbol should exist");

    // Should have both enum members and namespace function
    let exports = error_code_symbol
        .exports
        .as_ref()
        .expect("ErrorCode should have exports");

    assert!(
        exports.get("NotFound").is_some(),
        "NotFound should be in ErrorCode exports"
    );
    assert!(
        exports.get("ServerError").is_some(),
        "ServerError should be in ErrorCode exports"
    );
    assert!(
        exports.get("getMessage").is_some(),
        "getMessage should be in ErrorCode exports"
    );
}

// =============================================================================
// Scope Chain Traversal Tests
// =============================================================================

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
                !scope.parent.is_none(),
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
        "Valid code should have no validation errors: {:?}",
        errors
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

/// Test that multiple export * from statements are properly tracked.
/// This ensures we can resolve exports from files that re-export from multiple sources.
#[test]
fn test_multiple_wildcard_reexports() {
    use crate::binder::BinderState;
    use crate::parser::ParserState;

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
        "Should have 3 wildcard re-exports, got: {:?}",
        reexports
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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

    // Setup: We'll create module_exports and wildcard_reexports manually
    // to test the resolution logic without parsing multiple files
    let source = "const x = 1;"; // Minimal source just to create a binder

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Setup module_exports for two modules
    let mut module_a_exports = SymbolTable::new();
    let sym_a = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcA".to_string());
    module_a_exports.set("funcA".to_string(), sym_a);
    binder
        .module_exports
        .insert("./moduleA".to_string(), module_a_exports);

    let mut module_b_exports = SymbolTable::new();
    let sym_b = binder
        .symbols
        .alloc(symbol_flags::FUNCTION, "funcB".to_string());
    module_b_exports.set("funcB".to_string(), sym_b);
    binder
        .module_exports
        .insert("./moduleB".to_string(), module_b_exports);

    // Setup index.ts to re-export from both
    binder.wildcard_reexports.insert(
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

/// Regression test: Two lib files with overlapping local SymbolIds must resolve
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
    let ids = vec![object_id, array_id, promise_id, map_id];
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
