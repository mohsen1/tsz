#[test]
fn test_overloaded_call_initializer_does_not_use_first_signature_return_type() {
    let output = emit_dts_with_binding(
        r#"
function parse(input: string): string;
function parse(input: number): number;
function parse(input: string | number): string | number { return input; }
const result = parse(42);
"#,
    );

    assert!(
        !output.contains("declare const result: string;"),
        "Did not expect overloaded call initializer to use the first overload return type: {output}"
    );
    assert!(
        output.contains("declare const result = 42;"),
        "Expected overloaded call initializer to fall back without first-overload poisoning: {output}"
    );
}

#[test]
fn test_private_overloaded_method_initializer_reuses_matching_signature_return_type() {
    let output = emit_dts_with_binding(
        r#"
function noArgs(): string { return null as any; }
function oneArg(input: string): string { return null as any; }

export class Wrapper {
    private proxy<T, U>(fn: (options: T) => U): (options: T) => U;
    private proxy<T, U>(fn: (options?: T) => U, noArgs: true): (options?: T) => U;
    private proxy<T, U>(fn: (options: T) => U) {
        return null as any;
    }

    public Proxies = {
        Failure: this.proxy(noArgs, true),
        Success: this.proxy(oneArg),
    };
}
"#,
    );

    assert!(
        output.contains("Failure: (options?: unknown) => string;"),
        "Expected optional proxy overload to infer a callable return type: {output}"
    );
    assert!(
        output.contains("Success: (options: string) => string;"),
        "Expected one-argument proxy overload to infer a callable return type: {output}"
    );
}

#[test]
fn test_object_literal_computed_accessor_pair_emits_writable_symbol_property() {
    let output = emit_dts_with_binding(
        r#"
var obj = {
    get [Symbol.isConcatSpreadable]() { return ""; },
    set [Symbol.isConcatSpreadable](x) { }
};
"#,
    );

    assert!(
        output.contains("[Symbol.isConcatSpreadable]: string;"),
        "Expected computed accessor pair to collapse to writable symbol property: {output}"
    );
    assert!(
        !output.contains("readonly [Symbol.isConcatSpreadable]: string;"),
        "Did not expect computed accessor pair to remain readonly: {output}"
    );
}

#[test]
fn symbol_observer_computed_member_drops_redundant_index_signature() {
    let source = r#"
interface SymbolConstructor {
    readonly observer: symbol;
}
interface SymbolConstructor {
    readonly observer: unique symbol;
}

const obj = {
    [Symbol.observer]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .map(|(_, decl)| decl)
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[Symbol.observer]: number;"),
        "Expected computed symbol property to survive: {output}"
    );
    assert!(
        !output.contains("[x: number]: number;"),
        "Did not expect redundant synthetic numeric index signature: {output}"
    );
}

#[test]
fn non_symbol_computed_member_preserves_matching_index_signature() {
    let source = r#"
const key = "x";
const obj = {
    [key]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
        })
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[x: number]: number;"),
        "Expected non-Symbol computed property to preserve matching index signature: {output}"
    );
}

#[test]
fn well_known_symbol_computed_member_preserves_matching_index_signature() {
    let source = r#"
const obj = {
    [Symbol.iterator]: 0
};
"#;
    let (parser, root) = parse_test_source(source);

    let obj_decl = parser
        .arena
        .nodes
        .iter()
        .find_map(|node| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("obj"))
        })
        .expect("missing obj declaration");
    let object_literal = parser
        .arena
        .get(obj_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing obj object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("x")),
        }),
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(obj_decl.initializer.0, object_type);
    type_cache
        .node_types
        .insert(prop_assignment.initializer.0, TypeId::NUMBER);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("[Symbol.iterator]: number;"),
        "Expected computed symbol property to survive: {output}"
    );
    assert!(
        output.contains("[x: number]: number;"),
        "Expected non-observer Symbol computed property to preserve matching index signature: {output}"
    );
}

#[test]
fn negative_numeric_computed_member_preserves_computed_syntax() {
    let output = emit_dts_with_usage_analysis(
        r#"
var v = {
    [-1]: {},
    [+1]: {},
    [~1]: {},
    [!1]: {}
};
"#,
    );

    assert!(
        output.contains("[-1]: {};"),
        "Expected negative numeric computed syntax to be preserved: {output}"
    );
    assert!(
        !output.contains("\"-1\": {};"),
        "Did not expect negative numeric computed property to be quoted: {output}"
    );
    assert!(
        !output.contains("[-2]: {};"),
        "Expected non-literal negative numeric key to be covered by the numeric index signature: {output}"
    );
}

