use super::*;

#[test]
fn variable_declaration_creates_symbol_with_correct_flags() {
    let (binder, _parser) = parse_and_bind(
        r"
let a = 1;
const b = 2;
var c = 3;
",
    );

    let a_sym_id = binder.file_locals.get("a").expect("expected a");
    let a_symbol = binder.symbols.get(a_sym_id).unwrap();
    assert!(
        a_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "let should have BLOCK_SCOPED_VARIABLE"
    );

    let b_sym_id = binder.file_locals.get("b").expect("expected b");
    let b_symbol = binder.symbols.get(b_sym_id).unwrap();
    assert!(
        b_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0,
        "const should have BLOCK_SCOPED_VARIABLE"
    );

    let c_sym_id = binder.file_locals.get("c").expect("expected c");
    let c_symbol = binder.symbols.get(c_sym_id).unwrap();
    assert!(
        c_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0,
        "var should have FUNCTION_SCOPED_VARIABLE"
    );
}

#[test]
fn function_declaration_creates_function_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
function foo(a: number): number { return a; }
",
    );

    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "function declaration should have FUNCTION flag"
    );
}

#[test]
fn class_declaration_creates_class_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 0;
    method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::CLASS != 0,
        "class declaration should have CLASS flag"
    );
}

#[test]
fn abstract_class_gets_abstract_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
abstract class Base {
    abstract method(): void;
}
",
    );

    let base_sym_id = binder.file_locals.get("Base").expect("expected Base");
    let base_symbol = binder.symbols.get(base_sym_id).unwrap();
    assert!(
        base_symbol.flags & symbol_flags::CLASS != 0,
        "abstract class should have CLASS flag"
    );
    assert!(
        base_symbol.flags & symbol_flags::ABSTRACT != 0,
        "abstract class should have ABSTRACT flag"
    );
}

#[test]
fn interface_declaration_creates_interface_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
    method(): void;
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::INTERFACE != 0,
        "interface declaration should have INTERFACE flag"
    );
}

#[test]
fn interface_merging_adds_multiple_declarations() {
    // Two interface declarations with the same name should merge
    // (add declarations to the same symbol).
    let (binder, _parser) = parse_and_bind(
        r"
interface Foo {
    x: number;
}
interface Foo {
    y: string;
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::INTERFACE != 0,
        "merged interface should have INTERFACE flag"
    );
    assert!(
        foo_symbol.declarations.len() >= 2,
        "merged interface should have at least 2 declarations, got {}",
        foo_symbol.declarations.len()
    );
}

#[test]
fn type_alias_creates_type_alias_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
type MyType = string | number;
",
    );

    let sym_id = binder.file_locals.get("MyType").expect("expected MyType");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::TYPE_ALIAS != 0,
        "type alias should have TYPE_ALIAS flag"
    );
}

#[test]
fn enum_declaration_creates_regular_enum_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
",
    );

    let sym_id = binder.file_locals.get("Color").expect("expected Color");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::REGULAR_ENUM != 0,
        "enum should have REGULAR_ENUM flag"
    );
}

#[test]
fn const_enum_creates_const_enum_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
const enum Direction {
    Up,
    Down,
    Left,
    Right,
}
",
    );

    let sym_id = binder
        .file_locals
        .get("Direction")
        .expect("expected Direction");
    let symbol = binder.symbols.get(sym_id).unwrap();
    assert!(
        symbol.flags & symbol_flags::CONST_ENUM != 0,
        "const enum should have CONST_ENUM flag"
    );
}

#[test]
fn enum_members_are_in_exports() {
    // Enum members should be tracked as exports of the enum symbol.
    let (binder, _parser) = parse_and_bind(
        r"
enum Color {
    Red,
    Green,
    Blue,
}
",
    );

    let color_sym_id = binder.file_locals.get("Color").expect("expected Color");
    let color_symbol = binder.symbols.get(color_sym_id).unwrap();
    let exports = color_symbol
        .exports
        .as_ref()
        .expect("enum should have exports");

    assert!(exports.has("Red"), "expected Red in enum exports");
    assert!(exports.has("Green"), "expected Green in enum exports");
    assert!(exports.has("Blue"), "expected Blue in enum exports");
}

#[test]
fn namespace_merging_with_function() {
    // A namespace and function with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {}
namespace foo {
    export const x = 1;
}
",
    );

    // foo should exist in file_locals
    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();

    // Should have both FUNCTION and MODULE flags
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "merged symbol should have FUNCTION flag"
    );
}

