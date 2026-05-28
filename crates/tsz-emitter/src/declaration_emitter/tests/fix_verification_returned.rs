use super::*;

// Tail shard split from fix_verification.rs to keep test files under 2000 lines.

#[test]
fn fix_short_circuit_string_literal_overload_operands_match_tsc_dts_widening() {
    let output = emit_dts_with_usage_analysis(
        r#"
const explicitString: "string" = "string";
const explicitNumber: "number" = "number";
const explicitBoolean: "boolean" = "boolean";
const explicitStringOrNumber = explicitString || explicitNumber;
const explicitStringOrBoolean = explicitString || explicitBoolean;
const explicitBooleanOrNumber = explicitNumber || explicitBoolean;
const explicitStringOrBooleanOrNumber = explicitStringOrBoolean || explicitNumber;

const inferredString = "string";
const inferredNumber = "number";
const inferredBoolean = "boolean";
const inferredStringOrNumber = inferredString || inferredNumber;
const inferredStringOrBoolean = inferredString || inferredBoolean;
const inferredBooleanOrNumber = inferredNumber || inferredBoolean;
const inferredStringOrBooleanOrNumber = inferredStringOrBoolean || inferredNumber;
"#,
    );

    for expected in [
        r#"declare const explicitStringOrNumber: "string" | "number";"#,
        r#"declare const explicitStringOrBoolean: "string" | "boolean";"#,
        r#"declare const explicitBooleanOrNumber: "number" | "boolean";"#,
        r#"declare const explicitStringOrBooleanOrNumber: "string" | "number" | "boolean";"#,
        // Inferred const literals are widened intentionally in short-circuit operand
        // position (matching the tsc DTS surface for mutable bindings built from them).
        "declare const inferredStringOrNumber: string;",
        "declare const inferredStringOrBoolean: string;",
        "declare const inferredBooleanOrNumber: string;",
        "declare const inferredStringOrBooleanOrNumber: string;",
    ] {
        assert!(
            output.contains(expected),
            "expected short-circuit operand type `{expected}`: {output}"
        );
    }
}

#[test]
fn fix_generic_rest_identity_preserves_parameters_tuple_labels() {
    let output = emit_dts_with_usage_analysis_and_parameters_lib(
        r#"
declare function f<T extends any[]>(...x: T): T;
declare function g(elem: object, index: number): object;
declare function overloaded(seed: string): string;
declare function overloaded(elem: object, index: number): object;
declare function getArgsForInjection<T extends (...args: any[]) => any>(x: T): Parameters<T>;
declare function getArgsRenamed<Fn extends (...args: any[]) => any>(x: Fn): Parameters<Fn>;
type ArgsOf<Fn extends (...args: any[]) => any> = Parameters<Fn>;
declare function getArgsAlias<Fn extends (...args: any[]) => any>(x: Fn): ArgsOf<Fn>;

export const argumentsOfGAsFirstArgument = f(getArgsForInjection(g));
export const argumentsOfG = f(...getArgsForInjection(g));
export const argumentsOfGRenamed = f(...getArgsRenamed(g));
export const argumentsOfGAlias = f(...getArgsAlias(g));
export const argumentsOfOverload = f(...getArgsForInjection(overloaded));
"#,
    );

    for expected in [
        "export declare const argumentsOfGAsFirstArgument: [[elem: object, index: number]];",
        "export declare const argumentsOfG: [elem: object, index: number];",
        "export declare const argumentsOfGRenamed: [elem: object, index: number];",
        "export declare const argumentsOfGAlias: [elem: object, index: number];",
        "export declare const argumentsOfOverload: [elem: object, index: number];",
    ] {
        assert!(
            output.contains(expected),
            "expected labeled Parameters tuple `{expected}`: {output}"
        );
    }

    let shadowed_output = emit_dts_with_usage_analysis_and_parameters_lib(
        r#"
type Parameters<T> = T;
declare function f<T extends any[]>(...x: T): T;
declare function g(elem: object, index: number): object;
declare function getArgsShadowed<Fn extends (...args: any[]) => any>(x: Fn): Parameters<Fn>;

export const argumentsOfShadowedParameters = f(...getArgsShadowed(g));
"#,
    );

    assert!(
        !shadowed_output.contains(
            "export declare const argumentsOfShadowedParameters: [elem: object, index: number];"
        ),
        "shadowed Parameters must not trigger built-in tuple recovery: {shadowed_output}"
    );
}