#[test]
fn test_object_literal_computed_literal_key_reuses_resolved_property_name() {
    let source = r#"
const Foo = {
    BANANA: "banana" as "banana",
};

export const Baa = {
    [Foo.BANANA]: 1,
};
"#;
    let (parser, root) = parse_test_source(source);

    let baa_decl = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            parser
                .arena
                .get_variable_declaration(node)
                .filter(|decl| parser.arena.get_identifier_text(decl.name) == Some("Baa"))
                .map(|decl| (NodeIndex(idx as u32), decl))
        })
        .map(|(_, decl)| decl)
        .expect("missing Baa declaration");
    let object_literal = parser
        .arena
        .get(baa_decl.initializer)
        .and_then(|node| parser.arena.get_literal_expr(node))
        .expect("missing Baa object literal");
    let prop_assignment = parser
        .arena
        .get(object_literal.elements.nodes[0])
        .and_then(|node| parser.arena.get_property_assignment(node))
        .expect("missing computed property assignment");
    let computed_expr = parser
        .arena
        .get(prop_assignment.name)
        .and_then(|node| parser.arena.get_computed_property(node))
        .map(|computed| computed.expression)
        .expect("missing computed property name");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let banana_type = interner.literal_string("banana");
    let banana_atom = interner.intern_string("banana");
    let object_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: vec![PropertyInfo::new(banana_atom, TypeId::NUMBER)],
        string_index: None,
        number_index: None,
        symbol: None,
    });

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(computed_expr.0, banana_type);
    type_cache
        .node_types
        .insert(baa_decl.initializer.0, object_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("banana: number;"),
        "Expected computed literal key to emit resolved property name: {output}"
    );
    assert!(
        !output.contains("[Foo.BANANA]: number;"),
        "Did not expect computed literal key syntax to leak into declaration output: {output}"
    );
}

#[test]
fn test_local_interface_computed_names_do_not_leak_public_dependencies() {
    let output = emit_dts_with_usage_analysis(
        r#"
const localStringKey = "local";
const localNumberKey = 1;
const publicSymbolKey = Symbol();

interface LocalStringNamed {
    [localStringKey]: number;
}

interface LocalNumberNamed {
    [localNumberKey]: string;
}

export interface PublicNamed {
    [publicSymbolKey]: number;
}
"#,
    );

    assert!(
        !output.contains("localStringKey"),
        "Did not expect local-only interface computed name dependencies to emit: {output}"
    );
    assert!(
        !output.contains("localNumberKey"),
        "Did not expect local-only interface computed name dependencies to emit: {output}"
    );
    assert!(
        output.contains("declare const publicSymbolKey"),
        "Expected exported interface computed name dependency to emit: {output}"
    );
    assert!(
        output.contains("[publicSymbolKey]: number;"),
        "Expected exported interface computed member to emit: {output}"
    );
}

#[test]
fn test_referenced_local_interface_computed_names_keep_dependencies() {
    let output = emit_dts_with_usage_analysis(
        r#"
const localSymbolKey = Symbol();

interface LocalNamed {
    [localSymbolKey]: number;
}

export interface PublicNamed extends LocalNamed {}
"#,
    );

    assert!(
        output.contains("declare const localSymbolKey"),
        "Expected local interface computed name dependency to emit when interface is public: {output}"
    );
    assert!(
        output.contains("interface LocalNamed"),
        "Expected referenced local interface to emit: {output}"
    );
    assert!(
        output.contains("[localSymbolKey]: number;"),
        "Expected referenced local interface computed member to emit: {output}"
    );
}

#[test]
fn test_enum_member_initializers_respect_const_assertion_widening() {
    let output = emit_dts_with_binding(
        r#"
enum E { A, B }
let widened = E.B;
let preserved = E.B as const;
class C {
    p1 = E.B;
    p2 = E.B as const;
    readonly p3 = E.B;
}
"#,
    );

    assert!(
        output.contains("declare let widened: E;"),
        "Expected let enum member to widen to enum type: {output}"
    );
    assert!(
        output.contains("declare let preserved: E.B;"),
        "Expected const-asserted enum member to preserve member type: {output}"
    );
    assert!(
        output.contains("p1: E;"),
        "Expected property widening: {output}"
    );
    assert!(
        output.contains("p2: E.B;"),
        "Expected const-asserted property member type: {output}"
    );
    assert!(
        output.contains("readonly p3 = E.B;"),
        "Expected readonly enum property initializer form: {output}"
    );
}

#[test]
fn test_class_property_initializer_same_name_enum_uses_typeof_enum() {
    let source = r#"
enum Hello {
    World
}
class Foo {
    Hello = Hello;
}
"#;
    let (parser, root) = parse_test_source(source);
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let source_file = parser
        .arena
        .get(root)
        .and_then(|node| parser.arena.get_source_file(node))
        .expect("missing source file");
    let class_idx = source_file.statements.nodes[1];
    let prop_idx = parser
        .arena
        .get(class_idx)
        .and_then(|node| parser.arena.get_class(node))
        .and_then(|class| class.members.nodes.first().copied())
        .expect("missing property");

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(prop_idx.0, TypeId::ANY);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("Hello: typeof Hello;"),
        "Expected same-name enum initializer to emit typeof enum: {output}"
    );
    assert!(
        !output.contains("readonly [x: number]"),
        "Did not expect enum value object shape to leak into property type: {output}"
    );
}