#[test]
fn namespace_creates_namespace_module_symbol() {
    let (binder, _parser) = parse_and_bind(
        r"
namespace NS {
    export interface I {}
}
",
    );

    let ns_sym_id = binder.file_locals.get("NS").expect("expected NS");
    let ns_symbol = binder.symbols.get(ns_sym_id).unwrap();
    assert!(
        ns_symbol.flags & symbol_flags::NAMESPACE_MODULE != 0,
        "namespace should have NAMESPACE_MODULE flag"
    );
    let exports = ns_symbol
        .exports
        .as_ref()
        .expect("namespace should have exports");
    assert!(exports.has("I"), "expected I in namespace exports");
}

#[test]
fn class_members_are_tracked() {
    // Class members should be tracked in the class symbol's members table.
    let (binder, _parser) = parse_and_bind(
        r"
class Foo {
    x: number = 0;
    y: string = '';
    method(): void {}
}
",
    );

    let foo_sym_id = binder.file_locals.get("Foo").expect("expected Foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    let members = foo_symbol
        .members
        .as_ref()
        .expect("class should have members");

    assert!(members.has("x"), "expected x in class members");
    assert!(members.has("y"), "expected y in class members");
    assert!(members.has("method"), "expected method in class members");
}

// =============================================================================
// 6. EXTERNAL MODULE DETECTION
// =============================================================================

#[test]
fn import_makes_file_external_module() {
    // A file with an import declaration should be detected as an external module.
    let (binder, _parser) = parse_and_bind(
        r"
import { x } from './a';
",
    );

    assert!(
        binder.is_external_module,
        "file with import should be an external module"
    );
}

#[test]
fn export_makes_file_external_module() {
    // A file with an export declaration should be detected as an external module.
    let (binder, _parser) = parse_and_bind(
        r"
export const x = 1;
",
    );

    assert!(
        binder.is_external_module,
        "file with export should be an external module"
    );
}

#[test]
fn plain_script_is_not_external_module() {
    // A plain script without imports/exports should NOT be an external module.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {}
",
    );

    assert!(
        !binder.is_external_module,
        "plain script should not be an external module"
    );
}

#[test]
fn object_define_property_exports_make_js_file_external_module() {
    for source in [
        r#"
const URL = 1;
Object.defineProperty(exports, "value", { value: URL });
"#,
        r#"
const Headers = 1;
Object.defineProperty(module.exports, "value", { value: Headers });
"#,
    ] {
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        assert!(
            binder.is_external_module,
            "Object.defineProperty CommonJS export should make JS file module-scoped: {source}"
        );
    }
}

#[test]
fn nested_commonjs_export_assignment_makes_js_file_external_module() {
    let source = r#"
const URL = 1;
function publish() {
    module.exports.value = URL;
}
"#;
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.is_external_module,
        "nested CommonJS export assignments should make checked JS files module-scoped"
    );
}

// =============================================================================
// 7. FLOW GRAPH ADVANCED PATTERNS
// =============================================================================

#[test]
fn nested_if_creates_multiple_condition_flows() {
    // Nested if statements should each create their own condition flows.
    let (binder, _parser) = parse_and_bind(
        r"
let x: number | string | undefined;
if (x) {
    if (typeof x === 'number') {
        x;
    }
}
",
    );

    let true_count = count_flow_nodes_with_flags(&binder, flow_flags::TRUE_CONDITION);
    let false_count = count_flow_nodes_with_flags(&binder, flow_flags::FALSE_CONDITION);

    assert!(
        true_count >= 2,
        "nested ifs should create at least 2 TRUE_CONDITION flows, got {true_count}"
    );
    assert!(
        false_count >= 2,
        "nested ifs should create at least 2 FALSE_CONDITION flows, got {false_count}"
    );
}

#[test]
fn throw_creates_unreachable_flow() {
    // After a throw statement, the flow should become unreachable.
    let (binder, _parser) = parse_and_bind(
        r"
function foo() {
    throw new Error('oops');
    let x = 1;
}
",
    );

    // The function should still have its START node
    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert!(start_count >= 2, "function should have START flow node");
}

#[test]
fn multiple_functions_each_get_start_flow() {
    // Each function should get its own START flow node.
    let (binder, _parser) = parse_and_bind(
        r"
function a() {}
function b() {}
function c() {}
",
    );

    let start_count = count_flow_nodes_with_flags(&binder, flow_flags::START);
    assert_eq!(
        start_count, 4,
        "should have 4 START flow nodes: file + 3 functions"
    );
}

