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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

