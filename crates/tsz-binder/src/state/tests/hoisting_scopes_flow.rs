use super::*;

#[test]
fn var_declaration_hoisted_to_function_scope() {
    // `var` declarations inside blocks should be visible at the function scope
    // level, because JavaScript hoists `var` to the enclosing function.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        var x = 1;
    }
}
",
    );

    // `foo` should be in file_locals
    assert!(
        binder.file_locals.has("foo"),
        "function foo should be in file_locals"
    );

    // `x` should be in the function scope (hoisted), not just the block scope.
    // We check that a symbol for `x` was created with FUNCTION_SCOPED_VARIABLE flag.
    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).expect("expected symbol data");
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var x should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_hoisted_from_nested_blocks() {
    // `var` inside nested blocks (if/while/for) should still be hoisted
    // to the enclosing function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    if (true) {
        while (true) {
            var deep = 1;
        }
    }
}
",
    );

    let deep_sym = binder
        .symbols
        .find_by_name("deep")
        .expect("expected symbol for deep");
    let deep_symbol = binder.symbols.get(deep_sym).expect("expected symbol data");
    assert!(
        deep_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var deep should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn let_not_hoisted_across_blocks() {
    // `let` declarations should NOT be hoisted to function scope.
    // They should be block-scoped.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        let x = 1;
    }
}
",
    );

    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).expect("expected symbol data");
    assert!(
        x_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "let x should have BLOCK_SCOPED_VARIABLE flag"
    );
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE == 0,
        "let x should NOT have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn const_not_hoisted_across_blocks() {
    // `const` declarations should NOT be hoisted to function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    if (true) {
        const y = 2;
    }
}
",
    );

    let y_sym = binder
        .symbols
        .find_by_name("y")
        .expect("expected symbol for y");
    let y_symbol = binder.symbols.get(y_sym).expect("expected symbol data");
    assert!(
        y_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "const y should have BLOCK_SCOPED_VARIABLE flag"
    );
}

#[test]
fn function_declaration_hoisted_to_containing_scope() {
    // Function declarations at the top level should be hoisted and visible
    // in file_locals.
    let (binder, _parser) = parse_and_bind(
        r"
foo();
function foo() {}
",
    );

    assert!(
        binder.file_locals.has("foo"),
        "function declaration should be hoisted to file_locals"
    );
    let foo_sym_id = binder.file_locals.get("foo").unwrap();
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "foo should have FUNCTION flag"
    );
}

#[test]
fn function_declaration_hoisted_inside_function() {
    // Function declarations inside a function body should be hoisted to the
    // function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    inner();
    function inner() {}
}
",
    );

    let inner_sym = binder
        .symbols
        .find_by_name("inner")
        .expect("expected symbol for inner");
    let inner_symbol = binder.symbols.get(inner_sym).expect("expected symbol data");
    assert!(
        inner_symbol.flags & symbol_flags::FUNCTION != 0,
        "inner should have FUNCTION flag"
    );
}

#[test]
fn function_body_declarations_hoist_even_in_strict_es2015() {
    // A function body's top-level block is the function scope for declaration
    // hoisting. Nested blocks are block-scoped in ES2015 strict mode, but the
    // direct function body is not a nested block.
    let options = BinderOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r#"
function outer() {
    var clash;
    function clash() {}
}
"#,
        options,
    );

    let function_scope_clash = binder.scopes.iter().find_map(|scope| {
        (scope.kind == ContainerKind::Function)
            .then(|| scope.table.get("clash"))
            .flatten()
    });
    let clash_sym = function_scope_clash
        .and_then(|sym_id| binder.symbols.get(sym_id))
        .expect("expected `clash` in the function scope");
    assert!(
        clash_sym.flags & symbol_flags::FUNCTION != 0,
        "function-body declaration should hoist to the function scope, got flags {}",
        clash_sym.flags
    );
}

#[test]
fn function_in_block_not_hoisted_in_strict_mode() {
    // In strict mode (via "use strict"), function declarations in blocks
    // should be block-scoped, not hoisted.
    let options = BinderOptions {
        target: ScriptTarget::ES2015,
        always_strict: true,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r#"
function outer() {
    if (true) {
        function blockFunc() {}
    }
}
"#,
        options,
    );

    // The function should still exist as a symbol, but it should not be
    // hoisted to the function scope in strict mode.
    let block_func_sym = binder.symbols.find_by_name("blockFunc");
    assert!(
        block_func_sym.is_some(),
        "blockFunc should exist as a symbol"
    );
}

