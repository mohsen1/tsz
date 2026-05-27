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
fn test_generic_class_constructor_options_infer_object_member_type_arg() {
    let output = emit_dts_with_binding(
        r#"
interface WidgetOptions<State, Computed> {
    state?: State;
    computed?: Computed;
}

declare class Widget<State, Computed> {
    constructor(options: WidgetOptions<State, Computed>);
}

let widget = new Widget({
    state: {
        title: ""
    }
});
"#,
    );
    assert!(
        output.contains("declare let widget: Widget<{\n    title: string;\n}, unknown>;"),
        "Expected constructor option object member to infer the matching class type argument: {output}"
    );
}

#[test]
fn test_generic_class_constructor_options_maps_option_type_params_by_position() {
    let output = emit_dts_with_binding(
        r#"
interface SetupBox<Input, Output> {
    payload?: Input;
    result?: Output;
}

declare class Machine<Seed, Product> {
    constructor(settings: SetupBox<Seed, Product>);
}

let machine = new Machine({
    payload: {
        enabled: true
    }
});
"#,
    );
    assert!(
        output.contains("declare let machine: Machine<{\n    enabled: boolean;\n}, unknown>;"),
        "Expected option type parameter names to map to class type arguments by position: {output}"
    );
}

#[test]
fn test_generic_call_this_type_descriptor_intersections_preserve_source_surfaces() {
    let output = emit_dts_with_binding(
        r#"
type Point = {
    x: number;
    y: number;
    moveBy(dx: number, dy: number): void;
};

type ObjectDescriptor<D, M> = {
    data?: D;
    methods?: M & ThisType<D & M>;
};

declare function makeObject<D, M>(desc: ObjectDescriptor<D, M>): D & M;

let x = makeObject({
    data: { x: 0, y: 0 },
    methods: {
        moveBy(dx: number, dy: number) {}
    }
});

type PropDesc<T> = {
    value?: T;
    get?(): T;
    set?(value: T): void;
};

declare function defineProp<T, K extends string, U>(obj: T, name: K, desc: PropDesc<U> & ThisType<T>): T & Record<K, U>;

declare const point: Point;
let p = defineProp(point, "foo", { value: 42 });
"#,
    );

    assert!(
        output.contains(
            "declare let x: {\n    x: number;\n    y: number;\n} & {\n    moveBy(dx: number, dy: number): void;\n};"
        ),
        "Expected source object descriptor call to preserve `D & M`: {output}"
    );
    assert!(
        output.contains("declare let p: Point & Record<\"foo\", number>;"),
        "Expected descriptor intersection call to preserve alias and `Record`: {output}"
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

#[test]
fn test_inferred_return_preserves_mapped_parameter_annotation() {
    let output = emit_dts(
        r#"
    export function makeRecord<T, K extends string>(obj: { [P in K]: T }) {
        return obj;
    }

    export function makeDictionary<T>(obj: { [x: string]: T }) {
        return obj;
    }

    export function makeRecordRenamed<Value, Key extends string>(obj: { [X in Key]: Value }) {
        return obj;
    }
    "#,
    );

    assert!(
        output.contains("): { [P in K]: T; };"),
        "Expected mapped parameter annotation to drive inferred return type: {output}"
    );
    assert!(
        output.contains("): {\n    [x: string]: T;\n};"),
        "Expected index signature parameter annotation to keep object return layout: {output}"
    );
    assert!(
        output.contains("): { [X in Key]: Value; };"),
        "Expected renamed mapped variables to use the same return annotation rule: {output}"
    );
}

#[test]
fn generic_call_returned_function_object_widens_callback_literals() {
    let output = emit_dts_with_binding(
        r#"
type Func<Value> = (...args: any[]) => Value;
type Spec<Shape> = {
    [Field in keyof Shape]: Func<Shape[Field]> | Spec<Shape[Field]>;
};
declare function applySpec<Shape>(obj: Spec<Shape>): (...args: any[]) => Shape;

export var g1 = applySpec({
    sum: (a: any) => 3,
    nested: {
        mul: (b: any) => "n"
    }
});

type Rule<Result> = {
    [Name in keyof Result]: (() => Result[Name]) | Rule<Result[Name]>;
};
declare function makeRuleResult<Result>(rule: Rule<Result>): () => Result;

export var g2 = makeRuleResult({
    flag: () => true,
    child: {
        text: () => "ok"
    }
});
"#,
    );

    assert!(
        output.contains(
            "export declare var g1: (...args: any[]) => {\n    sum: number;\n    nested: {\n        mul: string;\n    };\n};"
        ),
        "Expected callback literal returns to widen inside returned function object: {output}"
    );
    assert!(
        output.contains(
            "export declare var g2: () => {\n    flag: boolean;\n    child: {\n        text: string;\n    };\n};"
        ),
        "Expected renamed generic/mapped callback spec to use the same source-call rule: {output}"
    );
}

#[test]
fn generic_call_returned_function_object_requires_callback_leaves() {
    let output = emit_dts(
        r#"
type Spec<Shape> = {
    [Field in keyof Shape]: (() => Shape[Field]) | Spec<Shape[Field]>;
};
declare function applySpec<Shape>(obj: Spec<Shape>): () => Shape;

export var g = applySpec({
    value: 3
});
"#,
    );

    assert!(
        !output.contains("export declare var g: () => {\n    value: number;\n};"),
        "Non-callback leaves should fall back to the normal inferred call surface: {output}"
    );
}

#[test]
fn generic_call_pick_mapped_arguments_preserve_public_inference_surface() {
    let output = emit_dts_with_binding(
        r#"
type Pick<T, K extends keyof T> = {
    [P in K]: T[P];
};
type Box<T> = {
    value: T;
};
type Boxified<T> = {
    [P in keyof T]: Box<T[P]>;
};
declare function f20<T, K extends keyof T>(obj: Pick<T, K>): T;
declare function f21<T, K extends keyof T>(obj: Pick<T, K>): K;
declare function f22<T, K extends keyof T>(obj: Boxified<Pick<T, K>>): T;
declare function f24<T, U, K extends keyof T | keyof U>(obj: Pick<T & U, K>): T & U;

let x0 = f20({ foo: 42, bar: "hello" });
let x1 = f21({ foo: 42, bar: "hello" });
let x2 = f22({ foo: { value: 42 }, bar: { value: "hello" } });
let x4 = f24({ foo: 42, bar: "hello" });

function getProps<T, K extends keyof T>(obj: T, list: K[]): Pick<T, K> {
    return {} as any;
}
const myAny: any = {};
const o1 = getProps(myAny, ["foo", "bar"]);
"#,
    );

    assert!(
        output.contains("declare let x0: {\n    foo: number;\n    bar: string;\n};"),
        "Expected Pick<T, K> object argument to infer the returned object surface: {output}"
    );
    assert!(
        output.contains("declare let x1: \"foo\" | \"bar\";"),
        "Expected Pick<T, K> object keys to infer K as a literal-key union: {output}"
    );
    assert!(
        output.contains("declare let x2: {\n    foo: number;\n    bar: string;\n};"),
        "Expected mapped wrapper over Pick<T, K> to unwrap one-property member values: {output}"
    );
    assert!(
        output.contains(
            "declare let x4: {\n    foo: number;\n    bar: string;\n} & {\n    foo: number;\n    bar: string;\n};"
        ),
        "Expected Pick<T & U, K> to preserve the intersection return surface: {output}"
    );
    assert!(
        output.contains("declare const o1: Pick<any, \"foo\" | \"bar\">;"),
        "Expected K[] literal argument to preserve Pick<any, literal-key-union>: {output}"
    );
}

#[test]
fn generic_call_non_mapped_wrapper_argument_does_not_infer_object_value_map() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Wrapper<V> = { value: V };
type Options<S> = { computed?: Wrapper<S> };
declare function make<S>(options: Options<S>): S;

const result = make({
    computed: {
        total(): number {
            return 1;
        },
        label: {
            get() {
                return "ready";
            }
        }
    }
});
"#,
    );

    assert!(
        output.contains("declare const result:"),
        "Expected the call result declaration to be emitted: {output}"
    );
    assert!(
        !output.contains("declare const result: {\n    total: number;\n    label: string;\n};"),
        "Non-mapped wrapper aliases must not infer object value maps from argument shape: {output}"
    );
}