#[test]
fn test_class_property_initializer_same_name_enum_uses_typeof_with_inferred_shape() {
    let output = emit_dts_with_binding(
        r#"
enum Hello {
    World
}
class Foo {
    Hello = Hello;
}
"#,
    );

    assert!(
        output.contains("Hello: typeof Hello;"),
        "Expected same-name enum initializer to emit typeof enum: {output}"
    );
    assert!(
        !output.contains("readonly [x: number]"),
        "Did not expect enum value object shape to leak into property type: {output}"
    );
}

#[test]
fn test_returned_local_conditional_annotation_uses_function_generic_scope() {
    let output = emit_dts_with_binding(
        r#"
function g<T>(x: T) {
    let y: typeof x extends (infer T)[] ? T : typeof x = null as any;
    return y;
}
"#,
    );

    assert!(
        output.contains("declare function g<T>(x: T): T extends (infer T_1)[] ? T_1 : T;"),
        "Expected returned local annotation to substitute parameter type queries and rename shadowed infer type parameter: {output}"
    );
}

#[test]
fn test_generic_class_unrelated_methods_preserve_literal_return_unions() {
    let output = emit_dts_with_binding(
        r#"
export class C<T> {
    m(x: boolean) { return x ? 1 : 2; }
    s(x: boolean) { return x ? "a" : "b"; }
}
"#,
    );

    assert!(
        output.contains("m(x: boolean): 1 | 2;"),
        "Expected generic class method numeric literal union to use source-backed return text: {output}"
    );
    assert!(
        output.contains(r#"s(x: boolean): "a" | "b";"#),
        "Expected generic class method string literal union to use source-backed return text: {output}"
    );
}

#[test]
fn test_const_enum_member_access_const_variable_preserves_initializer() {
    let output = emit_dts_with_binding(
        r#"
export const enum E {
    regular = 0,
    "hyphen-member" = 1,
}
export const a = E["hyphen-member"];
export const b = E.regular;
"#,
    );

    assert!(
        output.contains(r#"export declare const a = E["hyphen-member"];"#),
        "Expected string-keyed const enum member initializer: {output}"
    );
    assert!(
        output.contains("export declare const b = E.regular;"),
        "Expected property const enum member initializer: {output}"
    );
}

#[test]
fn test_inferred_const_from_namespace_infinity_alias_emits_literal() {
    let output = emit_dts_with_binding(
        r#"
export enum Foo {
    A = 1e999,
    B = -1e999,
}

namespace X {
    type A = 1e999;

    export function f(): A {
        throw new Error()
    }
}

export const m = X.f();
"#,
    );

    assert!(
        output.contains("export declare const m: Infinity;"),
        "Expected inaccessible infinity alias return to emit structural literal: {output}"
    );
    assert!(
        !output.contains("export declare const m: A;"),
        "Did not expect inaccessible alias or unqualified enum member to leak: {output}"
    );
}

#[test]
fn test_inferred_const_from_explicit_enum_member_return_keeps_member_type() {
    let output = emit_dts_with_binding(
        r#"
export enum Foo {
    A = 1,
    B = 2,
}

namespace X {
    export function f(): Foo.A {
        throw new Error()
    }
}

export const m = X.f();
"#,
    );

    assert!(
        output.contains("export declare const m: Foo.A;"),
        "Expected explicit enum member return annotation to stay nameable: {output}"
    );
    assert!(
        !output.contains("export declare const m: 1;"),
        "Did not expect explicit enum member return to collapse to literal: {output}"
    );
}

#[test]
fn test_inaccessible_constructor_new_initializer_emits_any() {
    let source = r#"
class C {
    constructor(public x: number) {}
}

class D {
    private constructor(public x: number) {}
}

class E {
    protected constructor(public x: number) {}
}

var c = new C(1);
var d = new D(1);
var e = new E(1);
"#;
    let (parser, root) = parse_test_source(source);

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let class_c_idx = source_file.statements.nodes[0];
    let class_d_idx = source_file.statements.nodes[1];
    let class_e_idx = source_file.statements.nodes[2];
    let var_c_stmt_idx = source_file.statements.nodes[3];
    let var_d_stmt_idx = source_file.statements.nodes[4];
    let var_e_stmt_idx = source_file.statements.nodes[5];

    let class_c = parser
        .arena
        .get(class_c_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class C");
    let class_d = parser
        .arena
        .get(class_d_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class D");
    let class_e = parser
        .arena
        .get(class_e_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class E");

    let var_c_decl = parser
        .arena
        .get(var_c_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var c declaration");
    let var_d_decl = parser
        .arena
        .get(var_d_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var d declaration");
    let var_e_decl = parser
        .arena
        .get(var_e_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing var e declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let c_sym = binder
        .get_node_symbol(class_c.name)
        .or_else(|| binder.get_node_symbol(class_c_idx))
        .expect("missing symbol for C");
    let d_sym = binder
        .get_node_symbol(class_d.name)
        .or_else(|| binder.get_node_symbol(class_d_idx))
        .expect("missing symbol for D");
    let e_sym = binder
        .get_node_symbol(class_e.name)
        .or_else(|| binder.get_node_symbol(class_e_idx))
        .expect("missing symbol for E");

    let interner = TypeInterner::new();
    let c_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(c_sym),
    });
    let d_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(d_sym),
    });
    let e_type = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: Some(e_sym),
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(var_c_decl.name.0, c_type);
    type_cache.node_types.insert(var_d_decl.name.0, d_type);
    type_cache.node_types.insert(var_e_decl.name.0, e_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var c: C;"),
        "Expected C type: {output}"
    );
    assert!(
        output.contains("declare var d: any;"),
        "Expected d to degrade to any: {output}"
    );
    assert!(
        output.contains("declare var e: any;"),
        "Expected e to degrade to any: {output}"
    );
}

#[test]
fn test_construct_signature_new_initializer_keeps_inferred_any() {
    let source = r#"
interface Input {}
interface Factory {
    new (value: Input);
}
declare var ctor: Factory;
declare var value: Input;
var instance = new ctor(value);
"#;
    let (parser, root) = parse_test_source(source);

    let Some(root_node) = parser.arena.get(root) else {
        panic!("missing root node");
    };
    let Some(source_file) = parser.arena.get_source_file(root_node) else {
        panic!("missing source file data");
    };

    let instance_stmt_idx = source_file.statements.nodes[4];
    let instance_decl = parser
        .arena
        .get(instance_stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|stmt| parser.arena.get(stmt.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable(node))
        .and_then(|decl_list| parser.arena.get(decl_list.declarations.nodes[0]))
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing instance declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(instance_decl.name.0, TypeId::ANY);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var instance: any;"),
        "Expected construct-signature new initializer to preserve inferred any: {output}"
    );
    assert!(
        !output.contains("declare var instance: ctor;"),
        "Did not expect constructor variable name to leak into the emitted type: {output}"
    );
}

#[test]
fn test_constructor_type_no_double_semicolon() {
    let source = "export type Ctor = new (...args: any[]) => void;";
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("new (...args: any[]) => void;"),
        "Expected constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in constructor type alias: {output}"
    );
}

#[test]
fn test_template_literal_type_no_double_semicolon() {
    let source = r#"export type Outcome = `${string}_${string}`;"#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`${string}_${string}`"),
        "Expected template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in template literal type alias: {output}"
    );
}

#[test]
fn test_infer_type_no_double_semicolon() {
    let source = "export type Unpack<T> = T extends (infer U)[] ? U : T;";
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("infer U"),
        "Expected infer type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in type alias with infer: {output}"
    );
}

#[test]
fn test_abstract_constructor_type() {
    let source = "export type AbstractCtor = abstract new () => object;";
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("abstract new () => object;"),
        "Expected abstract constructor type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in abstract constructor type: {output}"
    );
}