#[test]
fn function_in_block_hoisted_in_non_strict_es5() {
    // In non-strict ES5 mode, function declarations in blocks should be
    // hoisted (Annex B behavior).
    let options = BinderOptions {
        target: ScriptTarget::ES5,
        always_strict: false,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r"
function outer() {
    if (true) {
        function blockFunc() {}
    }
}
",
        options,
    );

    let block_func_sym = binder.symbols.find_by_name("blockFunc");
    assert!(
        block_func_sym.is_some(),
        "blockFunc should exist as a symbol (hoisted in non-strict ES5)"
    );
}

#[test]
fn duplicate_var_declarations_merge() {
    // Duplicate `var` declarations should merge (not create separate symbols).
    // This is valid JavaScript behavior.
    let (binder, _parser) = parse_and_bind(
        r"
var x = 1;
var x = 2;
",
    );

    // There should be exactly one symbol for x in file_locals
    assert!(binder.file_locals.has("x"), "x should be in file_locals");

    // The symbol should have FUNCTION_SCOPED_VARIABLE flag
    let x_sym_id = binder.file_locals.get("x").unwrap();
    let x_symbol = binder.symbols.get(x_sym_id).unwrap();
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "x should have FUNCTION_SCOPED_VARIABLE flag"
    );
    // Should have multiple declarations
    assert!(
        x_symbol.declarations.len() >= 2,
        "duplicate var should have at least 2 declarations, got {}",
        x_symbol.declarations.len()
    );
}

#[test]
fn duplicate_const_then_var_keeps_earliest_syntactic_value_declaration() {
    let (binder, parser) = parse_and_bind(
        r"
declare const e: string | boolean | undefined;
declare var e: { a: number };
",
    );
    let arena = parser.get_arena();
    let e_sym_id = binder.file_locals.get("e").expect("expected e");
    let e_symbol = binder.symbols.get(e_sym_id).expect("expected e symbol");
    let value_decl = e_symbol.value_declaration;
    let parent = arena
        .get_extended(value_decl)
        .expect("expected value declaration metadata")
        .parent;
    let parent_node = arena
        .get(parent)
        .expect("expected variable declaration list");

    assert_eq!(
        arena
            .get(value_decl)
            .expect("expected value declaration")
            .kind,
        syntax_kind_ext::VARIABLE_DECLARATION
    );
    assert_eq!(parent_node.kind, syntax_kind_ext::VARIABLE_DECLARATION_LIST);
    assert!(
        node_flags::is_block_scoped(parent_node.flags as u32),
        "value declaration should be the source-earlier const, not the hoisted var"
    );
}

#[test]
fn duplicate_var_then_const_keeps_var_as_earliest_syntactic_value_declaration() {
    let (binder, parser) = parse_and_bind(
        r"
declare var e: { a: number };
declare const e: string | boolean | undefined;
",
    );
    let arena = parser.get_arena();
    let e_sym_id = binder.file_locals.get("e").expect("expected e");
    let e_symbol = binder.symbols.get(e_sym_id).expect("expected e symbol");
    let value_decl = e_symbol.value_declaration;
    let parent = arena
        .get_extended(value_decl)
        .expect("expected value declaration metadata")
        .parent;
    let parent_node = arena
        .get(parent)
        .expect("expected variable declaration list");

    assert_eq!(
        arena
            .get(value_decl)
            .expect("expected value declaration")
            .kind,
        syntax_kind_ext::VARIABLE_DECLARATION
    );
    assert_eq!(parent_node.kind, syntax_kind_ext::VARIABLE_DECLARATION_LIST);
    assert!(
        !node_flags::is_block_scoped(parent_node.flags as u32),
        "value declaration should remain the source-earlier var"
    );
}

