use super::*;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn test_destructuring_variable_declaration_groups_typed_bindings() {
    let source = r#"var [x, y] = [1, "hello"];"#;
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let stmt = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing variable statement");
    let decl_list = parser
        .arena
        .get(stmt.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable(node))
        .expect("missing declaration list");
    let decl = parser
        .arena
        .get(decl_list.declarations.nodes[0])
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing declaration");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare var x: number, y: string;"),
        "Expected destructured bindings to emit in one typed declaration: {output}"
    );
}

#[test]
fn test_string_literal_tuple_binding_preserves_alias_and_return_union() {
    let output = emit_dts(
        r#"
type RexOrRaptor = "t-rex" | "raptor";
let [im, a, dinosaur]: ["I'm", "a", RexOrRaptor] = ["I'm", "a", "t-rex"];

function rawr(dino: RexOrRaptor) {
    if (dino === "t-rex") {
        return "ROAAAAR!";
    }
    if (dino === "raptor") {
        return "yip yip!";
    }
    throw "Unexpected " + dino;
}
"#,
    );

    assert!(
        output.contains("declare let im: \"I'm\", a: \"a\", dinosaur: RexOrRaptor;"),
        "Expected tuple destructuring to preserve the alias from the source tuple annotation: {output}"
    );
    assert!(
        output.contains("declare function rawr(dino: RexOrRaptor): \"ROAAAAR!\" | \"yip yip!\";"),
        "Expected string literal returns from guarded branches to emit as a union: {output}"
    );
}

#[test]
fn isomorphic_mapped_call_infers_variadic_tuple_return_from_argument() {
    let output = emit_dts_with_binding(
        r#"
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare function unboxify<T>(x: Boxified<T>): T;
declare let x10: [Box<number>, Box<string>, ...Box<boolean>[]];
let y10 = unboxify(x10);
"#,
    );

    assert!(
        output.contains("declare let y10: [number, string, ...boolean[]];"),
        "Expected mapped tuple call to recover the public tuple return: {output}"
    );
}

#[test]
fn test_mutable_array_literal_binding_widens_homogeneous_literals() {
    let source = r#"
let [hello, brave] = ["Hello", "Brave"];
let [one, two] = [1, 2];
let [yes, no] = [true, false];
export let [ma, mb] = ["A", 1];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_types = [
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_string("Hello"),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_string("Brave"),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_number(1.0),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_number(2.0),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_boolean(true),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_boolean(false),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
        interner.tuple(vec![
            TupleElement {
                type_id: interner.literal_string("A"),
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: interner.literal_number(1.0),
                name: None,
                optional: false,
                rest: false,
            },
        ]),
    ];

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    for (decl_idx, tuple_type) in variable_declarations_from_source(&parser, root)
        .into_iter()
        .zip(tuple_types)
    {
        let decl = parser
            .arena
            .get(decl_idx)
            .and_then(|node| parser.arena.get_variable_declaration(node))
            .expect("missing variable declaration");
        type_cache.node_types.insert(decl.initializer.0, tuple_type);
    }

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare let hello: string, brave: string;"),
        "Expected mutable string array binding literals to widen: {output}"
    );
    assert!(
        output.contains("declare let one: number, two: number;"),
        "Expected mutable number array binding literals to widen: {output}"
    );
    assert!(
        output.contains("declare let yes: boolean, no: boolean;"),
        "Expected mutable boolean array binding literals to widen: {output}"
    );
    assert!(
        output.contains("export declare let ma: string, mb: number;"),
        "Expected mutable mixed array binding literals to widen per binding: {output}"
    );
}