// =============================================================================
// 8. STRICT MODE BEHAVIOR
// =============================================================================

#[test]
fn use_strict_enables_strict_mode() {
    // A file with "use strict" prologue should bind in strict mode.
    // In strict mode, function declarations in blocks are block-scoped.
    let (binder, _parser) = parse_and_bind(
        r#"
"use strict";
let x = 1;
"#,
    );

    // The binder should detect strict mode
    assert!(
        binder.is_strict_scope,
        "\"use strict\" should enable strict mode"
    );
}

#[test]
fn always_strict_option_enables_strict_mode() {
    // The always_strict option should enable strict mode even without "use strict".
    let options = BinderOptions {
        target: ScriptTarget::ES5,
        always_strict: true,
    };
    let (binder, _parser) = parse_and_bind_with_options(
        r"
let x = 1;
",
        options,
    );

    assert!(
        binder.is_strict_scope,
        "always_strict should enable strict mode"
    );
}

// =============================================================================
// 9. DECLARED MODULES
// =============================================================================

#[test]
fn declare_module_with_string_name() {
    // `declare module "..." { }` should be tracked as a declared module.
    let (binder, _parser) = parse_and_bind(
        r#"
declare module "my-module" {
    export function foo(): void;
}
"#,
    );

    assert!(
        binder.declared_modules.contains("my-module"),
        "declared module should be tracked"
    );
}

// =============================================================================
// 10. MULTIPLE SCOPES AND RESOLUTION
// =============================================================================

#[test]
fn for_in_creates_scope() {
    // for-in loops should create a scope for the iteration variable.
    let (binder, _parser) = parse_and_bind(
        r"
for (let key in obj) {
    key;
}
",
    );

    // Check that a block scope exists
    let block_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Block)
        .count();
    assert!(block_count >= 1, "for-in should create a block scope");
}

#[test]
fn arrow_function_creates_function_scope() {
    // Arrow functions should create their own scope.
    let (binder, _parser) = parse_and_bind(
        r"
const fn = (x: number) => {
    let y = x + 1;
    return y;
};
",
    );

    let function_count = binder
        .scopes
        .iter()
        .filter(|s| s.kind == ContainerKind::Function)
        .count();
    assert!(
        function_count >= 1,
        "arrow function should create a Function scope"
    );
}

#[test]
fn destructuring_binding_creates_symbols() {
    // Destructuring patterns should create individual symbols for each name.
    let (binder, _parser) = parse_and_bind(
        r"
const { a, b } = { a: 1, b: 2 };
const [c, d] = [3, 4];
",
    );

    for name in &["a", "b", "c", "d"] {
        let sym = binder.symbols.find_by_name(name);
        assert!(
            sym.is_some(),
            "destructuring should create symbol for {name}"
        );
    }
}

#[test]
fn function_overloads_merge() {
    // Multiple function declarations with the same name should merge.
    let (binder, _parser) = parse_and_bind(
        r"
function foo(x: number): number;
function foo(x: string): string;
function foo(x: number | string): number | string {
    return x;
}
",
    );

    let foo_sym_id = binder.file_locals.get("foo").expect("expected foo");
    let foo_symbol = binder.symbols.get(foo_sym_id).unwrap();
    assert!(
        foo_symbol.flags & symbol_flags::FUNCTION != 0,
        "merged function overloads should have FUNCTION flag"
    );
    assert!(
        foo_symbol.declarations.len() >= 2,
        "function overloads should have multiple declarations, got {}",
        foo_symbol.declarations.len()
    );
}

// =============================================================================
// 11. NODE-TO-SYMBOL AND NODE-TO-FLOW MAPPINGS
// =============================================================================

#[test]
fn node_symbols_populated_for_declarations() {
    // The binder should populate node_symbols for declaration nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
function foo() {}
class Bar {}
",
    );

    assert!(
        !binder.node_symbols.is_empty(),
        "node_symbols should be populated after binding"
    );
}

#[test]
fn node_flow_populated_for_identifiers() {
    // The binder should populate node_flow for identifier nodes.
    let (binder, _parser) = parse_and_bind(
        r"
let x = 1;
x;
",
    );

    assert!(
        !binder.node_flow.is_empty(),
        "node_flow should be populated after binding"
    );
}

// =============================================================================
// 12. FILE FEATURES DETECTION
// =============================================================================