#[test]
fn construct_signature_non_mapped_wrapper_argument_does_not_infer_object_value_map() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Wrapper<V> = { value: V };
type Options<S> = { computed?: Wrapper<S> };
declare const Ctor: new <S>(options: Options<S>) => S;

const result = new Ctor({
    computed: {
        total(): number {
            return 1;
        },
        label: {
            get() {
                return "ready";
            }
        }
    }
});
"#,
    );

    assert!(
        output.contains("declare const result:"),
        "Expected the construct result declaration to be emitted: {output}"
    );
    assert!(
        !output.contains("declare const result: {\n    total: number;\n    label: string;\n};"),
        "Construct signatures must not infer object value maps through non-mapped wrapper aliases: {output}"
    );
}

#[test]
fn generic_call_constrained_mapped_return_uses_concrete_constraint_surface() {
    let output = emit_dts_with_binding(
        r#"
declare function f1<T1>(): { [P in keyof T1]: void };
declare function f2<T1 extends string>(): { [P in keyof T1]: void };
declare function f3<T1 extends number>(): { [P in keyof T1]: void };
interface Number {
    toString(): string;
    toFixed(): string;
    toExponential(): string;
    toPrecision(): string;
    valueOf(): number;
    toLocaleString(): string;
}
declare function f4<T1 extends Number>(): { [P in keyof T1]: void };
interface WrappedValue {
    alpha(): string;
    beta: number;
}
declare function f5<Value extends WrappedValue>(): { [Key in keyof Value]: boolean };

let x1 = f1();
let x2 = f2();
let x3 = f3();
let x4 = f4();
let x5 = f5();
"#,
    );

    assert!(
        output.contains("declare let x2: string;"),
        "Expected string-constrained mapped call result to expand to string: {output}"
    );
    assert!(
        output.contains("declare let x3: number;"),
        "Expected number-constrained mapped call result to expand to number: {output}"
    );
    assert!(
        output.contains(
            "declare let x4: {\n    toString: void;\n    toFixed: void;\n    toExponential: void;\n    toPrecision: void;\n    valueOf: void;\n    toLocaleString: void;\n};"
        ),
        "Expected object-wrapper mapped call result to expand public members: {output}"
    );
    assert!(
        output.contains("declare let x5: {\n    alpha: boolean;\n    beta: boolean;\n};"),
        "Expected renamed wrapper mapped call result to expand declared public members: {output}"
    );
}

