use super::super::core::*;

#[test]
fn test_js_property_type_annotation_suppresses_downstream_semantic_checks() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        r#"
class Foo {
    constructor() {
        this.prop = {};
    }

    prop: string;

    method() {
        this.prop.foo;
    }
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 8010),
        "Expected TS8010 for property type annotation syntax in JS.\nGot: {diagnostics:#?}"
    );
    for code in [2322, 2339] {
        assert!(
            !has_error(&diagnostics, code),
            "Did not expect downstream semantic TS{code} for property type annotation syntax in JS.\nGot: {diagnostics:#?}"
        );
    }
}

#[test]
fn test_js_as_assertion_reports_ts8016() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.js",
        r#"
0 as number;
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    let ts8016 = diagnostics.iter().filter(|d| d.0 == 8016).count();
    assert_eq!(
        ts8016, 1,
        "Expected exactly one TS8016 for JS as-assertion syntax.\nGot: {diagnostics:#?}"
    );
}

#[test]
fn test_for_in_key_assignment_preserves_extract_keyof_string_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f3<T, K extends Extract<keyof T, string>>(t: T, k: K) {
    for (let key in t) {
        k = key;
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.iter().any(|(_, message)| {
            message.contains("Type 'Extract<keyof T, string>' is not assignable to type 'K'")
        }),
        "Expected for-in key assignment to preserve Extract<keyof T, string> in TS2322.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_binder_errors_use_module_and_cross_function_diagnostics() {
    let diagnostics = compile_and_get_diagnostics_named(
        "plainJSBinderErrors.js",
        r#"
export default 12
function* g() {
    const yield = 4
}
class C {
    label() {
        for(;;) {
            label: var x = 1
            break label
        }
    }
}
const eval = 9
const arguments = 10
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 1214),
        "Expected generator `yield` in a JS module to use TS1214.\nGot: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 1215),
        "Expected top-level `eval`/`arguments` bindings in a JS module to use TS1215.\nGot: {diagnostics:?}"
    );
    assert!(
        has_error(&diagnostics, 1107),
        "Expected `break label` after a non-enclosing labeled statement to use TS1107 in the function body.\nGot: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 1116),
        "Did not expect TS1116 once the cross-function boundary diagnostic is selected.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_import_equals_reserved_word_uses_ts1214() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
// @target: es2015
// @module: commonjs
"use strict"
import public = require("1");
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 1214),
        "Expected `import public = require(...)` to report TS1214 in module context.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_for_in_index_access_preserves_extract_keyof_string_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f3<T, K extends Extract<keyof T, string>>(t: T, k: K, tk: T[K]) {
    for (let key in t) {
        tk = t[key];
    }
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.iter().any(|(_, message)| {
            message.contains("Type 'T[Extract<keyof T, string>]' is not assignable to type 'T[K]'")
        }),
        "Expected generic for-in indexed access to preserve Extract<keyof T, string> in TS2322.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_in_operator_still_requires_object_for_generic_indexed_access() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f54<T>(obj: T, key: keyof T) {
    const b = "foo" in obj[key];
}

function f55<T, K extends keyof T>(obj: T, key: K) {
    const b = "foo" in obj[key];
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.iter().any(|(_, message)| {
            message.contains("Type 'T[keyof T]' is not assignable to type 'object'")
        }),
        "Expected `in` RHS generic indexed access to error as object-incompatible.\nGot: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, message)| {
            message.contains("Type 'T[K]' is not assignable to type 'object'")
        }),
        "Expected `in` RHS keyed generic indexed access to error as object-incompatible.\nGot: {diagnostics:?}"
    );
}

#[test]
fn test_in_operator_generic_indexed_access_anchors_at_rhs_expression() {
    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        r#"
function f54<T>(obj: T, key: keyof T) {
    const b = "foo" in obj[key];
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322 = diagnostics
        .iter()
        .find(|d| {
            d.code == 2322
                && d.message_text
                    .contains("Type 'T[keyof T]' is not assignable to type 'object'")
        })
        .expect("expected TS2322 for generic indexed-access in-operator RHS");

    assert_eq!(ts2322.start, 64, "Expected TS2322 to anchor at `obj[key]`.");
}

#[test]
fn test_assignment_diagnostic_preserves_literal_for_literal_sensitive_element_write() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f(obj: { a: number, b: 0 | 1 }, k: 'a' | 'b') {
    obj[k] = "x";
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '\"x\"' is not assignable to type '0 | 1'")
        }),
        "Expected literal-preserving TS2322 for literal-sensitive element write.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_diagnostic_widens_literal_for_generic_indexed_write() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Item = { a: string, b: number };