#[test]
fn test_simple_template_literal_type() {
    let source = r#"export type Greeting = `hello`;"#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("`hello`"),
        "Expected simple template literal type in output: {output}"
    );
    assert!(
        !output.contains(";;"),
        "Must not have double semicolon in simple template literal type: {output}"
    );
}

#[test]
fn test_public_modifier_omitted_from_dts_class_members() {
    // tsc omits `public` from .d.ts output since it's the default accessibility
    let source = r#"
    export class Foo {
        public x: number;
        public greet(): string { return "hello"; }
        protected y: number;
        private z: number;
    }
    "#;
    let (parser, root) = parse_test_source(source);

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    // `public` should be stripped (it's the default)
    assert!(
        !output.contains("public "),
        "Expected `public` modifier to be omitted from .d.ts output: {output}"
    );
    // `protected` and `private` should be preserved
    assert!(
        output.contains("protected y"),
        "Expected `protected` modifier to be preserved: {output}"
    );
    assert!(
        output.contains("private z"),
        "Expected `private` modifier to be preserved: {output}"
    );
    // Members themselves should still be present
    assert!(
        output.contains("x: number"),
        "Expected public property to still be emitted (without modifier): {output}"
    );
    assert!(
        output.contains("greet("),
        "Expected public method to still be emitted (without modifier): {output}"
    );
}

// =============================================================================
// 2. Variable Declarations
// =============================================================================