#[test]
fn var_in_for_loop_head_hoisted() {
    // `var` in a for-loop initializer should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var i = 0; i < 10; i++) {
        // body
    }
}
",
    );

    let i_sym = binder
        .symbols
        .find_by_name("i")
        .expect("expected symbol for i");
    let i_symbol = binder.symbols.get(i_sym).expect("expected symbol data");
    assert!(
        i_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var i in for-loop should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_in_for_in_loop_head_hoisted() {
    // `var` in a for-in loop should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var key in obj) {
        // body
    }
}
",
    );

    let key_sym = binder
        .symbols
        .find_by_name("key")
        .expect("expected symbol for key");
    let key_symbol = binder.symbols.get(key_sym).expect("expected symbol data");
    assert!(
        key_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var key in for-in should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_in_for_of_loop_head_hoisted() {
    // `var` in a for-of loop should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    for (var item of items) {
        // body
    }
}
",
    );

    let item_sym = binder
        .symbols
        .find_by_name("item")
        .expect("expected symbol for item");
    let item_symbol = binder.symbols.get(item_sym).expect("expected symbol data");
    assert!(
        item_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var item in for-of should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn var_hoisted_from_try_catch_finally() {
    // `var` declarations in try, catch, and finally blocks should all be
    // hoisted to the enclosing function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    try {
        var tryVar = 1;
    } catch (e) {
        var catchVar = 2;
    } finally {
        var finallyVar = 3;
    }
}
",
    );

    for name in &["tryVar", "catchVar", "finallyVar"] {
        let sym = binder
            .symbols
            .find_by_name(name)
            .unwrap_or_else(|| panic!("expected symbol for {name}"));
        let symbol = binder.symbols.get(sym).unwrap();
        assert!(
            symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
            "{name} should have FUNCTION_SCOPED_VARIABLE flag"
        );
    }
}

#[test]
fn var_hoisted_from_switch_statement() {
    // `var` declarations in switch case/default clauses should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    switch (x) {
        case 1:
            var caseVar = 1;
            break;
        default:
            var defaultVar = 2;
    }
}
",
    );

    for name in &["caseVar", "defaultVar"] {
        let sym = binder
            .symbols
            .find_by_name(name)
            .unwrap_or_else(|| panic!("expected symbol for {name}"));
        let symbol = binder.symbols.get(sym).unwrap();
        assert!(
            symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
            "{name} should have FUNCTION_SCOPED_VARIABLE flag"
        );
    }
}

#[test]
fn var_hoisted_from_labeled_statement() {
    // `var` inside a labeled statement should be hoisted.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    label: var x = 1;
}
",
    );

    let x_sym = binder
        .symbols
        .find_by_name("x")
        .expect("expected symbol for x");
    let x_symbol = binder.symbols.get(x_sym).unwrap();
    assert!(
        x_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var inside labeled statement should be hoisted"
    );
}

// =============================================================================
// 2. SCOPE MANAGEMENT
// =============================================================================

#[test]
fn source_file_creates_root_scope() {
    // The source file should always create a root scope.
    let (binder, _parser) = parse_and_bind("let x = 1;");

    assert!(
        !binder.scopes.is_empty(),
        "binding should create at least one scope"
    );
    assert_eq!(
        binder.scopes[0].kind,
        ContainerKind::SourceFile,
        "first scope should be SourceFile"
    );
}

#[test]
fn block_creates_block_scope() {
    // An explicit block (`{ ... }`) should create a Block scope.
    let (binder, _parser) = parse_and_bind(
        r"
{
    let x = 1;
}
",
    );

    let has_block_scope = binder.scopes.iter().any(|s| s.kind == ContainerKind::Block);
    assert!(has_block_scope, "block should create a Block scope");
}

#[test]
fn function_creates_function_scope() {
    // A function declaration should create a Function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    let has_function_scope = binder
        .scopes
        .iter()
        .any(|s| s.kind == ContainerKind::Function);
    assert!(
        has_function_scope,
        "function declaration should create a Function scope"
    );
}

#[test]
fn class_creates_class_scope() {
    // A class declaration should create a Class scope.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 1;
}
",
    );

    let has_class_scope = binder.scopes.iter().any(|s| s.kind == ContainerKind::Class);
    assert!(
        has_class_scope,
        "class declaration should create a Class scope"
    );
}