function f<T extends Item, K extends keyof T>(obj: T, k: K) {
    obj[k] = 123;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'number' is not assignable to type 'T[K]'")
        }),
        "Expected widened source display for generic indexed write TS2322.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_mapped_type_known_keys_emit_ts2551_and_ts2862() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function test<Shape extends Record<string, string>>(shape: Shape, key: keyof Shape) {
    const obj = {} as Record<keyof Shape | "knownLiteralKey", number>;

    obj.knownLiteralKey = 1;
    obj[key] = 2;

    obj.unknownLiteralKey = 3;
    obj['' as string] = 4;
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2551
                && message.contains("unknownLiteralKey")
                && message.contains("knownLiteralKey")
        }),
        "Expected TS2551 for unknown literal property on generic mapped type.\nActual diagnostics: {diagnostics:#?}"
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2862 && message.contains("Record<keyof Shape | \"knownLiteralKey\", number>")
        }),
        "Expected TS2862 for broad string write through generic mapped type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_diagnostic_preserves_generic_mapped_intersection_index_access_target() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Errors<T> = { [P in keyof T]: string | undefined } & { all: string | undefined };

function foo<T>() {
    let obj!: Errors<T>;
    let x!: keyof T;
    obj[x] = undefined;
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message
                    .contains("Type 'undefined' is not assignable to type 'Errors<T>[keyof T]'.")
        }),
        "Expected TS2322 to preserve generic mapped intersection indexed-access display.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_readonly_generic_write_with_concrete_keyof_reports_ts2862_not_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Dict = { readonly [key: string]: number };

function f<T extends Dict, K extends keyof T>(
    obj: T,
    k1: keyof Dict,
    k2: keyof T,
    k3: K,
) {
    obj.foo = 123;
    obj[k1] = 123;
    obj[k2] = 123;
    obj[k3] = 123;
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2862 && message.contains("Type 'T' is generic") }),
        "Expected TS2862 for concrete keyof write through readonly generic constraint.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2536),
        "Did not expect TS2536 to preempt TS2862 for readonly generic write.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_tuple_value_index_access_does_not_emit_ts2536() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r##"
type Bar<T> = { [K in keyof T]: [K] };
type Wrapped<T> = { [key: string]: { [K in keyof T]: [K] }[keyof T] };
type Qux<T, Q extends Wrapped<T>> = { [K in keyof Q]: T[Q[K]["0"]] };
"##,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2536),
        "Did not expect TS2536 for indexing mapped tuple values with \"0\".\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_return_diagnostic_preserves_literal_for_generic_indexed_target() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Type {
    a: 123;
    b: "some string";
}

function get123<K extends keyof Type>(): Type[K] {
    return 123;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type '123' is not assignable to type 'Type[K]'")
        }),
        "Expected literal-preserving TS2322 for generic indexed return.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_diagnostic_widens_literal_for_keyof_target() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f4<T extends { [K in keyof T]: string }>(k: keyof T) {
    k = 42;
    k = "hello";
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'number' is not assignable to type 'keyof T'")
        }),
        "Expected widened numeric literal display for keyof target TS2322.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'string' is not assignable to type 'keyof T'")
        }),
        "Expected widened string literal display for keyof target TS2322.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_assignment_diagnostic_widens_literal_for_named_keyof_display_target() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare const sym: unique symbol;

function g<T>() {
    type Orig = { [k: string]: any, str: any, [sym]: any } & T;
    type NonIndex<T extends PropertyKey> = {} extends Record<T, any> ? never : T;
    type DistributiveNonIndex<T extends PropertyKey> = T extends unknown ? NonIndex<T> : never;
    type Remapped = { [K in keyof Orig as DistributiveNonIndex<K>]: any };
    let x: keyof Remapped;
    x = "whatever";
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type 'string' is not assignable to type 'keyof Remapped'")
        }),
        "Expected named `keyof` target display to widen literal source text.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message
                    .contains("Type '\"whatever\"' is not assignable to type 'keyof Remapped'")
        }),
        "Did not expect literal source text for named `keyof` target display.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_string_index_constraint_allows_read_but_rejects_write_via_dot_access() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f<T extends { [key: string]: number }>(c: T, k: keyof T) {
    c.x;
    c[k];
    c.x = 1;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'x' does not exist on type 'T'")
        }),
        "Expected TS2339 for generic write through dot access.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|(code, message)| {
                *code == 2339 && message.contains("Property 'x' does not exist on type 'T'")
            })
            .count(),
        1,
        "Expected only the write access to error.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2339_preserves_merge_alias_receiver_for_instantiation_chain() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type merge<base, props> = Omit<base, keyof props & keyof base> & props;