#[test]
fn test_variadic_tuple_call_return_materializes_prefix_or_constraint() {
    let output = emit_dts_with_binding(
        r#"
    declare function collect<Items extends readonly [string, ...string[]]>(
        ...values: readonly [...Items, number]
    ): [...Items, number];

    export const one = collect("first", 1);
    export const two = collect("first", "second", 1);
    export const fallback = collect(1, 2);
    "#,
    );

    assert!(
        output.contains("export declare const one: [\"first\", number];"),
        "Expected single literal prefix to materialize into the variadic tuple return: {output}"
    );
    assert!(
        output.contains("export declare const two: [\"first\", \"second\", number];"),
        "Expected multiple literal prefixes to materialize into the variadic tuple return: {output}"
    );
    assert!(
        output.contains("export declare const fallback: [string, ...string[], number];"),
        "Expected invalid or unmaterializable prefixes to fall back to the tuple constraint: {output}"
    );
}

#[test]
fn test_variadic_tuple_call_return_escapes_string_literal_prefixes() {
    // The materialized prefix literal must carry TypeScript-compatible
    // string-literal escaping derived from the cooked literal value, not from
    // trimming the source token and re-wrapping it in quotes. The reported
    // shapes that the source-slice approach mangled:
    //   - a single-quoted string with an embedded double quote,
    //   - a no-substitution template literal (backticks are not quote chars),
    //   - a double-quoted string containing a backslash.
    let output = emit_dts_with_binding(
        r#"
    declare function collect<Items extends readonly [string, ...string[]]>(
        ...values: readonly [...Items, number]
    ): [...Items, number];

    export const sq = collect('a"b', 1);
    export const tmpl = collect(`c"d`, 1);
    export const bs = collect("e\\f", 1);
    "#,
    );

    assert!(
        output.contains(r#"export declare const sq: ["a\"b", number];"#),
        "Single-quoted prefix with an embedded double quote must be re-escaped: {output}"
    );
    assert!(
        output.contains(r#"export declare const tmpl: ["c\"d", number];"#),
        "No-substitution template prefix must emit an escaped double-quoted literal: {output}"
    );
    assert!(
        output.contains(r#"export declare const bs: ["e\\f", number];"#),
        "Prefix containing a backslash must preserve the escaped backslash: {output}"
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
        let atom = interner.intern_string(&name);
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

fn emit_dts_with_method_node_return_type(
    source: &str,
    method_name_str: &str,
    return_type_id: TypeId,
) -> String {
    use tsz_parser::parser::syntax_kind_ext;

    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
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
            (parser.arena.get_identifier_text(decl.name)? == method_name_str).then_some(nidx)
        })
        .expect("method not found");

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

fn emit_dts_with_function_return_type(
    source: &str,
    function_name_str: &str,
    return_type_id: TypeId,
) -> String {
    use tsz_parser::parser::syntax_kind_ext;

    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let func_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                return None;
            }
            let nidx = NodeIndex(idx as u32);
            let func = parser.arena.get_function(parser.arena.get(nidx)?)?;
            (parser.arena.get_identifier_text(func.name)? == function_name_str).then_some(nidx)
        })
        .expect("function not found");

    let func_type = interner.function(FunctionShape::new(Vec::new(), return_type_id));
    let mut node_types = FxHashMap::default();
    node_types.insert(func_idx.0, func_type);

    let type_cache = crate::type_cache_view::TypeCacheView {
        node_types,
        ..Default::default()
    };

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.emit(root)
}

fn preferred_function_body_return_text(source: &str, function_name_str: &str) -> Option<String> {
    use tsz_parser::parser::syntax_kind_ext;

    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();

    let function_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            if node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                return None;
            }
            let nidx = NodeIndex(idx as u32);
            let func = parser.arena.get_function(parser.arena.get(nidx)?)?;
            (parser.arena.get_identifier_text(func.name)? == function_name_str).then_some(nidx)
        })?;
    let func = parser
        .arena
        .get(function_idx)
        .and_then(|node| parser.arena.get_function(node))?;
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.function_body_preferred_return_type_text(func.body)
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

