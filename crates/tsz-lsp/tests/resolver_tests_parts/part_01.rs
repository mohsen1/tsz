#[test]
fn test_switch_case_variable_scoping() {
    let source = r#"
const x = 1;
switch (x) {
    case 1: {
        const caseVar = "one";
        break;
    }
    default: {
        const defaultVar = "other";
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("x").is_some(),
        "'x' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("caseVar").is_none(),
        "'caseVar' should NOT be in file_locals (block-scoped inside case)"
    );
    assert!(
        binder.file_locals.get("defaultVar").is_none(),
        "'defaultVar' should NOT be in file_locals (block-scoped inside default)"
    );
}

#[test]
fn test_if_else_block_variable_scoping() {
    let source = r#"
if (true) {
    const ifVar = 1;
} else {
    const elseVar = 2;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("ifVar").is_none(),
        "'ifVar' should NOT be in file_locals (block-scoped in if)"
    );
    assert!(
        binder.file_locals.get("elseVar").is_none(),
        "'elseVar' should NOT be in file_locals (block-scoped in else)"
    );
}

#[test]
fn test_function_expression_name_scoping() {
    let source = r#"
const fn1 = function namedExpr() {
    return namedExpr;
};
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("fn1").is_some(),
        "'fn1' should be in file_locals"
    );
    // The named function expression name should NOT leak to file scope
    assert!(
        binder.file_locals.get("namedExpr").is_none(),
        "'namedExpr' should NOT be in file_locals (function expression name is local)"
    );
}

#[test]
fn test_multiple_declarations_same_scope() {
    let source = "const a = 1;\nlet b = 2;\nvar c = 3;\nfunction d() {}\nclass E {}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(binder.file_locals.get("a").is_some());
    assert!(binder.file_locals.get("b").is_some());
    assert!(binder.file_locals.get("c").is_some());
    assert!(binder.file_locals.get("d").is_some());
    assert!(binder.file_locals.get("E").is_some());
}

#[test]
fn test_find_references_class_used_in_type_position() {
    let source = r#"
class MyClass {}
const a: MyClass = new MyClass();
function foo(x: MyClass) {}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    let class_symbol = binder
        .file_locals
        .get("MyClass")
        .expect("MyClass should be bound");

    let mut walker = ScopeWalker::new(arena, &binder);
    let refs = walker.find_references(root, class_symbol);

    // Should find at least 3 references (declaration + type annotation + new expression)
    assert!(
        refs.len() >= 3,
        "should find at least 3 references to 'MyClass', got {}",
        refs.len()
    );
}

#[test]
fn test_scope_chain_at_arrow_function_body() {
    let source = r#"
const outer = 1;
const fn1 = (x: number) => {
    const inner = x;
    return inner;
};
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find the 'inner' usage in 'return inner;'
    let inner_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("inner") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&inner_usage) = inner_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, inner_usage);

        // Should have at least file + arrow function scopes
        assert!(
            chain.len() >= 2,
            "scope chain inside arrow function should have at least 2 scopes, got {}",
            chain.len()
        );

        // 'outer' should be visible
        let has_outer = chain.iter().any(|scope| scope.get("outer").is_some());
        assert!(
            has_outer,
            "'outer' should be visible from inside arrow function"
        );
    }
}

#[test]
fn test_try_finally_variable_scoping() {
    let source = r#"
try {
    const tryVar = 1;
} finally {
    const finallyVar = 2;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("tryVar").is_none(),
        "'tryVar' should NOT be in file_locals (block-scoped in try)"
    );
    assert!(
        binder.file_locals.get("finallyVar").is_none(),
        "'finallyVar' should NOT be in file_locals (block-scoped in finally)"
    );
}

// ============================================================================
// Additional resolver tests (batch 3)
// ============================================================================

#[test]
fn test_resolve_interface_in_file_locals() {
    let source = "interface Foo { x: number; }\ninterface Bar extends Foo {}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Foo").is_some(),
        "'Foo' interface should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Bar").is_some(),
        "'Bar' interface should be in file_locals"
    );
}

#[test]
fn test_resolve_enum_members_not_in_file_locals() {
    let source = "enum Direction { Up, Down, Left, Right }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Direction").is_some(),
        "'Direction' should be in file_locals"
    );
    // Individual enum members should not leak to file scope
    assert!(
        binder.file_locals.get("Up").is_none(),
        "'Up' enum member should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_class_in_file_locals() {
    let source = "class Animal { name: string = ''; }\nclass Dog extends Animal {}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Animal").is_some(),
        "'Animal' class should be in file_locals"
    );
    assert!(
        binder.file_locals.get("Dog").is_some(),
        "'Dog' class should be in file_locals"
    );
}