#[test]
fn test_short_circuit_const_literal_variables_preserve_literal_union() {
    let output = emit_dts_with_binding(
        r#"
const string: "string" = "string";
const number: "number" = "number";
const boolean: "boolean" = "boolean";

const stringOrNumber = string || number;
const stringOrBoolean = string || boolean;
const booleanOrNumber = number || boolean;
const stringOrBooleanOrNumber = stringOrBoolean || number;
"#,
    );

    assert!(
        output.contains("declare const stringOrNumber: \"string\" | \"number\";"),
        "Expected `||` over literal-typed identifiers to preserve the source union: {output}"
    );
    assert!(
        output.contains("declare const stringOrBoolean: \"string\" | \"boolean\";"),
        "Expected `||` over literal-typed identifiers to include the right operand: {output}"
    );
    assert!(
        output.contains("declare const booleanOrNumber: \"number\" | \"boolean\";"),
        "Expected `||` over literal-typed identifiers to preserve numeric and boolean arms: {output}"
    );
    assert!(
        output.contains(
            "declare const stringOrBooleanOrNumber: \"string\" | \"number\" | \"boolean\";"
        ),
        "Expected chained `||` over literal-typed identifiers to preserve all source arms: {output}"
    );
}

#[test]
fn test_short_circuit_drops_falsy_left_literal_from_dts_union() {
    let output = emit_dts_with_binding(
        r#"
const empty: "" = "";
const fallback: "fallback" = "fallback";
const value = empty || fallback;
"#,
    );

    assert!(
        output.contains("declare const value: \"fallback\";"),
        "Expected `||` declaration inference to exclude a known-falsy left literal: {output}"
    );
}

#[test]
fn test_short_circuit_keeps_fallback_when_left_union_can_be_falsy() {
    let output = emit_dts_with_binding(
        r#"
const maybe: "" | "value" = "" as any;
const fallback: "fallback" = "fallback";
const value = maybe || fallback;
"#,
    );

    assert!(
        output.contains("declare const value: \"value\" | \"fallback\";"),
        "Expected `||` declaration inference to keep fallback only when the left side can be falsy: {output}"
    );
}

#[test]
fn test_short_circuit_omits_right_operand_when_left_is_syntactically_truthy() {
    let output = emit_dts_with_binding(
        r#"
class C {}
const value = (() => new C()) || "";
"#,
    );

    assert!(
        output.contains("declare const value: () => C;"),
        "Expected declaration inference to omit unreachable `||` right operand: {output}"
    );
}

#[test]
fn test_short_circuit_keeps_uncovered_fallback_for_broad_falsy_primitives() {
    let output = emit_dts_with_binding(
        r#"
export let s: string;
export let n: number;
export let b: boolean;
export const stringOrNumber = s || 1;
export const stringOrString = s || "fallback";
export const numberOrString = n || "fallback";
export const booleanOrString = b || "fallback";
"#,
    );

    assert!(
        output.contains("export declare const stringOrNumber: string | number;"),
        "Expected broad string left operand to keep an uncovered number fallback: {output}"
    );
    assert!(
        output.contains("export declare const stringOrString: string;"),
        "Expected broad string left operand to cover string literal fallback: {output}"
    );
    assert!(
        output.contains("export declare const numberOrString: number | string;"),
        "Expected broad number left operand to keep an uncovered string fallback: {output}"
    );
    assert!(
        output.contains("export declare const booleanOrString: true | string;"),
        "Expected broad boolean left operand to narrow to true and keep fallback: {output}"
    );
}

#[test]
fn test_short_circuit_numeric_and_bigint_truthiness_matches_tsc_dts_surface() {
    let output = emit_dts_with_binding(
        r#"
const zero = 0 || "";
const one = 1 || "";
const zeroBig = 0n || "";
const oneBig = 1n || "";
"#,
    );

    assert!(
        output.contains("declare const zero: \"\";"),
        "Expected numeric zero left operand to expose the fallback literal: {output}"
    );
    assert!(
        output.contains("declare const one: 1;"),
        "Expected non-zero numeric left operand to omit unreachable fallback: {output}"
    );
    assert!(
        output.contains("declare const zeroBig: \"\";"),
        "Expected bigint zero left operand to expose the fallback literal: {output}"
    );
    assert!(
        output.contains("declare const oneBig: 1n;"),
        "Expected non-zero bigint left operand to omit unreachable fallback: {output}"
    );
}