declare const merge: <l, r>(l: l, r: r) => merge<l, r>;

const o1 = merge({ p1: 1 }, { p2: 2 });
const o2 = merge(o1, { p3: 3 });
o2.p4;
"#,
    );

    let ts2339 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing p4");
    assert!(
        ts2339.1.contains("merge<merge<"),
        "Expected TS2339 receiver to preserve merge alias chain.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2339.1.contains("Omit<"),
        "Expected TS2339 receiver to avoid the expanded Omit surface.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .1
            .contains("merge<merge<{ p1: number; }, { p2: number; }>, { p3: number; }>"),
        "Expected TS2339 receiver to widen inferred merge literal arguments.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2339_keeps_conditional_merge_receiver_branch_display() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type merge<base, props> = keyof base & keyof props extends never
    ? base & props
    : Omit<base, keyof props & keyof base> & props;
declare const merge: <l, r>(l: l, r: r) => merge<l, r>;

const o1 = merge({ p1: 1 }, { p2: 2 });
const o2 = merge(o1, { p2: 2, p3: 3 });
o2.p4;
"#,
    );

    let ts2339 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing p4");
    assert!(
        ts2339.1.contains("Omit<"),
        "Expected TS2339 receiver to preserve the conditional Omit branch.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !ts2339.1.contains("merge<"),
        "Expected TS2339 receiver not to repaint a resolved conditional branch as merge.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2339_elides_long_merge_receiver_instantiation_chain() {
    let mut source = String::from(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type merge<base, props> = Omit<base, keyof props & keyof base> & props;
declare const merge: <l, r>(l: l, r: r) => merge<l, r>;

const o1 = merge({ p1: 1 }, { p2: 2 });
"#,
    );
    for i in 2..=30 {
        source.push_str(&format!(
            "const o{i} = merge(o{}, {{ p{}: {} }});\n",
            i - 1,
            i + 1,
            i + 1
        ));
    }
    source.push_str("o30.p38;\no30.p51;\n");

    let diagnostics = compile_and_get_diagnostics(&source);
    assert!(
        diagnostics.iter().filter(|(code, _)| *code == 2339).count() == 2,
        "Expected TS2339 for both missing long-chain properties.\nActual diagnostics: {diagnostics:#?}"
    );
    for (_, message) in diagnostics.iter().filter(|(code, _)| *code == 2339) {
        assert!(
            message.matches("merge<").count() >= 25,
            "Expected TS2339 receiver to preserve the long merge application chain.\nActual message: {message}"
        );
        assert!(
            message.contains("{ p1: number; }")
                && message.contains("{ p2: number; }")
                && message.contains("{ p5: number; }"),
            "Expected TS2339 receiver to preserve the stable merge chain prefix.\nActual message: {message}"
        );
        assert!(
            message.contains("{ ...; }"),
            "Expected TS2339 receiver to elide the middle merge object arguments.\nActual message: {message}"
        );
        assert!(
            !message.contains("{ p31: number; }"),
            "Expected TS2339 receiver to truncate before the shallow suffix.\nActual message: {message}"
        );
        assert!(
            message.contains("{ ....."),
            "Expected TS2339 receiver truncation to match tsc's merge-chain suffix.\nActual message: {message}"
        );
        assert!(
            !message.contains("<...,"),
            "Expected TS2339 receiver not to collapse the older chain to a raw ellipsis.\nActual message: {message}"
        );
        assert!(
            message.len() < 390,
            "Expected TS2339 receiver to stay bounded.\nActual len: {}\nActual message: {message}",
            message.len()
        );
    }
}