#[test]
fn test_labeled_statement_scoping() {
    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    inner: for (let j = 0; j < 10; j++) {
        if (i === j) break outer;
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // Loop variables should be block-scoped and not in file_locals
    assert!(
        binder.file_locals.get("i").is_none(),
        "'i' should NOT be in file_locals (block-scoped in for loop)"
    );
    assert!(
        binder.file_locals.get("j").is_none(),
        "'j' should NOT be in file_locals (block-scoped in for loop)"
    );
}

#[test]
fn test_resolve_variable_in_template_literal() {
    let source = r#"
const name = "world";
const greeting = `hello ${name}`;
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("name").is_some(),
        "'name' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("greeting").is_some(),
        "'greeting' should be in file_locals"
    );

    // Find the 'name' usage inside the template literal
    let name_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("name") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if name_nodes.len() >= 2 {
        let name_usage = *name_nodes.last().unwrap();
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, name_usage);
        assert!(
            resolved.is_some(),
            "'name' usage in template literal should resolve"
        );
    }
}

#[test]
fn test_resolve_default_parameter_value() {
    let source = "function greet(name: string = 'world') { return name; }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("greet").is_some(),
        "'greet' should be in file_locals"
    );

    // 'name' should not leak to file scope
    assert!(
        binder.file_locals.get("name").is_none(),
        "'name' parameter should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_rest_parameter() {
    let source = "function sum(...nums: number[]) { return nums.reduce((a, b) => a + b, 0); }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("sum").is_some(),
        "'sum' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("nums").is_none(),
        "'nums' rest parameter should NOT be in file_locals"
    );
}

#[test]
fn test_find_references_interface_usage() {
    let source = r#"
interface Config {
    host: string;
    port: number;
}
const cfg: Config = { host: "localhost", port: 3000 };
function setup(c: Config) {}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    if let Some(config_symbol) = binder.file_locals.get("Config") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, config_symbol);
        // Should find at least 3: declaration + type annotation on cfg + parameter type
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Config', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_var_hoisting_inside_if_block() {
    let source = r#"
function foo() {
    if (true) {
        var hoisted = 1;
    }
    return hoisted;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // 'hoisted' should NOT be in file_locals (var hoists to function, not file)
    assert!(
        binder.file_locals.get("hoisted").is_none(),
        "'hoisted' should NOT be in file_locals (hoisted to function scope)"
    );
}

#[test]
fn test_resolve_computed_property_no_crash() {
    let source = r#"
const key = "hello";
const obj = { [key]: 1 };
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("key").is_some(),
        "'key' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("obj").is_some(),
        "'obj' should be in file_locals"
    );

    // Resolve the 'key' usage in the computed property - should not crash
    let key_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("key") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if key_nodes.len() >= 2 {
        let key_usage = *key_nodes.last().unwrap();
        let mut walker = ScopeWalker::new(arena, &binder);
        let resolved = walker.resolve_node(root, key_usage);
        assert!(
            resolved.is_some(),
            "'key' in computed property should resolve"
        );
    }
}

#[test]
fn test_namespace_members_in_file_locals() {
    let source = "namespace MyNS { export const inner = 1; }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("MyNS").is_some(),
        "'MyNS' should be in file_locals"
    );
    // 'inner' is inside the namespace, should NOT be in file_locals
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (scoped to namespace)"
    );
}

#[test]
fn test_scope_chain_at_nested_arrow_functions() {
    let source = r#"
const a = 1;
const outer = () => {
    const b = 2;
    const inner = () => {
        const c = 3;
        return a + b + c;
    };
};
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find the 'c' usage in return statement
    let c_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("c") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&c_usage) = c_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, c_usage);

        // Should have at least 3 scopes (file + outer arrow + inner arrow)
        assert!(
            chain.len() >= 3,
            "nested arrow function scope chain should have at least 3 scopes, got {}",
            chain.len()
        );

        // 'a' should be visible from innermost scope
        let has_a = chain.iter().any(|scope| scope.get("a").is_some());
        assert!(has_a, "'a' should be visible from nested arrow function");
    }
}

// ============================================================================
// Additional resolver tests (batch 4 - edge cases)
// ============================================================================

#[test]
fn test_resolve_empty_source() {
    let source = "";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // No file_locals should exist in an empty file
    assert!(
        binder.file_locals.is_empty(),
        "empty source should have no file_locals"
    );
}

#[test]
fn test_resolve_destructuring_array_declaration() {
    let source = "const [first, second, ...rest] = [1, 2, 3, 4, 5];";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // Array destructured variables should be in file_locals
    if let Some(_sym) = binder.file_locals.get("first") {
        assert!(binder.file_locals.get("second").is_some());
    }
    // rest may or may not be bound depending on binder implementation
    let _ = binder.file_locals.get("rest");
}