#[test]
fn test_unannotated_method_indexed_access_return_preserves_declared_receiver_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
class Shape {
    name: string;
    width: number;
    height: number;
}
export class Reader {
    readShape<K extends keyof Shape>(shape: Shape, key: K) {
        return shape[key];
    }
}
"#,
        "readShape",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("readShape<K extends keyof Shape>(shape: Shape, key: K): Shape[K]"),
        "Expected indexed-access return surface from declared receiver: {output}"
    );
}

#[test]
fn test_unannotated_method_indexed_access_return_preserves_renamed_key_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
type Store = {
    title: string;
    count: number;
};
export class Reader {
    readStore<TKey extends keyof Store>(store: Store, key: TKey) {
        return store[key];
    }
}
"#,
        "readStore",
        TypeId::STRING,
    );
    assert!(
        output
            .contains("readStore<TKey extends keyof Store>(store: Store, key: TKey): Store[TKey]"),
        "Expected indexed-access return surface with renamed key type: {output}"
    );
}

#[test]
fn test_unannotated_this_indexed_access_return_preserves_this_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
export class Bag {
    value: number;
    get<TKey extends keyof this>(key: TKey) {
        return this[key];
    }
}
"#,
        "get",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("get<TKey extends keyof this>(key: TKey): this[TKey]"),
        "Expected this-indexed return surface: {output}"
    );
}