#[test]
fn test_short_circuit_drops_right_for_truthy_function_expression() {
    let output = emit_dts_with_binding(
        r#"
class C { private p: string; }
var l = (() => new C()) || "";
var m = (function () { return new C(); }) || "";
"#,
    );

    assert!(
        output.contains("declare var l: () => C;"),
        "Expected `||` with a parenthesized arrow left operand to keep only the function type: {output}"
    );
    assert!(
        output.contains("declare var m: () => C;"),
        "Expected `||` with a parenthesized function-expression left operand to keep only the function type: {output}"
    );
    assert!(
        !output.contains("declare var l: (() => C) | string;")
            && !output.contains("declare var m: (() => C) | string;"),
        "Definitely-truthy function expressions should not union in the unreachable right operand: {output}"
    );
}

#[test]
fn test_short_circuit_reference_respects_annotated_widened_surface() {
    let output = emit_dts_with_binding(
        r#"
const a: "a" = "a";
const b: "b" = "b";
const c: "c" = "c";
let ab: string = a || b;
export const y = ab || c;
"#,
    );

    assert!(
        output.contains("export declare const y: string;"),
        "Expected referenced annotated short-circuit declarations to keep their reachable declared surface: {output}"
    );
    assert!(
        !output.contains("export declare const y: \"a\" | \"b\" | \"c\";"),
        "Annotated referenced declarations must not be expanded through their initializer: {output}"
    );
}

#[test]
fn test_nullish_coalescing_drops_nullish_left_from_dts_union() {
    let output = emit_dts_with_binding(
        r#"
const maybe: "value" | undefined = undefined as any;
const fallback: "fallback" = "fallback";
const value = maybe ?? fallback;
"#,
    );

    assert!(
        output.contains("declare const value: \"value\" | \"fallback\";"),
        "Expected `??` declaration inference to remove nullish left arms and keep fallback: {output}"
    );
}

#[test]
fn test_instantiated_short_circuit_function_union_deduplicates_identical_members() {
    let output = emit_dts_with_binding(
        r#"
declare let g: (<T>(x: T) => T) | undefined;

const orValue = g<string> || ((x: string) => x);
const nullishValue = g<string> ?? ((x: string) => x);
"#,
    );

    assert!(
        output.contains("declare const orValue: (x: string) => string;"),
        "Expected `||` over identical instantiated function surfaces to collapse to one signature: {output}"
    );
    assert!(
        output.contains("declare const nullishValue: (x: string) => string;"),
        "Expected `??` over identical instantiated function surfaces to collapse to one signature: {output}"
    );
    assert!(
        !output.contains("((x: string) => string) | ((x: string) => string)"),
        "Duplicate rendered function signatures should not remain in DTS unions: {output}"
    );
}

#[test]
fn test_const_asserted_array_literal_binding_preserves_literals() {
    let source = r#"let [hello, brave] = ["Hello", "Brave"] as const;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: interner.literal_string("Hello"),
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: interner.literal_string("Brave"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let decl_idx = variable_declarations_from_source(&parser, root)
        .into_iter()
        .next()
        .expect("missing variable declaration");
    let decl = parser
        .arena
        .get(decl_idx)
        .and_then(|node| parser.arena.get_variable_declaration(node))
        .expect("missing variable declaration");
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(decl.initializer.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("declare let hello: \"Hello\", brave: \"Brave\";"),
        "Expected const-asserted array binding literals to stay literal: {output}"
    );
}

fn variable_declarations_from_source(parser: &ParserState, root: NodeIndex) -> Vec<NodeIndex> {
    let root_node = parser.arena.get(root).expect("missing root node");
    let source_file = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file");
    let mut declarations = Vec::new();
    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = parser.arena.get(stmt_idx) else {
            continue;
        };
        let variable_stmt_idx =
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION {
                parser
                    .arena
                    .get_export_decl(stmt_node)
                    .map(|export| export.export_clause)
                    .unwrap_or(stmt_idx)
            } else {
                stmt_idx
            };
        let Some(stmt) = parser
            .arena
            .get(variable_stmt_idx)
            .and_then(|node| parser.arena.get_variable(node))
        else {
            continue;
        };
        for &decl_list_idx in &stmt.declarations.nodes {
            let Some(decl_list) = parser
                .arena
                .get(decl_list_idx)
                .and_then(|node| parser.arena.get_variable(node))
            else {
                continue;
            };
            declarations.extend(decl_list.declarations.nodes.iter().copied());
        }
    }
    declarations
}