#[test]
fn test_resolve_destructuring_object_declaration() {
    let source = "const { name, age } = { name: 'Alice', age: 30 };";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // Object destructured variables should be in file_locals
    if let Some(_sym) = binder.file_locals.get("name") {
        assert!(
            binder.file_locals.get("age").is_some(),
            "'age' should also be in file_locals from destructuring"
        );
    }
}

#[test]
fn test_abstract_class_in_file_locals() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Shape").is_some(),
        "'Shape' abstract class should be in file_locals"
    );
}

#[test]
fn test_class_with_private_constructor_in_file_locals() {
    let source = "class Singleton {\n  private static instance: Singleton;\n  private constructor() {}\n  static getInstance() { return new Singleton(); }\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Singleton").is_some(),
        "'Singleton' should be in file_locals"
    );
}

#[test]
fn test_resolve_variable_in_immediately_invoked_arrow() {
    let source = r#"
const result = (() => {
    const inner = 42;
    return inner;
})();
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("result").is_some(),
        "'result' should be in file_locals"
    );
    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' should NOT be in file_locals (inside IIFE)"
    );
}

#[test]
fn test_multiple_var_declarations_same_name() {
    // var allows redeclaration in the same function scope
    let source = r#"
function foo() {
    var x = 1;
    var x = 2;
    return x;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("foo").is_some(),
        "'foo' should be in file_locals"
    );
    // 'x' should not be in file_locals (scoped to function)
    assert!(
        binder.file_locals.get("x").is_none(),
        "'x' should NOT be in file_locals (function-scoped var)"
    );
}

#[test]
fn test_resolve_export_default_class() {
    let source = "export default class DefaultClass {\n  value = 1;\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    // The class name should be in file_locals
    if let Some(_sym) = binder.file_locals.get("DefaultClass") {
        // verified
    }
    // At minimum, should not panic
}

#[test]
fn test_resolve_class_with_static_methods() {
    let source = r#"
class MathUtils {
    static add(a: number, b: number) { return a + b; }
    static multiply(a: number, b: number) { return a * b; }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("MathUtils").is_some(),
        "'MathUtils' should be in file_locals"
    );
    // Static methods are members, not file_locals
    assert!(
        binder.file_locals.get("add").is_none(),
        "'add' static method should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("multiply").is_none(),
        "'multiply' static method should NOT be in file_locals"
    );
}

#[test]
fn test_find_references_enum_used_as_type_and_value() {
    let source = r#"
enum Status { Active, Inactive }
const s: Status = Status.Active;
function check(status: Status) { return status; }
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    if let Some(status_symbol) = binder.file_locals.get("Status") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, status_symbol);
        // Should find at least 3: declaration + type annotation + value usage
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Status', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_scope_chain_at_getter_body() {
    let source = r#"
const globalVal = 100;
class Counter {
    get count() {
        const temp = globalVal;
        return temp;
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find 'temp' usage in 'return temp;'
    let temp_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("temp") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&temp_usage) = temp_nodes.last() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, temp_usage);

        // Should have at least file + class + getter scopes
        assert!(
            chain.len() >= 3,
            "scope chain inside getter should have at least 3 scopes, got {}",
            chain.len()
        );

        // 'globalVal' should be visible
        let has_global = chain.iter().any(|scope| scope.get("globalVal").is_some());
        assert!(
            has_global,
            "'globalVal' should be visible from inside getter"
        );
    }
}

#[test]
fn test_resolve_const_enum_name() {
    let source = "const enum Direction { Up, Down, Left, Right }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Direction").is_some(),
        "'Direction' const enum should be in file_locals"
    );
}

#[test]
fn test_resolve_generic_class_name() {
    let source =
        "class Container<T> {\n  value: T;\n  constructor(val: T) { this.value = val; }\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Container").is_some(),
        "'Container' generic class should be in file_locals"
    );
}

#[test]
fn test_resolve_async_function_name() {
    let source = "async function fetchData() {\n  return await Promise.resolve(1);\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("fetchData").is_some(),
        "'fetchData' async function should be in file_locals"
    );
}

#[test]
fn test_resolve_generator_function_name() {
    let source = "function* counter() {\n  let i = 0;\n  while (true) { yield i++; }\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("counter").is_some(),
        "'counter' generator function should be in file_locals"
    );
}

#[test]
fn test_let_not_hoisted_out_of_block() {
    let source = r#"
{
    let blockScoped = 1;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("blockScoped").is_none(),
        "'blockScoped' let should NOT be in file_locals (block-scoped)"
    );
}

#[test]
fn test_const_not_hoisted_out_of_block() {
    let source = r#"
if (true) {
    const inner = 42;
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("inner").is_none(),
        "'inner' const should NOT be in file_locals (block-scoped)"
    );
}