#[test]
fn fix_generic_call_constructor_return_object_formats_multiline() {
    let output = emit_dts(
        r#"
declare const a: symbol;
type Constructor = new (...args: any[]) => {};
declare function Mix<T extends Constructor>(
    classish: T
): T & (new (...args: any[]) => {mixed: true});

export const Mixer = Mix(class {
    [a]() { return 1 };
});
"#,
    );

    assert!(
        output.contains("): T & (new (...args: any[]) => {\n    mixed: true;\n});"),
        "constructor-arrow return object should be normalized to multiline: {output}"
    );
}

#[test]
fn fix_deferred_lookup_preserves_string_literal_key_substitution() {
    let output = emit_dts_with_binding(
        r#"
declare function f1<A extends string, B extends string>(a: A, b: B): { [P in A | B]: any };

function f2<A extends string>(a: A) {
    return f1(a, 'x');
}
"#,
    );

    assert!(
        output
            .contains(r#"declare function f2<A extends string>(a: A): { [P in A | "x"]: any; };"#),
        "expected f2 to preserve the string literal key substitution: {output}"
    );
    assert!(
        !output.contains("[P in string | string]"),
        "string-constrained literal substitution should not widen both keys: {output}"
    );
}

#[test]
fn fix_const_literal_preservation_uses_lexical_const_symbol() {
    let output = emit_dts_with_binding(
        r#"
const tag = "outer";

export namespace N {
  const tag = "inner";
  export const value = tag;
}
"#,
    );

    assert!(
        output.contains("const value = \"inner\";"),
        "Expected namespace const to preserve the inner lexical binding: {output}"
    );
    assert!(
        !output.contains("const value = \"outer\";"),
        "Declaration emit must not use the shadowed top-level const: {output}"
    );
}

#[test]
fn fix_identity_call_preservation_uses_lexical_callee_symbol() {
    let output = emit_dts_with_binding(
        r#"
function id<T extends string>(value: T): T {
  return value;
}

export namespace N {
  function id(_: string) {
    return "wide";
  }

  export const value = id("narrow");
}
"#,
    );

    assert!(
        output.contains("const value: string;"),
        "Expected local non-identity callee to widen the declaration type: {output}"
    );
    assert!(
        !output.contains("const value = \"narrow\";"),
        "Declaration emit must not use the shadowed top-level identity helper: {output}"
    );
}

#[test]
fn fix_returned_object_jsdoc_keeps_method_signature() {
    let output = emit_dts_with_binding(
        r#"
export const foo = (p: string) => {
    return {
        /**
         * comment2
         * @param s
         */
        bar: (s: number) => {},
        /**
         * comment3
         * @param s
         */
        bar2(s: number) {},
    };
};
"#,
    );

    assert!(
        output.contains("bar: (s: number) => void;"),
        "property function syntax should stay unchanged: {output}"
    );
    assert!(
        output.contains("bar2(s: number): void;"),
        "method syntax should survive JSDoc insertion: {output}"
    );
    assert!(
        !output.contains("bar2: (s: number) => void;"),
        "returned object methods must not be rewritten as property functions: {output}"
    );
}

#[test]
fn fix_shadowed_type_param_new_class_return_uses_global_this() {
    let output = emit_dts_with_binding(
        r#"
class A {}

var make = <A,>(value: A) => new A();
function make2<A,>(value: A) {
    return new A();
}

interface B {}
var id = <B,>(value: B) => value;
"#,
    );

    assert!(
        output.contains("declare var make: <A>(value: A) => globalThis.A;"),
        "Expected arrow return to qualify the shadowed class constructor type: {output}"
    );
    assert!(
        output.contains("declare function make2<A>(value: A): globalThis.A;"),
        "Expected function return to qualify the shadowed class constructor type: {output}"
    );
    assert!(
        output.contains("declare var id: <B>(value: B) => B;"),
        "Type-only interface names should still bind to the type parameter: {output}"
    );
}

#[test]
fn fix_static_method_return_this_does_not_emit_instance_this() {
    let output = emit_dts_with_binding(
        r#"
export class Enhancement {
  static getType() {
    return this;
  }
}
"#,
    );

    assert!(
        !output.contains("static getType(): this;"),
        "Static methods must not use instance this return syntax: {output}"
    );
}

// === Tests for symbol_types fallback (issue #8744) ===

#[test]
fn fix_symbol_types_fallback_used_when_node_types_is_any() {
    // When node_types holds ANY (context-contaminated by contextual re-evaluation),
    // the emitter must fall back to symbol_types to recover the arrow function return type.
    use crate::type_cache_view::TypeCacheView;
    use tsz_binder::BinderState;
    use tsz_parser::parser::NodeIndex;
    use tsz_parser::parser::syntax_kind_ext;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{FunctionShape, ParamInfo, TypeId};

    let source = "export const isNonNull = (x: number | null) => x !== null;";
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    // Find the arrow function node.
    let arrow_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| {
            (n.kind == syntax_kind_ext::ARROW_FUNCTION).then_some(NodeIndex(i as u32))
        })
        .expect("ARROW_FUNCTION node");

    // Get the symbol bound to "isNonNull".
    let sym_id = binder
        .file_locals
        .get("isNonNull")
        .expect("isNonNull symbol");

    // Build a concrete function type: (x: number | null) => boolean
    let interner = TypeInterner::new();
    let x_atom = interner.intern_string("x");
    let param_type = interner.union(vec![TypeId::NUMBER, TypeId::NULL]);
    let func_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(x_atom),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Simulate contamination: node_types = ANY, symbol_types = real function type.
    let mut type_cache = TypeCacheView::default();
    type_cache.node_types.insert(arrow_idx.0, TypeId::ANY);
    type_cache.symbol_types.insert(sym_id, func_type);

    let mut emitter = crate::declaration_emitter::DeclarationEmitter::with_type_info(
        &parser.arena,
        type_cache,
        &interner,
        &binder,
    );
    let output = emitter.emit(root);

    assert!(
        !output.contains(": any"),
        "should not emit `any` when symbol_types holds a concrete type: {output}"
    );
    assert!(
        output.contains("boolean"),
        "should recover boolean return type from symbol_types: {output}"
    );
}