#[test]
fn test_destructured_parameter_with_defaulted_property_uses_multiline_object_type() {
    let output = emit_dts("const k = ({ x: z = 'y' }) => {};");
    assert!(
        output.contains("declare const k: ({ x: z }: {\n    x?: string;\n}) => void;"),
        "Expected defaulted object binding parameter to emit a multiline object type: {output}"
    );
}

#[test]
fn test_destructured_parameter_defaulting_from_any_emits_any() {
    let output = emit_dts("var a; function f({ p: {} = a } = a) {}");
    assert!(
        output.contains("declare function f({ p: {} }?: any): void;"),
        "Expected destructured parameter defaulting from any to emit any: {output}"
    );
}

#[test]
fn test_returned_function_expression_preserves_destructured_typeof_alias_parameter() {
    let output = emit_dts(
        "type Named = { name: string }; function f({ name: alias }: Named) { return function(p: typeof alias) {} }",
    );
    assert!(
        output.contains("declare function f({ name: alias }: Named): (p: typeof alias) => void;"),
        "Expected returned function expression parameter to preserve typeof alias: {output}"
    );
}

#[test]
fn test_method_returning_non_null_null_widens_to_any() {
    let output = emit_dts(
        "type Named = { name: string }; class C { m({ name: alias }: Named, p: typeof alias) { return null!; } }",
    );
    assert!(
        output.contains("m({ name: alias }: Named, p: typeof alias): any;"),
        "Expected null! method return to emit any: {output}"
    );
}

#[test]
fn test_inferred_object_return_preserves_destructured_typeof_alias_member() {
    let output = emit_dts_with_binding(
        "type Named = { name: string }; function f({ name: alias }: Named) { type Named2 = { name: typeof alias }; return null! as Named2; }",
    );
    assert!(
        output
            .contains("declare function f({ name: alias }: Named): {\n    name: typeof alias;\n};"),
        "Expected asserted local alias return to preserve typeof destructured alias: {output}"
    );
}

#[test]
fn test_destructuring_parameter_properties_emit_individual_class_properties() {
    let source = "class C { constructor(public [x, y]: [string, number]) {} }";
    let (parser, root) = parse_test_source(source);
    let root_node = parser.arena.get(root).expect("missing root node");
    let stmt_idx = parser
        .arena
        .get_source_file(root_node)
        .expect("missing source file")
        .statements
        .nodes[0];
    let class_decl = parser
        .arena
        .get(stmt_idx)
        .and_then(|node| parser.arena.get_class(node))
        .expect("missing class declaration");
    let ctor_idx = class_decl.members.nodes[0];
    let ctor = parser
        .arena
        .get(ctor_idx)
        .and_then(|node| parser.arena.get_constructor(node))
        .expect("missing constructor");
    let param_idx = ctor.parameters.nodes[0];
    let param = parser
        .arena
        .get(param_idx)
        .and_then(|node| parser.arena.get_parameter(node))
        .expect("missing parameter");

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache
        .node_types
        .insert(param.type_annotation.0, tuple_type);

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("x: string;"),
        "Expected first destructured parameter property to be emitted: {output}"
    );
    assert!(
        output.contains("y: number;"),
        "Expected second destructured parameter property to be emitted: {output}"
    );
    assert!(
        !output.contains("[x, y]: [string, number];"),
        "Did not expect destructuring pattern to be emitted as a property name: {output}"
    );
}