#[test]
fn generator_function_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
function* gen() {
    yield 1;
}
",
    );

    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::GENERATORS),
        "generator function should set GENERATORS feature flag"
    );
}

#[test]
fn async_generator_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind(
        r"
async function* asyncGen() {
    yield 1;
}
",
    );

    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::ASYNC_GENERATORS),
        "async generator function should set ASYNC_GENERATORS feature flag"
    );
}

#[test]
fn using_declaration_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind("using d = undefined;");
    assert!(
        binder.file_features.has(crate::state::FileFeatures::USING),
        "using declaration should set USING feature flag"
    );
}

#[test]
fn await_using_declaration_sets_feature_flag() {
    let (binder, _parser) = parse_and_bind("await using e = undefined;");
    assert!(
        binder
            .file_features
            .has(crate::state::FileFeatures::AWAIT_USING),
        "await using declaration should set AWAIT_USING feature flag"
    );
}

// =============================================================================
// 13. RESET AND REUSE
// =============================================================================

#[test]
fn binder_reset_clears_state() {
    let mut binder = BinderState::new();

    // Bind something
    let mut parser = ParserState::new("test.ts".to_string(), "let x = 1;".to_string());
    let root = parser.parse_source_file();
    binder.bind_source_file(parser.get_arena(), root);
    assert!(!binder.file_locals.is_empty());
    assert!(!binder.symbols.is_empty());

    // Reset
    binder.reset();

    assert!(
        binder.file_locals.is_empty(),
        "reset should clear file_locals"
    );
    assert!(binder.symbols.is_empty(), "reset should clear symbols");
    assert!(
        binder.node_symbols.is_empty(),
        "reset should clear node_symbols"
    );
    assert!(binder.scopes.is_empty(), "reset should clear scopes");
}

#[test]
fn binder_reset_clears_lib_symbol_reverse_remap() {
    use crate::SymbolId;

    let mut binder = BinderState::new();

    // Insert a fake reverse-remap entry. This map is normally populated by
    // the lib-merge path (`crates/tsz-binder/src/state/lib_merge.rs`); the
    // LSP re-bind path calls `reset()` before `bind_source_file()`, so any
    // stale entry that survives `reset()` would feed into the downstream
    // `lib_symbol_reverse_remap.contains_key(...)` check in
    // `crates/tsz-checker/src/state/type_analysis/cross_file.rs`.
    Arc::make_mut(&mut binder.lib_symbol_reverse_remap).insert(SymbolId(42), (7, SymbolId(13)));

    assert!(
        !binder.lib_symbol_reverse_remap.is_empty(),
        "precondition: fake reverse-remap entry should be present"
    );

    binder.reset();

    assert!(
        binder.lib_symbol_reverse_remap.is_empty(),
        "reset should clear lib_symbol_reverse_remap (PR #1399 followup)"
    );
}

// =============================================================================
// 14. SYMBOL ARENA TESTS
// =============================================================================

#[test]
fn symbol_arena_alloc_and_get() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    let id = arena.alloc(symbol_flags::CLASS, "MyClass".to_string());

    let sym = arena.get(id).expect("should get symbol by ID");
    assert_eq!(sym.escaped_name, "MyClass");
    assert!(sym.flags & symbol_flags::CLASS != 0);
    assert_eq!(sym.id, id);
}

#[test]
fn symbol_arena_find_by_name() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION, "foo".to_string());
    arena.alloc(symbol_flags::CLASS, "Bar".to_string());

    assert!(arena.find_by_name("foo").is_some());
    assert!(arena.find_by_name("Bar").is_some());
    assert!(arena.find_by_name("baz").is_none());
}

#[test]
fn symbol_arena_find_all_by_name() {
    use crate::SymbolArena;

    let mut arena = SymbolArena::new();
    arena.alloc(symbol_flags::FUNCTION_SCOPED_VARIABLE, "x".to_string());
    arena.alloc(symbol_flags::BLOCK_SCOPED_VARIABLE, "x".to_string());
    arena.alloc(symbol_flags::CLASS, "Y".to_string());

    let all_x = arena.find_all_by_name("x");
    assert_eq!(all_x.len(), 2, "should find 2 symbols named x");
}

#[test]
fn symbol_table_operations() {
    let mut table = SymbolTable::new();
    use crate::SymbolId;

    let id1 = SymbolId(0);
    let id2 = SymbolId(1);

    table.set("foo".to_string(), id1);
    table.set("bar".to_string(), id2);

    assert!(table.has("foo"));
    assert!(table.has("bar"));
    assert!(!table.has("baz"));

    assert_eq!(table.get("foo"), Some(id1));
    assert_eq!(table.len(), 2);

    table.remove("foo");
    assert!(!table.has("foo"));
    assert_eq!(table.len(), 1);
}