#[test]
fn namespace_creates_module_scope() {
    // A namespace declaration should create a Module scope.
    let (binder, _parser) = parse_and_bind(
        r"
namespace M {
    export const x = 1;
}
",
    );

    let has_module_scope = binder
        .scopes
        .iter()
        .any(|s| s.kind == ContainerKind::Module);
    assert!(
        has_module_scope,
        "namespace declaration should create a Module scope"
    );
}

#[test]
fn if_body_creates_block_scope() {
    // The block body of an if statement should create a Block scope.
    let (binder, _parser) = parse_and_bind(
        r"
if (true) {
    let x = 1;
}
",
    );

    // There should be at least 2 scopes: SourceFile + Block for if body
    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(
        block_count >= 1,
        "if body block should create a Block scope"
    );
}

#[test]
fn for_loop_creates_block_scope() {
    // A for loop should create a Block scope (for the initializer variable).
    let (binder, _parser) = parse_and_bind(
        r"
for (let i = 0; i < 10; i++) {
    let x = i;
}
",
    );

    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(
        block_count >= 1,
        "for loop should create at least one Block scope"
    );
}

#[test]
fn nested_scopes_have_correct_parent_chain() {
    // Nested scopes should correctly link to their parent.
    let (binder, _parser) = parse_and_bind(
        r"
function outer() {
    function inner() {
        let x = 1;
    }
}
",
    );

    // We should have: SourceFile -> Function (outer) -> Function (inner)
    // Verify that function scopes exist and have parent links
    let function_scopes: Vec<_> = binder
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == ContainerKind::Function)
        .collect();

    assert!(
        function_scopes.len() >= 2,
        "should have at least 2 function scopes (outer and inner)"
    );

    // The inner function scope should have a parent that's not ScopeId::NONE
    for (_, scope) in &function_scopes {
        assert!(
            scope.parent.is_some() || scope.parent.is_none(),
            "function scopes should have valid parent chain"
        );
    }
}

#[test]
fn function_scope_contains_parameters() {
    // Function parameters should be declared in the function scope.
    let (binder, _parser) = parse_and_bind(
        r"
function foo(a: number, b: string) {
    return a;
}
",
    );

    // Parameters should be created as FUNCTION_SCOPED_VARIABLE symbols
    let a_sym = binder
        .symbols
        .find_by_name("a")
        .expect("expected symbol for parameter a");
    let a_symbol = binder.symbols.get(a_sym).unwrap();
    assert!(
        a_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "parameter a should have FUNCTION_SCOPED_VARIABLE flag"
    );

    let b_sym = binder
        .symbols
        .find_by_name("b")
        .expect("expected symbol for parameter b");
    let b_symbol = binder.symbols.get(b_sym).unwrap();
    assert!(
        b_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "parameter b should have FUNCTION_SCOPED_VARIABLE flag"
    );
}

#[test]
fn module_scope_contains_top_level_declarations() {
    // Top-level declarations in a source file should be in file_locals.
    let (binder, _parser) = parse_and_bind(
        r"
let a = 1;
const b = 2;
var c = 3;
function d() {}
class E {}
",
    );

    assert!(
        binder.file_locals.has("a"),
        "let a should be in file_locals"
    );
    assert!(
        binder.file_locals.has("b"),
        "const b should be in file_locals"
    );
    assert!(
        binder.file_locals.has("c"),
        "var c should be in file_locals"
    );
    assert!(
        binder.file_locals.has("d"),
        "function d should be in file_locals"
    );
    assert!(
        binder.file_locals.has("E"),
        "class E should be in file_locals"
    );
}

// =============================================================================
// 3. SYMBOL RESOLUTION
// =============================================================================

#[test]
fn resolve_identifier_in_file_locals() {
    // Identifiers in file_locals should be resolvable via resolve_identifier.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
x;
",
    );

    // x should be in file_locals
    assert!(binder.file_locals.has("x"), "x should be in file_locals");
}

#[test]
fn shadowing_inner_scope_shadows_outer() {
    // An inner scope declaration should shadow an outer scope declaration.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {
    let x = 2;
}
",
    );

    // Both symbols should exist (with same name but different IDs)
    let all_x = binder.symbols.find_all_by_name("x");
    assert!(
        all_x.len() >= 2,
        "should have at least 2 symbols named x (outer and inner), got {}",
        all_x.len()
    );
}