#[test]
fn test_unannotated_indexed_access_return_preserves_array_element_key_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
type Store<K extends string> = { [P in K]: 0 | 1 };
export class Reader {
    read<K extends string>(store: Store<K>, keys: K[]) {
        return store[keys[0]];
    }
}
"#,
        "read",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("read<K extends string>(store: Store<K>, keys: K[]): Store<K>[K]"),
        "Expected indexed-access return surface from array element key: {output}"
    );
}

#[test]
fn test_unannotated_function_indexed_access_return_preserves_array_element_key_surface() {
    let output = emit_dts_with_function_return_type(
        r#"
type Store<K extends string> = { [P in K]: 0 | 1 };
export function read<K extends string>(store: Store<K>, keys: K[]) {
    return store[keys[0]];
}
"#,
        "read",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("read<K extends string>(store: Store<K>, keys: K[]): Store<K>[K]"),
        "Expected top-level indexed-access return surface from array element key: {output}"
    );
}

#[test]
fn test_unannotated_function_returning_local_indexed_helper_call_preserves_surface() {
    let source = r#"
interface Shape {
    name: string;
    width: number;
    height: number;
    visible: boolean;
}
function getProperty<T, K extends keyof T>(obj: T, key: K) {
    return obj[key];
}
export function read<S extends Shape, K extends keyof S>(shape: S, key: K) {
    let prop = getProperty(shape, key);
    return prop;
}
"#;
    assert_eq!(
        preferred_function_body_return_text(source, "read").as_deref(),
        Some("S[K]")
    );
    let output = emit_dts_with_function_return_type(source, "read", TypeId::NUMBER);
    assert!(
        output.contains("read<S extends Shape, K extends keyof S>(shape: S, key: K): S[K]"),
        "Expected local helper-call indexed-access surface: {output}"
    );
}

#[test]
fn test_explicit_type_argument_indexed_member_call_result_uses_member_type() {
    let output = emit_dts_with_binding(
        r#"
type MethodDescriptor = {
    name: string;
    args: any[];
    returnValue: any;
};
declare function dispatchMethod<M extends MethodDescriptor>(
    name: M["name"],
    args: M["args"]
): M["returnValue"];
type StringMethodDescriptor = {
    name: "stringMethod";
    args: [string, number];
    returnValue: string[];
};
let result = dispatchMethod<StringMethodDescriptor>("stringMethod", ["hello", 35]);
"#,
    );
    assert!(
        output.contains("declare let result: string[]"),
        "Expected explicit type argument indexed member return to evaluate to member type: {output}"
    );
}

#[test]
fn test_unannotated_method_returning_indexed_helper_call_preserves_this_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
function read<TObj, TKey extends keyof TObj>(obj: TObj, key: TKey) {
    return obj[key];
}
export class Person {
    parts: number;
    getParts() {
        return read(this, "parts");
    }
}
"#,
        "getParts",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("getParts(): this[\"parts\"]"),
        "Expected helper call indexed-access surface: {output}"
    );
}

#[test]
fn test_unannotated_method_returning_inherited_this_indexed_method_call_preserves_surface() {
    let output = emit_dts_with_method_node_return_type(
        r#"
export class Base {
    get<TKey extends keyof this>(key: TKey) {
        return this[key];
    }
}
export class Person extends Base {
    parts: number;
    getParts() {
        return this.get("parts");
    }
}
"#,
        "getParts",
        TypeId::NUMBER,
    );
    assert!(
        output.contains("getParts(): this[\"parts\"]"),
        "Expected inherited this-indexed method call surface: {output}"
    );
}