#[test]
fn fix_predicate_pattern2_does_not_rewrite_unrelated_union_shapes() {
    // (T & undefined) | string must NOT be rewritten as T & ({} | undefined).
    // Only (T & undefined) | (T & {}) qualifies — both arms must be verified.
    use crate::type_cache_view::TypeCacheView;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{
        FunctionShape, ParamInfo, TypeId,
        types::{TypeParamInfo, TypePredicate, TypePredicateTarget},
    };

    let interner = TypeInterner::new();

    // Build type param T.
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo::simple(t_atom));

    // Build (T & undefined) | string — the second arm is NOT T & {}.
    let t_and_undef = interner.intersection(vec![t_param, TypeId::UNDEFINED]);
    let bad_union = interner.union(vec![t_and_undef, TypeId::STRING]);

    // Build a function type with predicate `x is (T & undefined) | string`.
    let x_atom = interner.intern_string("x");
    let func_type = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo::simple(t_atom)],
        params: vec![ParamInfo {
            name: Some(x_atom),
            type_id: TypeId::UNKNOWN,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(x_atom),
            type_id: Some(bad_union),
            parameter_index: None,
        }),
        is_constructor: false,
        is_method: false,
    });

    let parser = tsz_parser::ParserState::new("test.ts".to_string(), String::new());
    let binder = tsz_binder::BinderState::new();
    let type_cache = TypeCacheView::default();
    let emitter = crate::declaration_emitter::DeclarationEmitter::with_type_info(
        &parser.arena,
        type_cache,
        &interner,
        &binder,
    );

    // The predicate type should fall through to the normal printer, not rewrite.
    let text = emitter
        .function_type_predicate_text(func_type, None)
        .unwrap_or_default();

    assert!(
        !text.contains("& ({} | undefined)"),
        "should not rewrite (T & undefined) | string as T & ({{}} | undefined): {text}"
    );
}