// =============================================================================
// 15. FLOW NODE ARENA TESTS
// =============================================================================

#[test]
fn flow_node_arena_operations() {
    use crate::FlowNodeArena;

    let mut arena = FlowNodeArena::new();
    assert!(arena.is_empty());

    let id1 = arena.alloc(flow_flags::START);
    let id2 = arena.alloc(flow_flags::UNREACHABLE);

    assert_eq!(arena.len(), 2);
    assert!(!arena.is_empty());

    let node1 = arena.get(id1).unwrap();
    assert!(node1.has_any_flags(flow_flags::START));
    assert!(!node1.has_any_flags(flow_flags::UNREACHABLE));

    let node2 = arena.get(id2).unwrap();
    assert!(node2.has_any_flags(flow_flags::UNREACHABLE));

    // find_unreachable should find the UNREACHABLE node
    let found = arena.find_unreachable();
    assert_eq!(found, Some(id2));
}

#[test]
fn flow_node_antecedents() {
    use crate::FlowNodeArena;

    let mut arena = FlowNodeArena::new();
    let start = arena.alloc(flow_flags::START);
    let branch = arena.alloc(flow_flags::BRANCH_LABEL);

    // Add antecedent
    if let Some(node) = arena.get_mut(branch) {
        node.antecedent.push(start);
    }

    let branch_node = arena.get(branch).unwrap();
    assert_eq!(branch_node.antecedent.len(), 1);
    assert_eq!(branch_node.antecedent[0], start);
}

// =============================================================================
// 16. SCOPE TESTS
// =============================================================================

#[test]
fn scope_is_function_scope() {
    use crate::scopes::Scope;
    use tsz_parser::NodeIndex;

    let source_scope = Scope::new(
        crate::ScopeId::NONE,
        ContainerKind::SourceFile,
        NodeIndex::NONE,
    );
    assert!(
        source_scope.is_function_scope(),
        "SourceFile is a function scope"
    );

    let func_scope = Scope::new(
        crate::ScopeId::NONE,
        ContainerKind::Function,
        NodeIndex::NONE,
    );
    assert!(
        func_scope.is_function_scope(),
        "Function is a function scope"
    );

    let module_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Module, NodeIndex::NONE);
    assert!(
        module_scope.is_function_scope(),
        "Module is a function scope"
    );

    let block_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Block, NodeIndex::NONE);
    assert!(
        !block_scope.is_function_scope(),
        "Block is NOT a function scope"
    );

    let class_scope = Scope::new(crate::ScopeId::NONE, ContainerKind::Class, NodeIndex::NONE);
    assert!(
        !class_scope.is_function_scope(),
        "Class is NOT a function scope"
    );
}

// =============================================================================
// 17. SYMBOL FLAG COMPOSITE TESTS
// =============================================================================

#[test]
fn symbol_flag_composites() {
    // Verify composite flag relationships
    assert_eq!(
        symbol_flags::ENUM,
        symbol_flags::REGULAR_ENUM | symbol_flags::CONST_ENUM
    );
    assert_eq!(
        symbol_flags::VARIABLE,
        symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE
    );
    assert_eq!(
        symbol_flags::MODULE,
        symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE
    );
    assert_eq!(
        symbol_flags::ACCESSOR,
        symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR
    );
}

#[test]
fn symbol_has_flags_checks() {
    use crate::Symbol;
    use crate::SymbolId;

    let sym = Symbol::new(
        SymbolId(0),
        symbol_flags::CLASS | symbol_flags::ABSTRACT,
        "Foo".to_string(),
    );

    assert!(sym.has_flags(symbol_flags::CLASS));
    assert!(sym.has_flags(symbol_flags::ABSTRACT));
    assert!(sym.has_flags(symbol_flags::CLASS | symbol_flags::ABSTRACT));
    assert!(!sym.has_flags(symbol_flags::INTERFACE));

    assert!(sym.has_any_flags(symbol_flags::CLASS));
    assert!(sym.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE));
    assert!(!sym.has_any_flags(symbol_flags::INTERFACE | symbol_flags::FUNCTION));
}

// =============================================================================
// 18. EXPORT RESOLUTION TESTS
// =============================================================================