// =============================================================================
// Method return type inference from arithmetic body expressions
// =============================================================================

#[test]
fn method_return_type_inferred_from_addition_of_number_properties() {
    let output = emit_dts_with_binding(
        r#"
class Calculator {
    public x: number;
    public add(b: number) {
        return this.x + b;
    }
}
"#,
    );
    assert!(
        output.contains("add(b: number): number;"),
        "Expected method return type to be inferred as number from this.x + b: {output}"
    );
}

#[test]
fn method_body_return_inference_survives_non_callable_cached_method_type() {
    let source = r#"
class Boxed {
    values() {
        return new Boxed();
    }
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let method_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::METHOD_DECLARATION)
                .then_some(NodeIndex(idx as u32))
                .filter(|&method_idx| {
                    parser
                        .arena
                        .get(method_idx)
                        .and_then(|node| parser.arena.get_method_decl(node))
                        .and_then(|method| parser.arena.get_identifier_text(method.name))
                        == Some("values")
                })
        })
        .expect("missing values method");

    let interner = TypeInterner::new();
    let mut type_cache = crate::type_cache_view::TypeCacheView::default();
    type_cache.node_types.insert(method_idx.0, TypeId::UNKNOWN);
    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let output = emitter.emit(root);

    assert!(
        output.contains("values(): Boxed;"),
        "Expected body inference to recover the method return type when cache is non-callable: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_subtraction() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public sub(b: number) {
        return this.value - b;
    }
}
"#,
    );
    assert!(
        output.contains("sub(b: number): number;"),
        "Expected method return type to be inferred as number from subtraction: {output}"
    );
}

#[test]
fn method_return_type_inferred_from_multiplication() {
    let output = emit_dts_with_binding(
        r#"
class Calc {
    public value: number;
    public mul(b: number) {
        return this.value * b;
    }
}
"#,
    );
    assert!(
        output.contains("mul(b: number): number;"),
        "Expected method return type to be inferred as number from multiplication: {output}"
    );
}

#[test]
fn static_method_return_type_inferred_from_addition() {
    let output = emit_dts_with_binding(
        r#"
class C {
    static s1: number;
    static add(b: number) {
        return C.s1 + b;
    }
}
"#,
    );
    assert!(
        output.contains("static add(b: number): number;"),
        "Expected static method return type to be inferred as number: {output}"
    );
}

#[test]
fn method_return_type_string_concatenation() {
    let output = emit_dts_with_binding(
        r#"
class Greeter {
    public name: string;
    public greet() {
        return "Hello, " + this.name;
    }
}
"#,
    );
    assert!(
        output.contains("greet(): string;"),
        "Expected method return type to be inferred as string from string concatenation: {output}"
    );
}

#[test]
fn reference_declared_type_annotation_resolves_property_declarations() {
    let output = emit_dts_with_binding(
        r#"
class Foo {
    public x: number;
    public getX() {
        return this.x;
    }
}
"#,
    );
    assert!(
        output.contains("getX(): number;"),
        "Expected method returning this.x to be inferred as number: {output}"
    );
}

#[test]
fn js_method_return_type_uses_constructor_assignment_property_facts() {
    let output = emit_js_dts_with_usage_analysis(
        r#"
export class Counted {
    constructor() {
        this.total = 12;
        this.label = "ok";
    }
    value() {
        return this.total;
    }
    describe() {
        return "value: " + this.label;
    }
}

export class Renamed {
    constructor(seed) {
        this.amount = seed;
    }
    current() {
        return this.amount;
    }
}
"#,
    );

    assert!(
        output.contains("value(): number;"),
        "Expected JS method return to reuse constructor-assigned property facts: {output}"
    );
    assert!(
        output.contains("describe(): string;"),
        "Expected composed JS method return to reuse constructor-assigned property facts: {output}"
    );
    assert!(
        output.contains("current(): any;"),
        "Expected unsupported untyped constructor parameter assignments to preserve fallback behavior: {output}"
    );
}