#[test]
fn fix_inferred_predicate_return_prefers_nameable_alias() {
    use crate::type_cache_view::TypeCacheView;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{
        FunctionShape, ParamInfo, TypeId,
        types::{ObjectFlags, ObjectShape, TypePredicate, TypePredicateTarget},
    };

    let mut parser = tsz_parser::ParserState::new(
        "test.ts".to_string(),
        r#"
type Foo = {
    foo: string;
};
type Bar = Foo & {
    bar: string;
};
"#
        .to_string(),
    );
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let bar_sym = binder.file_locals.get("Bar").expect("missing Bar symbol");

    let interner = TypeInterner::new();
    let bar_def = tsz_solver::DefId(94_501);
    let foo_surface = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let bar_surface = interner.object_with_index(ObjectShape {
        flags: ObjectFlags::default(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
    });
    let param_type = interner.union(vec![foo_surface, bar_surface, TypeId::NULL]);

    let x_atom = interner.intern_string("x");
    let func_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(x_atom),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: Some(TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(x_atom),
            type_id: Some(bar_surface),
            parameter_index: None,
        }),
        is_constructor: false,
        is_method: false,
    });

    let mut type_cache = TypeCacheView::default();
    type_cache.def_to_symbol.insert(bar_def, bar_sym);
    type_cache.def_types.insert(bar_def.0, bar_surface);
    let emitter = crate::declaration_emitter::DeclarationEmitter::with_type_info(
        &parser.arena,
        type_cache,
        &interner,
        &binder,
    );
    let output = emitter
        .function_type_predicate_text(func_type, None)
        .unwrap_or_default();

    assert!(
        output == "x is Bar",
        "expected inferred predicate return to preserve the public alias: {output}"
    );
    assert!(
        !output.contains("x is {"),
        "predicate return should not expand the nameable alias structurally: {output}"
    );
}

#[test]
fn fix_element_access_decl_uses_source_array_element_type() {
    let output = emit_dts_with_usage_analysis(
        r#"
type Foo = {
    foo: string;
};
type Bar = Foo & {
    bar: string;
};

const list: (Foo | Bar)[] = [];
const fooOrBar = list[0];
"#,
    );

    assert!(
        output.contains("declare const fooOrBar: Foo | Bar;"),
        "element access over a source array annotation should print the element type: {output}"
    );
}

#[test]
fn fix_array_filter_typeof_decl_uses_narrowed_primitive_array() {
    let output = emit_dts_with_usage_analysis(
        r#"
const strings = [1, "foo", 2, "bar"].filter(x => typeof x === "string");
const numbers = ["a", 1, "b", 2].filter(x => "number" === typeof x);
const impossible = [1, 2].filter(x => typeof x === "string");
"#,
    );

    assert!(
        output.contains("declare const strings: string[];"),
        "typeof string filter should print string[]: {output}"
    );
    assert!(
        output.contains("declare const numbers: number[];"),
        "reversed typeof number filter should print number[]: {output}"
    );
    assert!(
        !output.contains("declare const impossible: string[];"),
        "impossible typeof string filter should not be overstated as string[]: {output}"
    );
}

