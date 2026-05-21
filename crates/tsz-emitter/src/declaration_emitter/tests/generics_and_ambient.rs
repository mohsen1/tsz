use super::*;

// =============================================================================
// 6. Generic Declarations
// =============================================================================

#[test]
fn test_generic_function() {
    let output = emit_dts("export function identity<T>(x: T): T { return x; }");
    assert!(
        output.contains("<T>"),
        "Expected generic type parameter: {output}"
    );
    assert!(
        output.contains("x: T"),
        "Expected parameter with generic type: {output}"
    );
    assert!(
        output.contains("): T;"),
        "Expected return type with generic: {output}"
    );
}

#[test]
fn test_generic_interface_with_constraint() {
    let output = emit_dts(
        r#"
    export interface Container<T extends object> {
        value: T;
    }
    "#,
    );
    assert!(
        output.contains("<T extends object>"),
        "Expected generic type parameter with constraint: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected member with generic type: {output}"
    );
}

#[test]
fn test_generic_class_with_default() {
    let output = emit_dts(
        r#"
    export class Box<T = string> {
        content: T;
        constructor(value: T) { this.content = value; }
    }
    "#,
    );
    assert!(
        output.contains("<T = string>"),
        "Expected generic type parameter with default: {output}"
    );
}

#[test]
fn test_multiple_type_parameters() {
    let output = emit_dts(
        "export function map<T, U>(arr: T[], fn: (x: T) => U): U[] { return arr.map(fn); }",
    );
    assert!(
        output.contains("<T, U>"),
        "Expected multiple type parameters: {output}"
    );
}

// =============================================================================
// 7. Ambient / Declare Declarations
// =============================================================================

#[test]
fn test_declare_class_passthrough() {
    let output = emit_dts(
        r#"
    declare class Foo {
        bar(): void;
    }
    "#,
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected declare class: {output}"
    );
    assert!(
        output.contains("bar(): void;"),
        "Expected method signature: {output}"
    );
}

#[test]
fn test_declare_function_passthrough() {
    let output = emit_dts("declare function greet(name: string): void;");
    assert!(
        output.contains("declare function greet(name: string): void;"),
        "Expected declare function: {output}"
    );
}

#[test]
fn test_declare_var_passthrough() {
    let output = emit_dts("declare var globalName: string;");
    assert!(
        output.contains("declare var globalName: string;"),
        "Expected declare var: {output}"
    );
}

// =============================================================================
// 8. Module / Namespace Declarations
// =============================================================================

#[test]
fn test_namespace_declaration() {
    let output = emit_dts(
        r#"
    export declare namespace MyLib {
        function create(): void;
        class Widget {
            name: string;
        }
    }
    "#,
    );
    assert!(
        output.contains("export declare namespace MyLib"),
        "Expected namespace declaration: {output}"
    );
    assert!(
        output.contains("function create(): void;"),
        "Expected function in namespace: {output}"
    );
    assert!(
        output.contains("class Widget"),
        "Expected class in namespace: {output}"
    );
}

#[test]
fn test_nested_namespace() {
    let output = emit_dts(
        r#"
    export declare namespace Outer {
        namespace Inner {
            const value: number;
        }
    }
    "#,
    );
    assert!(
        output.contains("namespace Outer"),
        "Expected outer namespace: {output}"
    );
    assert!(
        output.contains("namespace Inner"),
        "Expected inner namespace: {output}"
    );
}

// =============================================================================
// 9. Enum Declarations
// =============================================================================

#[test]
fn test_regular_enum() {
    let output = emit_dts(
        r#"
    export enum Color {
        Red,
        Green,
        Blue
    }
    "#,
    );
    assert!(
        output.contains("export declare enum Color"),
        "Expected exported declare enum: {output}"
    );
    assert!(output.contains("Red"), "Expected Red member: {output}");
    assert!(output.contains("Green"), "Expected Green member: {output}");
    assert!(output.contains("Blue"), "Expected Blue member: {output}");
}

#[test]
fn test_const_enum() {
    let output = emit_dts(
        r#"
    export const enum Direction {
        Up = 0,
        Down = 1,
        Left = 2,
        Right = 3
    }
    "#,
    );
    assert!(
        output.contains("export declare const enum Direction"),
        "Expected exported declare const enum: {output}"
    );
    assert!(output.contains("Up = 0"), "Expected Up = 0: {output}");
    assert!(output.contains("Right = 3"), "Expected Right = 3: {output}");
}

#[test]
fn test_invalid_const_enum_object_index_access_emits_any() {
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 1,
    B = 2,
}
let z1 = G[G.A];
"#,
    );

    assert!(
        output.contains("declare let z1: any;"),
        "Expected invalid const enum object index access to emit any: {output}"
    );
}

#[test]
fn test_string_enum() {
    let output = emit_dts(
        r#"
    export enum Status {
        Active = "active",
        Inactive = "inactive"
    }
    "#,
    );
    assert!(
        output.contains("Active = \"active\""),
        "Expected string enum value: {output}"
    );
    assert!(
        output.contains("Inactive = \"inactive\""),
        "Expected string enum value: {output}"
    );
}