#[test]
fn test_variable_const_declaration() {
    let output = emit_dts("export const MAX: number = 100;");
    assert!(
        output.contains("export declare const MAX: number;"),
        "Expected const variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_let_declaration() {
    let output = emit_dts("export let count: number = 0;");
    assert!(
        output.contains("export declare let count: number;"),
        "Expected let variable in .d.ts: {output}"
    );
}

#[test]
fn test_variable_var_declaration() {
    let output = emit_dts("export var name: string = 'hello';");
    assert!(
        output.contains("export declare var name: string;"),
        "Expected var variable in .d.ts: {output}"
    );
}

// =============================================================================
// 3. Visibility / Access Modifiers
// =============================================================================

#[test]
fn test_private_method_emits_name_only() {
    // tsc emits just `private methodName;` for private methods
    let output = emit_dts(
        r#"
    export class Foo {
        private secret(): void {}
    }
    "#,
    );
    assert!(
        output.contains("private secret;"),
        "Expected private method to emit name only: {output}"
    );
    // Should NOT include parameters or return type
    assert!(
        !output.contains("private secret()"),
        "Private method should not have params in .d.ts: {output}"
    );
}

#[test]
fn test_protected_member_included() {
    let output = emit_dts(
        r#"
    export class Foo {
        protected bar: number;
    }
    "#,
    );
    assert!(
        output.contains("protected bar: number;"),
        "Expected protected member to be included: {output}"
    );
}

#[test]
fn test_private_property_omits_type_annotation() {
    // tsc omits type annotations for private properties in .d.ts
    let output = emit_dts(
        r#"
    export class Foo {
        private value: number;
    }
    "#,
    );
    assert!(
        output.contains("private value;"),
        "Expected private property without type annotation: {output}"
    );
    assert!(
        !output.contains("private value: number;"),
        "Private property should NOT have type annotation: {output}"
    );
}

// =============================================================================
// 4. Export Handling
// =============================================================================

#[test]
fn test_named_export_with_specifiers() {
    let output = emit_dts(
        r#"
    const a: number = 1;
    const b: string = "x";
    export { a, b };
    "#,
    );
    assert!(
        output.contains("export { a, b }"),
        "Expected named export specifiers: {output}"
    );
}

#[test]
fn test_re_export_from_module() {
    let output = emit_dts(r#"export { foo, bar } from "./other";"#);
    assert!(
        output.contains("export { foo, bar } from"),
        "Expected re-export: {output}"
    );
}

#[test]
fn test_star_re_export() {
    let output = emit_dts(r#"export * from "./utils";"#);
    assert!(
        output.contains("export * from"),
        "Expected star re-export: {output}"
    );
}

#[test]
fn test_type_only_export() {
    let output = emit_dts(r#"export type { Foo } from "./types";"#);
    assert!(
        output.contains("export type { Foo }"),
        "Expected type-only export: {output}"
    );
}

#[test]
fn test_export_default_identifier() {
    // export default <identifier> should emit directly
    let output = emit_dts(
        r#"
    declare const myValue: number;
    export default myValue;
    "#,
    );
    assert!(
        output.contains("export default myValue;"),
        "Expected export default identifier: {output}"
    );
}

#[test]
fn test_js_export_default_identifier_emits_before_local_declaration() {
    // For JS source files, tsc emits `export default <Identifier>` before the
    // referenced local declaration when the identifier resolves to a top-level
    // local declaration. With no earlier public declaration, the default export
    // is the first output line. Repro for jsDeclarationEmitDoesNotRenameImport.
    let output = emit_js_dts(
        r#"
function validate() {}

export default validate;
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export default validate;"),
        "Expected `export default validate;` before the local declaration: {trimmed}"
    );
    let count = trimmed.matches("export default validate;").count();
    assert_eq!(
        count, 1,
        "Expected exactly one export-default emission: {trimmed}"
    );
    assert!(
        trimmed.contains("declare function validate(): void;"),
        "Expected the function declaration to follow: {trimmed}"
    );
    let default_pos = trimmed.find("export default validate;").unwrap();
    let decl_pos = trimmed.find("declare function validate").unwrap();
    assert!(
        default_pos < decl_pos,
        "`export default` should appear before the function declaration: {trimmed}"
    );
}

#[test]
fn test_redundant_named_import_alias_extends_uses_canonical_name() {
    let output = emit_dts_with_usage_analysis(
        r#"
import { Base, Base as Base2 } from "pkg";
export class A extends Base {}
export class B extends Base2 {}
"#,
    );

    assert!(
        output.contains("export declare class A extends Base"),
        "Expected first class to keep canonical import name: {output}"
    );
    assert!(
        output.contains("export declare class B extends Base"),
        "Expected aliased class heritage to use canonical import name: {output}"
    );
    assert!(
        output.contains("import { Base } from \"pkg\";"),
        "Expected redundant named import alias to be elided: {output}"
    );
    assert!(
        !output.contains("Base2"),
        "Did not expect declaration output to reference redundant alias: {output}"
    );
}

#[test]
fn test_js_export_default_class_emits_before_local_declaration() {
    // Same source-position scheduling rule as above, but for class declarations. Uses the
    // usage-analysis variant so the class isn't pruned from the .d.ts.
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @module Test */
class Test {}
export default Test;
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export default Test;"),
        "Expected `export default Test;` before the local class declaration: {trimmed}"
    );
    let count = trimmed.matches("export default Test;").count();
    assert_eq!(
        count, 1,
        "Expected exactly one export-default emission: {trimmed}"
    );
    let default_pos = trimmed.find("export default Test;").unwrap();
    let decl_pos = trimmed
        .find("declare class Test")
        .unwrap_or_else(|| panic!("expected `declare class Test` in JS dts: {trimmed}"));
    assert!(
        default_pos < decl_pos,
        "`export default` should appear before the class declaration: {trimmed}"
    );
}

#[test]
fn test_js_default_identifier_preserves_preceding_exported_declarations() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @module A */
class A {}

export const x = 1;
/**
 * Target element
 * @type {module:A}
 */
export let el = null;

export default A;
"#,
    );

    let expected = r#"export const x: 1;
/**
 * Target element
 * @type {module:A}
 */
export let el: any;
export default A;
/** @module A */
declare class A {
}
"#;

    assert_eq!(
        output, expected,
        "Expected source-position default export scheduling with deferred local class: {output}"
    );
}

#[test]
fn test_js_default_identifier_keeps_exported_target_in_source_order() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export const A = 1;
export default A;
"#,
    );

    let expected = "export const A: 1;\nexport default A;\n";
    assert_eq!(
        output, expected,
        "Expected exported default target to stay in source order without a duplicate declaration"
    );
}