#[test]
fn test_ts2339_elides_long_merge_receiver_method_chain_shape_access() {
    let mut source = String::from(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type merge<base, props> = Omit<base, keyof props & keyof base> & props;
type Type<t> = {
    shape: t;
    merge: <r>(r: r) => Type<merge<t, r>>;
};

declare const o1: Type<{ p1: 1 }>;
"#,
    );
    for i in 2..=30 {
        source.push_str(&format!(
            "const o{i} = o{}.merge({{ p{}: {} }});\n",
            i - 1,
            i,
            i
        ));
    }
    source.push_str("o30.shape.p31;\no30.shape.p38;\no30.shape.p50;\n");

    let diagnostics = compile_and_get_diagnostics(&source);
    assert!(
        diagnostics.iter().filter(|(code, _)| *code == 2339).count() == 3,
        "Expected TS2339 for missing long-chain shape properties.\nActual diagnostics: {diagnostics:#?}"
    );
    for (_, message) in diagnostics.iter().filter(|(code, _)| *code == 2339) {
        assert!(
            message.contains("{ p1: 1; }")
                && message.contains("{ p2: number; }")
                && message.contains("{ p5: number; }"),
            "Expected TS2339 receiver to preserve the stable method-chain prefix.\nActual message: {message}"
        );
        assert!(
            message.contains("{ ...; }") && message.contains("{ ....."),
            "Expected TS2339 receiver to elide and truncate the middle method-chain arguments.\nActual message: {message}"
        );
        assert!(
            !message.contains("{ p30: number; }") && !message.contains("<...,"),
            "Expected TS2339 receiver not to keep the shallow suffix or raw ellipsis.\nActual message: {message}"
        );
    }
}

#[test]
fn test_object_literal_source_display_preserves_quoted_numeric_property_names() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
const so2: string = { "0": 1 };
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type '{ \"0\": number; }' is not assignable to type 'string'")
        }),
        "Expected object-literal source display to preserve quoted numeric property names.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_object_literal_property_mismatch_widens_literal_source_display() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Foo {
    inner: {
        thing: string
    }
}

const foo: Foo = {
    inner: {
        thing: 1
    }
};
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'number' is not assignable to type 'string'")
        }),
        "Expected object-literal property mismatch to widen literal source display.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_conditional_return_with_any_branch_reports_non_any_failing_branch() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare function getAny(): any;

function return2(x: string): string {
    return x.startsWith("a") ? getAny() : 1;
}

const return5 = (x: string): string => x.startsWith("a") ? getAny() : 1;
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ESNext,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let branch_errors = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("Type 'number' is not assignable to type 'string'")
        })
        .count();

    assert_eq!(
        branch_errors, 2,
        "Expected conditional return branches to report the non-any branch mismatch.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_chained_assignment_diagnostics_use_terminal_rhs_source() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
var a: string;
var b: number;
var c: boolean;
var d: Date;
var e: RegExp;

a = b = c = d = e = null;
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322_messages
            .iter()
            .all(|message| message.contains("Type 'null'")),
        "Expected chained assignment diagnostics to report the terminal RHS source type.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_class_type_param_error_suppresses_cascading_call_mismatch() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
namespace Editor {
    export class List<T> {
        public next!: List<T>;
        public prev!: List<T>;

        constructor(public isHead: boolean, public data: T) {}

        public static MakeHead(): List<T> {
            var entry: List<T> = new List<T>(true, null);
            return entry;
        }
    }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2302_count = diagnostics.iter().filter(|(code, _)| *code == 2302).count();
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert!(
        ts2302_count >= 3,
        "Expected TS2302s for illegal class type-parameter references in static member.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 0,
        "Did not expect a cascading TS2345 once TS2302 has already invalidated the call.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_method_type_params_shadow_class_type_params() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
class Result<T, E> {
    constructor() {}

    static ok<T, E>(): Result<T, E> {
        return new Result<T, E>();
    }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Static method type parameters should shadow class type parameters in signatures and bodies.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_static_method_type_params_still_check_constructor_argument_nullability() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
namespace Editor {
    export class List<T> {
        public next!: List<T>;
        public prev!: List<T>;

        constructor(public isHead: boolean, public data: T) {}

        public static MakeHead2<T>(): List<T> {
            var entry: List<T> = new List<T>(true, null);
            entry.prev = entry;
            entry.next = entry;
            return entry;
        }

        public static MakeHead3<U>(): List<U> {
            var entry: List<U> = new List<U>(true, null);
            entry.prev = entry;
            entry.next = entry;
            return entry;
        }
    }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();
    let ts2302_count = diagnostics.iter().filter(|(code, _)| *code == 2302).count();

    assert_eq!(
        ts2302_count, 0,
        "Method type parameters should shadow class type parameters in static members.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2345_count, 2,
        "Explicitly-instantiated constructor arguments should still check nullability against method type parameters.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_non_generic_conditional_type_alias_resolves_before_assignability() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface Synthetic<A, B extends A> {}
type SyntheticDestination<T, U> = U extends Synthetic<T, infer V> ? V : never;
type TestSynthetic = SyntheticDestination<number, Synthetic<number, number>>;

const y: TestSynthetic = 3;
const z: TestSynthetic = '3';
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("Type 'string' is not assignable to type 'number'")
        }),
        "Expected the failing assignment to compare against resolved `number`.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains(
                    "Type 'number' is not assignable to type 'SyntheticDestination<number, Synthetic<number, number>>'"
                )
        }),
        "Expected the successful assignment to stop erroring once the alias resolves.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_type_assertion_no_overlap_widens_function_literal_return_type() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
var foo = <{ (): number; }> function() { return "err"; };
var bar = <{():number; (i:number):number; }> (function(){return "err";});
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2352
                && message.contains("Conversion of type '() => string' to type '() => number'")
        }),
        "Expected TS2352 to widen the function return literal to `string`.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2352
                && message.contains(
                    "Conversion of type '() => string' to type '{ (): number; (i: number): number; }'"
                )
        }),
        "Expected overload target TS2352 to widen the function return literal to `string`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_type_reports_ts2538_for_any_index() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Shape {
    name: string;
    width: number;
    height: number;
    visible: boolean;
}