#[test]
fn method_return_type_bitwise_operations_produce_number() {
    let output = emit_dts_with_binding(
        r#"
class BitOps {
    public a: number;
    public shiftLeft(n: number) {
        return this.a << n;
    }
    public bitwiseAnd(n: number) {
        return this.a & n;
    }
}
"#,
    );
    assert!(
        output.contains("shiftLeft(n: number): number;"),
        "Expected shift left to return number: {output}"
    );
    assert!(
        output.contains("bitwiseAnd(n: number): number;"),
        "Expected bitwise and to return number: {output}"
    );
}

// =============================================================================
// 23. Import-clause fallback heuristics (no usage tracking, e.g. --noCheck --noLib)
// =============================================================================

// Regression for #3337: with `--noCheck --noLib --declaration --emitDeclarationOnly`
// tsz dropped a regular default import even though the emitted `.d.ts` referenced
// the imported binding as a type. The fallback path must keep default imports for
// the same reason it keeps named imports — they may resolve a type reference in
// the declaration output.
#[test]
fn default_import_is_preserved_in_dts_fallback_when_used_as_type() {
    let output = emit_dts(
        r#"
import Foo from "./dep";
export let x: Foo;
"#,
    );

    assert!(
        output.contains(r#"import Foo from "./dep";"#),
        "Expected default import to be preserved in fallback dts emit: {output}"
    );
    assert!(
        output.contains("export declare let x: Foo;"),
        "Expected exported let to keep its `Foo` type annotation: {output}"
    );
}

#[test]
fn default_import_fallback_preserves_combined_default_and_named() {
    let output = emit_dts(
        r#"
import Foo, { Bar } from "./dep";
export let x: Foo;
export let y: Bar;
"#,
    );

    assert!(
        output.contains("Foo") && output.contains("Bar") && output.contains(r#""./dep""#),
        "Expected combined default + named imports to be preserved in fallback dts emit: {output}"
    );
}

#[test]
fn type_only_default_import_still_preserved_in_fallback() {
    let output = emit_dts(
        r#"
import type Foo from "./dep";
export let x: Foo;
"#,
    );

    assert!(
        output.contains("Foo") && output.contains(r#""./dep""#),
        "Expected type-only default import to still be preserved: {output}"
    );
}

#[test]
fn type_only_namespace_import_preserved_in_dts_fallback() {
    let output = emit_dts(
        r#"
import type * as ns from "./dep";
export interface Foo {
    x: string;
}
export type T = ns.Foo;
"#,
    );

    assert!(
        output.contains(r#"import type * as ns from "./dep";"#),
        "Expected type-only namespace import to be preserved: {output}"
    );
    assert!(
        output.contains("export type T = ns.Foo;"),
        "Expected exported type to reference preserved namespace import: {output}"
    );
}

#[test]
fn value_only_ambient_dependency_from_exported_initializer_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export const out: number = t;
"#,
    );

    assert!(
        !output.contains("declare const t"),
        "Did not expect ambient initializer-only dependency to leak: {output}"
    );
    assert!(
        output.contains("export declare const out: number;"),
        "Expected exported declaration to remain: {output}"
    );
}

#[test]
fn ambient_value_dependency_used_in_exported_type_query_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export type T = typeof t;
"#,
    );

    assert!(
        output.contains("declare const t: number;"),
        "Expected ambient value referenced by exported typeof to remain: {output}"
    );
    assert!(
        output.contains("export type T = typeof t;"),
        "Expected exported type query to remain: {output}"
    );
}

#[test]
fn ambient_value_dependency_exported_by_specifier_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare const t: number;
export { t };
"#,
    );

    assert!(
        output.contains("declare const t: number;"),
        "Expected ambient value exported by specifier to remain: {output}"
    );
    assert!(
        output.contains("export { t };"),
        "Expected export specifier to remain: {output}"
    );
}