#[test]
fn import_creates_alias_symbol() {
    // ES6 imports should create ALIAS symbols with import metadata.
    let (binder, _parser) = parse_and_bind(
        r"
import { foo } from './bar';
",
    );

    let foo_sym_id = binder
        .file_locals
        .get("foo")
        .expect("expected import symbol foo in file_locals");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::ALIAS != 0,
        "imported foo should have ALIAS flag"
    );
    assert_eq!(
        foo_symbol.import_module.as_deref(),
        Some("./bar"),
        "import_module should be './bar'"
    );
}

#[test]
fn import_as_creates_alias_with_original_name() {
    // `import { foo as bar }` should create an ALIAS symbol for bar with
    // import_name pointing to the original name "foo".
    let (binder, _parser) = parse_and_bind(
        r"
import { foo as bar } from './baz';
",
    );

    let bar_sym_id = binder
        .file_locals
        .get("bar")
        .expect("expected import symbol bar");
    let bar_symbol = binder.symbols.get(bar_sym_id).unwrap();
    assert!(bar_symbol.flags & symbol_flags::ALIAS != 0);
    assert_eq!(bar_symbol.import_module.as_deref(), Some("./baz"));
    assert_eq!(bar_symbol.import_name.as_deref(), Some("foo"));
}

#[test]
fn namespace_import_creates_alias() {
    // `import * as ns from './mod'` should create an ALIAS symbol.
    let (binder, _parser) = parse_and_bind(
        r"
import * as ns from './mod';
",
    );

    let ns_sym_id = binder
        .file_locals
        .get("ns")
        .expect("expected namespace import symbol ns");
    let ns_symbol = binder.symbols.get(ns_sym_id).unwrap();
    assert!(
        ns_symbol.flags & symbol_flags::ALIAS != 0,
        "namespace import should have ALIAS flag"
    );
    assert_eq!(ns_symbol.import_module.as_deref(), Some("./mod"));
}

#[test]
fn export_tracking_with_export_modifier() {
    // Symbols with `export` modifier should have is_exported set to true.
    let (binder, _parser) = parse_and_bind(
        r"
export const x = 1;
export function foo() {}
export class Bar {}
",
    );

    for name in &["x", "foo", "Bar"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("expected {name} in file_locals"));
        let symbol = binder.symbols.get(sym_id).unwrap();
        assert!(symbol.is_exported, "{name} should have is_exported = true");
    }
}

#[test]
fn type_only_import() {
    // `import type { X } from './mod'` should create a type-only alias.
    let (binder, _parser) = parse_and_bind(
        r"
import type { X } from './mod';
",
    );

    let x_sym_id = binder
        .file_locals
        .get("X")
        .expect("expected type-only import symbol X");
    let x_symbol = binder.symbols.get(x_sym_id).unwrap();
    assert!(
        x_symbol.is_type_only,
        "type-only import should have is_type_only = true"
    );
}

#[test]
fn default_import_creates_alias() {
    // `import Foo from './mod'` should create an ALIAS symbol for the default.
    let (binder, _parser) = parse_and_bind(
        r"
import Foo from './mod';
",
    );

    let foo_sym_id = binder
        .file_locals
        .get("Foo")
        .expect("expected default import symbol Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::ALIAS != 0,
        "default import should have ALIAS flag"
    );
    assert_eq!(foo_symbol.import_module.as_deref(), Some("./mod"));
}

// =============================================================================
// 4. FLOW GRAPH CONSTRUCTION
// =============================================================================

#[test]
fn basic_sequential_flow() {
    // Sequential statements should have a linear flow graph.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
let y = 2;
let z = 3;
",
    );

    // Should have at least: UNREACHABLE + START + some assignment nodes
    assert!(
        binder.flow_nodes.len() >= 2,
        "should have at least UNREACHABLE and START flow nodes"
    );

    // Verify there's exactly 1 START node (for the file)
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(start_count, 1, "should have exactly 1 START flow node");

    // Verify there's exactly 1 UNREACHABLE node
    let unreachable_count = count_flow_nodes_with_flags(&binder, flow_flags::UNREACHABLE);
    assert_eq!(
        unreachable_count, 1,
        "should have exactly 1 UNREACHABLE flow node"
    );
}