#[test]
fn test_jsdoc_module_reference_variable_type_falls_back_to_any() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
/** @type {module:pkg.Name} */
export let pkg = null;
"#,
    );

    assert!(
        output.contains("/** @type {module:pkg.Name} */\nexport let pkg: any;"),
        "Expected `module:` JSDoc references to emit as any: {output}"
    );
    assert!(
        !output.contains("pkg: null"),
        "JSDoc @type must take precedence over JS null inference: {output}"
    );
}

#[test]
fn test_js_default_typedef_after_default_identifier_export_uses_export_name() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
class Cls {
    x = 12;
}
export default Cls;
/** @typedef {string | number} default */
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed.starts_with("export type Cls = string | number;\nexport default Cls;"),
        "Expected default typedef to reuse the default-exported class name before the source-position default export: {trimmed}"
    );
    assert!(
        !trimmed.contains("export type Cls_1 = string | number;"),
        "Default typedef alias should not synthesize a unique name for a default-exported class: {trimmed}"
    );
    assert!(
        trimmed.contains("declare class Cls"),
        "Expected the exported class declaration to remain: {trimmed}"
    );
}

#[test]
fn test_ts_export_default_identifier_is_not_hoisted() {
    // TS files keep `export default <Identifier>` in source order — only JS
    // declaration emit applies the hoist transformation.
    let output = emit_dts(
        r#"
function validate() {}
export default validate;
"#,
    );
    let trimmed = output.trim();
    let default_pos = trimmed
        .find("export default validate;")
        .expect("expected export default validate; in TS output");
    let decl_pos = trimmed
        .find("declare function validate")
        .expect("expected declare function validate in TS output");
    assert!(
        decl_pos < default_pos,
        "TS files should preserve source order (declaration first): {trimmed}"
    );
}

/// Regression: a TypeScript class whose computed-name members appear
/// *before* the constructor must keep that order in d.ts.  Prior code
/// hoisted the constructor between statics and instance members
/// whenever a class had any computed name, which mangled
/// `[a]: number; [b]: number; constructor();` into
/// `constructor(); [a]: number; [b]: number;`.  tsc preserves source
/// order here (statics still hoist, but the constructor stays in its
/// non-static slot).
#[test]
fn ts_class_with_computed_names_keeps_constructor_after_instance_members() {
    let output = emit_dts(
        r#"
declare const a: 'a';
declare const b: unique symbol;
class C12 {
    [a]: number;
    [b]: number;
    ['c']: number;
    constructor() {}
}
"#,
    );
    let trimmed = output.trim();
    let a_pos = trimmed.find("[a]: number;").expect("expected [a] member");
    let b_pos = trimmed.find("[b]: number;").expect("expected [b] member");
    let c_pos = trimmed
        .find("['c']: number;")
        .expect("expected ['c'] member");
    let ctor_pos = trimmed
        .find("constructor();")
        .expect("expected constructor declaration");
    assert!(
        a_pos < b_pos && b_pos < c_pos && c_pos < ctor_pos,
        "TS class with computed names should preserve source order — instance members before constructor: {trimmed}"
    );
}

/// Regression: a `TupleType` whose source has JSDoc comments preceding
/// individual members must round-trip in d.ts emit as a multi-line
/// tuple with each comment on its own line, mirroring tsc's behaviour
/// (see `namedTupleMembers.SegmentAnnotated`).
///
/// Counter-regression: tuples *without* leading JSDoc on any member
/// must keep the compact one-line shape — the multi-line switch is
/// JSDoc-only, not "any time we have named tuple members" or "any
/// time we have a rest element".
#[test]
fn ts_tuple_with_jsdoc_member_emits_multiline_with_comments() {
    let output = emit_dts(
        r#"
export type SegmentAnnotated = [
    /**
     * Size of message buffer segment handles
     */
    length: number,
    /**
     * Number of segments handled at once
     */
    count: number
];
"#,
    );
    assert!(
        output.contains("/**\n     * Size of message buffer segment handles\n     */"),
        "tuple-member JSDoc should round-trip in d.ts emit: {output}"
    );
    assert!(
        output.contains("/**\n     * Number of segments handled at once\n     */"),
        "second tuple-member JSDoc should round-trip too: {output}"
    );
    let length_idx = output.find("length: number").expect("length member");
    let count_idx = output.find("count: number").expect("count member");
    assert!(
        length_idx < count_idx,
        "tuple member order must be preserved: {output}"
    );
}