#[test]
fn fix_array_map_callback_decl_uses_return_array_surface() {
    let output = emit_dts_with_usage_analysis(
        r#"
type MyObj = { data?: string };
type MyArray = { list?: MyObj[] }[];
const myArray: MyArray = [];

const result = myArray
  .map((arr) => arr.list)
  .filter((arr) => arr && arr.length)
  .map((arr) => arr
    .filter((obj) => obj && obj.data)
    .map(obj => JSON.parse(obj.data))
  );
"#,
    );

    assert!(
        output.contains("declare const result: any[][];"),
        "map callback return arrays should compose into the declaration type: {output}"
    );
}

#[test]
fn fix_source_predicate_decl_uses_truthy_union_and_negated_local_guard() {
    let output = emit_dts_with_usage_analysis(
        r#"
const numOrBoolean = (x: number | boolean) => typeof x === "number" || x;

type Animal = { breath: true };
type Rock = { breath: false };
type Something = Animal | Rock;

function isAnimal(something: Something): something is Animal {
  return something.breath;
}

function negative(t: Something) {
  return !isAnimal(t);
}
"#,
    );

    assert!(
        output.contains("declare const numOrBoolean: (x: number | boolean) => x is number | true;"),
        "truthy boolean plus typeof guard should print the inferred predicate: {output}"
    );
    assert!(
        output.contains("declare function negative(t: Something): t is Rock;"),
        "negated local predicate over a union alias should print the complement arm: {output}"
    );
}

// Tests for returned-function-expression recursive unrolling (issue #8683)

#[test]
fn fix_returned_arrow_chain_three_levels_no_shadowing() {
    // outer<A> → middle<B>() → inner<C>(x: C): C — distinct params, no renaming needed
    let output = emit_dts_with_usage_analysis(
        r#"
export function outer<A>() {
    return function middle<B>() {
        return function inner<C>(x: C): C {
            return x;
        };
    };
}
"#,
    );
    assert!(
        output.contains("<B>() => <C>(x: C) => C"),
        "three-level chain with distinct type params: {output}"
    );
    assert!(
        output.contains("declare function outer<A>"),
        "outer type param A preserved: {output}"
    );
}

#[test]
fn fix_returned_arrow_chain_three_levels_all_shadowed() {
    // outer<K> → middle<K>() → inner<K>(x: K): K — K shadows K shadows K
    // expected renames: outer=K, middle=K_1, inner=K_2
    let output = emit_dts_with_usage_analysis(
        r#"
export function outer<K>() {
    return function middle<K>() {
        return function inner<K>(x: K): K {
            return x;
        };
    };
}
"#,
    );
    assert!(
        output.contains("<K_1>() => <K_2>(x: K_2) => K_2"),
        "three-level all-shadowed chain must produce K_1/K_2 renames: {output}"
    );
}

#[test]
fn fix_returned_arrow_chain_two_levels_inner_shadows_outer() {
    // Rule: when the inner closure's type param shadows the outer's, the
    // emitter must rename the inner param (T → T_1) in the DTS return type,
    // and free references to inner's T inside the returned function type
    // must also be updated to T_1.
    let output = emit_dts_with_usage_analysis(
        r#"
export function outer<T>() {
    return function inner<T>(x: T) {
        return function deepest<U>(y: U): [T, U] {
            return [x, y];
        };
    };
}
"#,
    );
    // inner's T shadows outer's T → T_1; deepest's [T, U] references T_1
    assert!(
        output.contains("<T_1>(x: T_1) => <U>(y: U) => [T_1, U]"),
        "two-level inner-shadows-outer chain: {output}"
    );
}

#[test]
fn fix_returned_arrow_chain_concise_arrows() {
    // Concise arrow bodies: outer<V> returns arrow returning arrow.
    // inner has an explicit return annotation `[W, X]` which is used as-is.
    let output = emit_dts_with_usage_analysis(
        r#"
export function outer<V>() {
    return function middle<W>(x: W) {
        return function inner<X>(y: X): [W, X] {
            return [x, y];
        };
    };
}
"#,
    );
    assert!(
        output.contains("<W>(x: W) => <X>(y: X) => [W, X]"),
        "three-level chain with explicit tuple return annotation: {output}"
    );
}