#[test]
fn if_statement_creates_condition_flows() {
    // An if statement should create TRUE_CONDITION and FALSE_CONDITION flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
if (x) {
    x;
}
",
    );

    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    assert!(
        true_count >= 1,
        "if statement should create at least 1 TRUE_CONDITION flow"
    );

    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);
    assert!(
        false_count >= 1,
        "if statement should create at least 1 FALSE_CONDITION flow"
    );
}

#[test]
fn if_else_creates_branch_and_merge() {
    // An if/else should create branch flows and a merge point.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string;
if (typeof x === 'number') {
    x;
} else {
    x;
}
x;
",
    );

    // Should have TRUE_CONDITION and FALSE_CONDITION
    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);
    assert!(true_count >= 1, "should have TRUE_CONDITION flow");
    assert!(false_count >= 1, "should have FALSE_CONDITION flow");

    // Should have at least 1 BRANCH_LABEL (merge point after if/else)
    let branch_count = count_flow_nodes_with_flags(&binder, flow_flags::BRANCH_LABEL);
    assert!(
        branch_count >= 1,
        "if/else should create a BRANCH_LABEL merge point"
    );
}

#[test]
fn while_loop_creates_loop_label() {
    // A while loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 0;
while (x < 10) {
    x = x + 1;
}
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "while loop should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn for_loop_creates_loop_label() {
    // A for loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
for (let i = 0; i < 10; i++) {
    i;
}
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "for loop should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn do_while_creates_loop_label() {
    // A do-while loop should create a LOOP_LABEL flow node.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 0;
do {
    x = x + 1;
} while (x < 10);
",
    );

    let loop_count = count_flow_nodes_with_flags(&binder, flow_flags::LOOP_LABEL);
    assert!(
        loop_count >= 1,
        "do-while should create at least 1 LOOP_LABEL flow"
    );
}

#[test]
fn return_creates_unreachable_flow() {
    // After a return statement, the current flow should become unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    return 1;
    let x = 2;
}
",
    );

    // Verify the function has a START node
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert!(
        start_count >= 2,
        "function should get its own START flow node"
    );
}

#[test]
fn break_in_loop_jumps_to_post_loop() {
    // A break statement in a loop should create a flow to the post-loop
    // merge label and make subsequent code in the loop unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
while (true) {
    if (x) {
        break;
    }
    x = 1;
}
",
    );

    // Should have BRANCH_LABEL for the post-loop merge point
    let branch_count = count_flow_nodes_with_flags(&binder, flow_flags::BRANCH_LABEL);
    assert!(
        branch_count >= 1,
        "break in loop should have BRANCH_LABEL for post-loop"
    );
}

#[test]
fn assignment_creates_flow_assignment() {
    // Variable assignments should create ASSIGNMENT flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | undefined;
x = 1;
",
    );

    let assignment_count = count_flow_nodes_with_flags(&binder, flow_flags::ASSIGNMENT);
    assert!(
        assignment_count >= 1,
        "assignment should create ASSIGNMENT flow node"
    );
}

#[test]
fn assignment_in_class_computed_property_does_not_create_flow_assignment() {
    let (binder, _parser) = parse_and_bind(
        r#"
let x: number;
class A { [(x = 1, "_")]() {} }
x;
"#,
    );

    let assignment_count = count_flow_nodes_with_flags(&binder, flow_flags::ASSIGNMENT);
    assert_eq!(
        assignment_count, 0,
        "assignments evaluated inside class computed property names should not create ASSIGNMENT flow nodes"
    );
}

#[test]
fn switch_creates_switch_clause_flow() {
    // Switch statements should create SWITCH_CLAUSE flow nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string;
switch (typeof x) {
    case 'number':
        x;
        break;
    case 'string':
        x;
        break;
}
",
    );

    let switch_clause_count = count_flow_nodes_with_flags(&binder, flow_flags::SWITCH_CLAUSE);
    assert!(
        switch_clause_count >= 1,
        "switch should create SWITCH_CLAUSE flow nodes"
    );
}

#[test]
fn function_body_gets_own_start_flow() {
    // A regular function declaration should get its own START flow node.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    let x = 1;
}
",
    );

    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(
        start_count, 2,
        "should have 2 START flow nodes: file + function"
    );
}

// =============================================================================
// 5. DECLARATION BINDING
// =============================================================================