#[test]
fn ts_tuple_without_jsdoc_member_keeps_single_line_form() {
    let output = emit_dts(
        r#"
export type Segment = [length: number, count: number];
export type WithRest = [first: number, second?: number, ...rest: string[]];
"#,
    );
    let trimmed = output.trim();
    assert!(
        trimmed
            .lines()
            .any(|l| l.contains("export type Segment = [length: number, count: number];")),
        "non-annotated tuple should stay single-line: {output}"
    );
    assert!(
        trimmed.lines().any(|l| l.contains(
            "export type WithRest = [first: number, second?: number, ...rest: string[]];"
        )),
        "rest-only tuple without JSDoc should stay single-line: {output}"
    );
}

/// Counter-regression: when computed-named instance members appear in
/// source order *before* static members, the static members must still
/// hoist to the top of the d.ts class body — that's the actual rule
/// tsc follows for computed-name TS classes (see
/// `declarationEmitSimpleComputedNames1`).  Verifies the static-hoist
/// rule didn't regress when the constructor-handling fix landed.
#[test]
fn ts_class_with_computed_names_hoists_static_members_above_instance() {
    let output = emit_dts(
        r#"
declare const classFieldName: string;
declare const otherField: string;
declare const staticField: string;
export class Holder {
    [classFieldName]() { return "value"; }
    [otherField]() { return 42; }
    static [staticField]() { return { static: true as boolean }; }
    static [staticField]() { return { static: "sometimes" as string }; }
}
"#,
    );
    let trimmed = output.trim();
    let static_a = trimmed
        .find("static [staticField]")
        .expect("expected first static member");
    let instance_a = trimmed
        .find("[classFieldName]")
        .expect("expected first instance member");
    let instance_b = trimmed
        .find("[otherField]")
        .expect("expected second instance member");
    assert!(
        static_a < instance_a && static_a < instance_b,
        "static members should hoist above instance members for TS classes with computed names: {trimmed}"
    );
}

/// Direct regression test for the trim helper used by
/// `type_argument_list_source_text`.  Two-axis property: a bare
/// overshoot `Foo>` becomes `Foo`, and a nested balanced `<…>` like
/// `C.A<C.B>` is left intact (naive trimming would corrupt it into
/// `C.A<C.B`).  The parser's `token_full_start()` correctly anchors
/// `TypeReference` ends; only `LiteralType`/`UnionType`/
/// `IntersectionType` have the `token_end()` overshoot quirk this
/// helper fixes.
#[test]
fn strip_type_argument_overshoot_balances_nested_angle_brackets() {
    use crate::declaration_emitter::DeclarationEmitter;

    let mut overshoot = String::from("\"Hello\">");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut overshoot);
    assert_eq!(
        overshoot, "\"Hello\"",
        "literal-type overshoot must be trimmed"
    );

    let mut nested = String::from("C.A<C.B>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested);
    assert_eq!(
        nested, "C.A<C.B>",
        "balanced nested `<…>` must not be trimmed"
    );

    let mut nested_with_overshoot = String::from("C.A<C.B>>");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut nested_with_overshoot);
    assert_eq!(
        nested_with_overshoot, "C.A<C.B>",
        "trailing overshoot `>` must be trimmed but inner `>` kept"
    );

    let mut trailing_comma = String::from("\"foo\", ");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut trailing_comma);
    assert_eq!(
        trailing_comma, "\"foo\"",
        "trailing `,`/whitespace must drop"
    );

    let mut quoted_gt = String::from("\"a>b\"");
    DeclarationEmitter::strip_type_argument_overshoot_for_test(&mut quoted_gt);
    assert_eq!(
        quoted_gt, "\"a>b\"",
        "`>` inside string literals must not affect the balance count"
    );
}

#[test]
fn test_js_exported_class_emits_documented_constructor_assignment_field() {
    let source = r#"
export class Aleph {
    /**
     * Impossible to construct.
     * @param {Aleph} a
     * @param {null} b
     */
	    constructor(a, b) {
	        /**
	         * Field is always null
	         */
	        this.field = b;
	        /**
	         * Explicitly typed count.
	         * @type {number}
	         */
	        this.count = 1;
	    }

    /**
     * Doesn't actually do anything
     * @returns {void}
     */
    doIt() {}
	}
	"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains(
            "/**\n     * Field is always null\n     */\n    field: any;\n    /**\n     * Explicitly typed count.\n     * @type {number}\n     */\n    count: number;\n    /**\n     * Doesn't actually do anything"
        ),
        "Expected documented constructor assignment field before method declaration: {output}"
    );
}

