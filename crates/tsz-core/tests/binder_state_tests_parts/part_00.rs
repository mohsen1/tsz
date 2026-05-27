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
            .get_identifier_at(func.name)
            .map(|ident| ident.escaped_text.as_str());
        if name != Some(function_name) {
            continue;
        }
        for &param_idx in &func.parameters.nodes {
            let Some(param) = arena.get_parameter_at(param_idx) else {
                continue;
            };
            let param_text = arena
                .get_identifier_at(param.name)
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
        param_name_idx.is_some(),
        "Expected to find parameter name for {function_name}"
    );
    assert!(
        param_symbol.is_some(),
        "Expected parameter symbol for {function_name}"
    );
    assert!(
        function_body.is_some(),
        "Expected function body for {function_name}"
    );

    let mut usage_idx = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if idx == param_name_idx {
            continue;
        }
        let Some(ident) = arena.get_identifier_at(idx) else {
            continue;
        };
        if ident.escaped_text != param_name {
            continue;
        }
        let mut current = idx;
        let mut in_body = false;
        while current.is_some() {
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
        usage_idx.is_some(),
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
    while current.is_some() {
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

fn find_function_param_and_usage(
    arena: &crate::parser::node::NodeArena,
    binder: &BinderState,
    function_name: &str,
    param_name: &str,
) -> (crate::parser::NodeIndex, Option<crate::binder::SymbolId>) {
    use crate::parser::{NodeIndex, syntax_kind_ext};

    let mut param_name_idx = NodeIndex::NONE;
    let mut function_body = NodeIndex::NONE;
    let mut param_symbol = None;

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
            .get_identifier_at(func.name)
            .map(|ident| ident.escaped_text.as_str());
        if name != Some(function_name) {
            continue;
        }

        function_body = func.body;
        for &param_idx in &func.parameters.nodes {
            let Some(param) = arena.get_parameter_at(param_idx) else {
                continue;
            };
            let param_text = arena
                .get_identifier_at(param.name)
                .map(|ident| ident.escaped_text.as_str());
            if param_text == Some(param_name) {
                param_name_idx = param.name;
                param_symbol = binder.get_node_symbol(param.name);
                break;
            }
        }
        break;
    }

    assert!(
        param_name_idx.is_some(),
        "Expected to find parameter '{param_name}' in function '{function_name}'"
    );
    assert!(
        function_body.is_some(),
        "Expected function body for '{function_name}'"
    );

    let mut usage_idx = NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if idx == param_name_idx {
            continue;
        }
        let Some(ident) = arena.get_identifier_at(idx) else {
            continue;
        };
        if ident.escaped_text != param_name {
            continue;
        }
        if node_is_within(arena, idx, function_body) {
            usage_idx = idx;
            break;
        }
    }

    assert!(
        usage_idx.is_some(),
        "Expected usage of parameter '{param_name}' in function body"
    );

    (usage_idx, param_symbol)
}

#[test]
fn test_resolve_identifier_caches_results_for_repeated_lookup() {
    let source = r#"
function f(x: number) {
    return x + x;
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    assert_eq!(binder.resolved_identifier_cache_len(), 0);

    let (usage_idx, expected_symbol) = find_function_param_and_usage(arena, &binder, "f", "x");

    let first = binder.resolve_identifier(arena, usage_idx);
    assert_eq!(first, expected_symbol);
    assert_eq!(binder.resolved_identifier_cache_len(), 1);

    let second = binder.resolve_identifier(arena, usage_idx);
    assert_eq!(second, expected_symbol);
    assert_eq!(binder.resolved_identifier_cache_len(), 1);
}

#[test]
fn test_resolve_identifier_cache_cleared_on_rebind() {
    let source = r#"
function f(x: number) {
    return x;
}
"#;
    let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let (usage_idx, _) = find_function_param_and_usage(arena, &binder, "f", "x");
    let _ = binder.resolve_identifier(arena, usage_idx);
    assert_eq!(binder.resolved_identifier_cache_len(), 1);

    let mut parser2 = ParserState::new("b.ts".to_string(), "const y = 1;".to_string());
    let root2 = parser2.parse_source_file();
    binder.bind_source_file(parser2.get_arena(), root2);

    assert_eq!(binder.resolved_identifier_cache_len(), 0);
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
            .get_identifier_at(func.name)
            .map(|ident| ident.escaped_text.as_str());
        if name == Some("getModuleInstanceStateForAliasTarget") {
            function_body = func.body;
            break;
        }
    }

    assert!(
        function_body.is_some(),
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
        decl_name_idx.is_some(),
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
        let Some(ident) = arena.get_identifier_at(for_data.expression) else {
            continue;
        };
        if ident.escaped_text == "statements" {
            usage_idx = for_data.expression;
            break;
        }
    }

    assert!(
        usage_idx.is_some(),
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

    let source = r#"
namespace foo {
    export class Provide {}
    class NotExported {}
    export function bar() {}
    function baz() {}
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
namespace NS {
    export class C {}
}
import Alias = NS.C;
var x: Alias;
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
class Merge {
    constructor() {}
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
class Merge {
    constructor() {}
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
function Merge() {}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
function Merge() {}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
enum Merge {
    A,
}
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
namespace Merge {
    export const extra = 1;
    const hidden = 2;
}
enum Merge {
    A,
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(&source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let source = r#"
namespace NS {
    export const exported = 1;
    const notExported = 2;
}

const a = NS.exported;
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