type T = Shape[any];
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2538 && message.contains("Type 'any' cannot be used as an index type")
        }),
        "Expected TS2538 for `Shape[any]`.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_indexed_access_type_reports_ts2537_for_array_string_index() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
type T = string[][string];
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2537
                && message
                    .contains("Type 'string[]' has no matching index signature for type 'string'")
        }),
        "Expected TS2537 for `string[][string]`.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2536),
        "Did not expect TS2536 for `string[][string]` once concrete classifier applies.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_intersection_callback_return_preserves_object_literal_members() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare function test4(
  arg: { a: () => { prop: "foo" } } & {
    [k: string]: () => { prop: any };
  },
): unknown;

test4({
  a: () => ({ prop: "foo" }),
  b: () => ({ prop: "bar" }),
});

test4({
  a: () => ({ prop: "bar" }),
});
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    let bar_errors = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("Type '\"bar\"' is not assignable to type '\"foo\"'")
        })
        .count();

    assert_eq!(
        bar_errors, 1,
        "Expected exactly the single invalid callback-return literal mismatch from test4, matching the TypeScript baseline.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_excess_property_display_widens_mapped_callback_value_param() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare function f2<T extends object>(
  data: T,
  handlers: { [P in keyof T as T[P] extends string ? P : never]: (value: T[P], prop: P) => void },
): void;

f2(
  {
    foo: 0,
    bar: "",
  },
  {
    foo: (value, key) => {},
  },
);
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            strict_null_checks: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(_, message)| message.contains("(value: string, prop: \"bar\") => void")),
        "Expected excess-property target display to widen callback value parameter to string.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(_, message)| message.contains("(value: \"\", prop: \"bar\") => void")),
        "Did not expect literal empty-string callback parameter in excess-property target display.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "Pre-existing failure: AsyncGenerator lib types emit TS2504/TS2318"]
fn test_async_generator_type_references_preserve_all_type_params() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface Result<T, E> {
    [Symbol.iterator](): Generator<E, T, unknown>
}

type Book = { id: string; title: string; authorId: string };
type Author = { id: string; name: string };
type BookWithAuthor = Book & { author: Author };

declare const authorPromise: Promise<Result<Author, "NOT_FOUND_AUTHOR">>;
declare const mapper: <T>(result: Result<T, "NOT_FOUND_AUTHOR">) => Result<T, "NOT_FOUND_AUTHOR">;
type T = AsyncGenerator<string, number, unknown>;
declare const g: <T, U, V>() => AsyncGenerator<T, U, V>;
async function* f(): AsyncGenerator<"NOT_FOUND_AUTHOR" | "NOT_FOUND_BOOK", BookWithAuthor, unknown> {
    const test1 = await authorPromise.then(mapper);
    const test2 = yield* await authorPromise.then(mapper);
    const x1 = yield* g();
    const x2: number = yield* g();
    return null! as BookWithAuthor;
}
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "AsyncGenerator should retain its 3-parameter lib arity.\nActual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2322).count(),
        0,
        "AsyncGenerator yield* contextual typing should preserve delegated return context.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| matches!(*code, 2504 | 2769)),
        "Optional callback unions should preserve contextual signatures for generic mappers.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "Delegated `yield* await promise.then(mapper)` should not over-constrain the generic mapper callback.\nActual diagnostics: {diagnostics:#?}"
    );
}