#[test]
fn test_js_constructor_assignment_single_line_type_comment_stays_compact() {
    let source = r#"
/**
 * @typedef {string | number} Whatever
 */
class Conn {
    constructor() {}
    item = 3;
    method() {}
}

class Wrap {
    /**
     * @param {Conn} c
     */
    constructor(c) {
        this.connItem = c.item;
        /** @type {Whatever} */
        this.another = "";
    }
}

export { Wrap };
"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains("    /** @type {Whatever} */\n    another: Whatever;"),
        "Expected single-line constructor assignment @type JSDoc to stay compact: {output}"
    );
    assert!(
        output.contains("export type Whatever = string | number;"),
        "Expected exported typedef alias used by compact @type comment to be emitted: {output}"
    );
}

#[test]
fn test_js_local_bare_require_alias_without_exports_is_elided() {
    let source = r#"
const u = require("untyped");
u.assignment.nested = true;
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        !output.contains("declare const u"),
        "Expected local bare require alias in a non-exporting JS module to be elided: {output}"
    );
    assert_eq!("export {};", output.trim());
}

#[test]
fn test_js_local_destructured_require_alias_without_exports_is_elided() {
    let source = r#"
const { apply } = require("./moduleExportAliasDuplicateAlias");
const result = apply.toFixed();
"#;
    let output = emit_js_dts_with_usage_analysis(source);

    assert!(
        !output.contains("declare const apply"),
        "Expected local destructured require alias in a non-exporting JS module to be elided: {output}"
    );
    assert!(
        !output.contains("declare const result"),
        "Expected locals derived from the elided destructured require alias to be omitted: {output}"
    );
    assert_eq!("export {};", output.trim());
}

#[test]
fn test_js_local_dynamic_require_alias_without_exports_is_preserved() {
    let source = r#"
const moduleName = "untyped";
const u = require(moduleName);
u.noError();
"#;
    let output = emit_js_dts(source);

    assert!(
        output.contains("declare const u: any;"),
        "Expected dynamic require alias to be preserved: {output}"
    );
}

#[test]
fn test_js_returned_function_expression_uses_attached_jsdoc_signature() {
    let output = emit_js_dts(
        r#"
function f1() {
    /**
     * @param {number} a
     * @param {number} b
     * @returns {number}
     */
    return (a, b) => a + b;
}

function f2() {
    /** @type {(a: string, b: string) => string} */
    return function (a, b) {
        return a + b;
    };
}
"#,
    );

    assert!(
        output.contains("declare function f1(): (a: number, b: number) => number;"),
        "Expected returned arrow signature to use attached @param/@returns JSDoc: {output}"
    );
    assert!(
        output.contains("declare function f2(): (a: string, b: string) => string;"),
        "Expected returned function expression signature to use attached @type JSDoc: {output}"
    );
}

#[test]
fn test_js_export_equals_function_static_assignments_stay_top_level() {
    let output = emit_js_dts(
        r#"
module.exports = MyClass;

function MyClass() {}
MyClass.staticMethod = function() {}
MyClass.prototype.method = function() {}
MyClass.staticProperty = 123;
"#,
    );

    assert!(
        output.contains("export = MyClass;"),
        "Expected CommonJS export assignment: {output}"
    );
    assert!(
        output.contains(
            "declare namespace MyClass {\n    export { staticMethod, staticProperty };\n}"
        ),
        "Expected namespace to re-export top-level expando declarations: {output}"
    );
    assert!(
        output.contains("declare function staticMethod(): void;"),
        "Expected static function expando to remain a top-level declaration: {output}"
    );
    assert!(
        output.contains("declare var staticProperty: number;"),
        "Expected static value expando to remain a top-level declaration: {output}"
    );
    assert!(
        !output.contains("declare namespace MyClass {\n    function staticMethod(): void;"),
        "Did not expect static expandos to be folded into the namespace body: {output}"
    );
}

#[test]
fn test_js_function_static_properties_export_from_merged_namespace() {
    let output = emit_js_dts(
        r#"
function foo() {}
foo.x = 1;
foo.default = 2;
"#,
    );

    assert!(
        output.contains("declare namespace foo {\n    export let x: number;"),
        "Expected direct expando property to get export let when a reserved-word sibling requires aliasing: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved expando property to use local alias plus export specifier: {output}"
    );
}

#[test]
fn test_js_function_expando_function_member_exported_when_alias_sibling_present() {
    let output = emit_js_dts(
        r#"
function bar() {}
bar.greet = function(name) { return name; };
bar.default = 42;
"#,
    );

    assert!(
        output.contains("export function greet"),
        "Expected function-valued expando member to get export when a reserved-word sibling requires aliasing: {output}"
    );
    assert!(
        output.contains("let _default: number;\n    export { _default as default };"),
        "Expected reserved-word alias emission for default: {output}"
    );
}