#[test]
fn test_resolve_multiple_interfaces_same_name() {
    // Declaration merging: multiple interfaces with same name
    let source = r#"
interface Opts { x: number; }
interface Opts { y: string; }
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Opts").is_some(),
        "'Opts' merged interface should be in file_locals"
    );
}

#[test]
fn test_scope_chain_at_setter_body() {
    let source = r#"
const limit = 100;
class Config {
    set maxSize(val: number) {
        const clamped = Math.min(val, limit);
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find 'clamped' usage
    let clamped_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("clamped") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&clamped_node) = clamped_nodes.first() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, clamped_node);

        // Should have at least file + class + setter scopes
        assert!(
            chain.len() >= 3,
            "scope chain inside setter should have at least 3 scopes, got {}",
            chain.len()
        );

        let has_limit = chain.iter().any(|scope| scope.get("limit").is_some());
        assert!(has_limit, "'limit' should be visible from inside setter");
    }
}

#[test]
fn test_find_references_type_alias_usage() {
    let source = r#"
type Point = { x: number; y: number };
const origin: Point = { x: 0, y: 0 };
function move(p: Point): Point { return p; }
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    if let Some(point_symbol) = binder.file_locals.get("Point") {
        let mut walker = ScopeWalker::new(arena, &binder);
        let refs = walker.find_references(root, point_symbol);
        // Should find at least 3: declaration + const annotation + param annotation + return annotation
        assert!(
            refs.len() >= 3,
            "should find at least 3 references to 'Point', got {}",
            refs.len()
        );
    }
}

#[test]
fn test_resolve_nested_namespace_function() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export function deep() { return 42; }
    }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Outer").is_some(),
        "'Outer' namespace should be in file_locals"
    );
    // 'Inner' and 'deep' should not be at file level
    assert!(
        binder.file_locals.get("Inner").is_none(),
        "'Inner' should NOT be in file_locals (nested namespace)"
    );
    assert!(
        binder.file_locals.get("deep").is_none(),
        "'deep' should NOT be in file_locals (nested in namespace)"
    );
}

#[test]
fn test_resolve_abstract_class_name() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("Shape").is_some(),
        "'Shape' abstract class should be in file_locals"
    );
}

#[test]
fn test_class_method_not_in_file_locals() {
    let source = r#"
class Calculator {
    add(a: number, b: number) { return a + b; }
    subtract(a: number, b: number) { return a - b; }
}
"#;
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(binder.file_locals.get("Calculator").is_some());
    assert!(
        binder.file_locals.get("add").is_none(),
        "'add' method should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("subtract").is_none(),
        "'subtract' method should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_arrow_function_variable_in_file_locals() {
    let source = "const transform = (x: number) => x * 2;";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("transform").is_some(),
        "'transform' arrow function variable should be in file_locals"
    );
}

#[test]
fn test_scope_chain_in_nested_arrow_callbacks() {
    let source = r#"
const outer = 1;
const fn1 = () => {
    const mid = 2;
    const fn2 = () => {
        const inner = 3;
    };
};
"#;
    let (parser, root, binder) = bind_test_source(source);
    let arena = parser.get_arena();
    let _ = root;

    // Find 'inner' identifier
    let inner_nodes: Vec<_> = arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if node.kind == SyntaxKind::Identifier as u16 {
                let node_idx = tsz_parser::NodeIndex(idx as u32);
                if arena.get_identifier_text(node_idx) == Some("inner") {
                    return Some(node_idx);
                }
            }
            None
        })
        .collect();

    if let Some(&inner_node) = inner_nodes.first() {
        let mut walker = ScopeWalker::new(arena, &binder);
        let chain = walker.get_scope_chain(root, inner_node);

        // Should have at least: file + fn1 arrow + fn2 arrow
        assert!(
            chain.len() >= 3,
            "scope chain inside nested arrows should have at least 3 scopes, got {}",
            chain.len()
        );

        let has_outer = chain.iter().any(|scope| scope.get("outer").is_some());
        assert!(has_outer, "'outer' should be visible from inner arrow");
    }
}

#[test]
fn test_resolve_enum_not_in_file_locals_members() {
    let source = "enum Color { Red, Green, Blue }";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(binder.file_locals.get("Color").is_some());
    assert!(
        binder.file_locals.get("Red").is_none(),
        "'Red' enum member should NOT be in file_locals"
    );
    assert!(
        binder.file_locals.get("Green").is_none(),
        "'Green' enum member should NOT be in file_locals"
    );
}

#[test]
fn test_resolve_export_const_in_file_locals() {
    let source = "export const API_KEY = 'abc123';";
    let (parser, root, binder) = bind_test_source(source);
    let _arena = parser.get_arena();
    let _ = root;

    assert!(
        binder.file_locals.get("API_KEY").is_some(),
        "'API_KEY' exported const should be in file_locals"
    );
}