#[test]
fn test_enum_auto_increment() {
    let output = emit_dts(
        r#"
    export enum Seq {
        A = 10,
        B,
        C
    }
    "#,
    );
    assert!(output.contains("A = 10"), "Expected A = 10: {output}");
    assert!(
        output.contains("B = 11"),
        "Expected B = 11 (auto-increment): {output}"
    );
    assert!(
        output.contains("C = 12"),
        "Expected C = 12 (auto-increment): {output}"
    );
}

// =============================================================================
// Generic method return types with indexed access (issue #8520)
// =============================================================================

/// Build a minimal type cache for a generic class method where the checker has stored
/// an `IndexAccess` type as the method's node type (as opposed to a function type).
fn emit_dts_with_index_access_return(source: &str, method_name_str: &str) -> String {
    use tsz_parser::parser::syntax_kind_ext;

    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();

    // Find the method declaration node.
    let method_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != syntax_kind_ext::METHOD_DECLARATION {
                return None;
            }
            let nidx = NodeIndex(idx as u32);
            let decl = parser.arena.get_method_decl(parser.arena.get(nidx)?)?;
            if parser.arena.get_identifier_text(decl.name)? == method_name_str {
                Some(nidx)
            } else {
                None
            }
        })
        .expect("method not found");

    // Find the class node to get its type params.
    let class_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::CLASS_DECLARATION).then_some(NodeIndex(idx as u32))
        })
        .expect("class not found");

    // Collect all in-scope type parameter node indices (class + method).
    let class_type_param_nodes: Vec<NodeIndex> = parser
        .arena
        .get(class_idx)
        .and_then(|n| parser.arena.get_class(n))
        .and_then(|c| c.type_parameters.as_ref())
        .map(|list| list.nodes.clone())
        .unwrap_or_default();

    let method_decl = parser
        .arena
        .get(method_idx)
        .and_then(|n| parser.arena.get_method_decl(n))
        .expect("method decl not found");

    let method_type_param_nodes: Vec<NodeIndex> = method_decl
        .type_parameters
        .as_ref()
        .map(|list| list.nodes.clone())
        .unwrap_or_default();

    // Build TypeId for each type parameter by reading its name from the AST.
    let mut param_type_ids: Vec<(String, TypeId)> = Vec::new();
    for &idx in class_type_param_nodes
        .iter()
        .chain(method_type_param_nodes.iter())
    {
        let name = parser
            .arena
            .get(idx)
            .and_then(|n| parser.arena.get_type_parameter(n))
            .and_then(|p| parser.arena.get_identifier_text(p.name))
            .expect("type param name");
        let atom = interner.intern_string(name);
        let type_id = interner.type_param(tsz_solver::TypeParamInfo::simple(atom));
        param_type_ids.push((name.to_string(), type_id));
    }

    // Build the IndexAccess type: <first_class_param>[<first_method_param>].
    // For `getProperty<K>(key: K): PropType[K]` that is PropType[K].
    let obj_type = param_type_ids.first().expect("class type param").1;
    let idx_type = param_type_ids.last().expect("method type param").1;
    let return_type_id = interner.index_access(obj_type, idx_type);

    let mut node_types = FxHashMap::default();
    node_types.insert(method_idx.0, return_type_id);

    let type_cache = crate::type_cache_view::TypeCacheView {
        node_types,
        ..Default::default()
    };

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.emit(root)
}

#[test]
fn test_generic_class_method_indexed_access_return_class_param() {
    // Method returns PropType[K] where PropType is the class type param and K is the method's.
    let output = emit_dts_with_index_access_return(
        r#"
export class Component<PropType> {
    props: PropType;
    constructor(props: PropType) { this.props = props; }
    getProperty<K extends keyof PropType>(key: K): PropType[K] {
        return this.props[key];
    }
}
"#,
        "getProperty",
    );
    assert!(
        output.contains("getProperty<K extends keyof PropType>(key: K): PropType[K]"),
        "Expected indexed-access return type with class type param: {output}"
    );
}

#[test]
fn test_generic_class_method_indexed_access_return_different_names() {
    // Same semantic rule, different type-parameter names — guards against hardcoded-name fixes.
    let output = emit_dts_with_index_access_return(
        r#"
export class Store<TState> {
    state: TState;
    constructor(s: TState) { this.state = s; }
    select<TKey extends keyof TState>(k: TKey): TState[TKey] {
        return this.state[k];
    }
}
"#,
        "select",
    );
    assert!(
        output.contains("select<TKey extends keyof TState>(k: TKey): TState[TKey]"),
        "Expected indexed-access return type with renamed type params: {output}"
    );
}

#[test]
fn test_generic_class_method_indexed_access_non_generic_class_unaffected() {
    // Non-generic class: the fix must not change output for plain methods.
    let output = emit_dts(
        r#"
export class Plain {
    get(key: string): string { return ""; }
}
"#,
    );
    assert!(
        output.contains("get(key: string): string"),
        "Non-generic class method should still emit correctly: {output}"
    );
}
