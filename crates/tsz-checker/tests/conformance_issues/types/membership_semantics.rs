use super::super::core::*;

#[test]
fn test_destructuring_fallback_literals_do_not_emit_false_assignability_errors() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
function f1(options?: { color: string, width: number }) {
    let { color, width } = options || {};
    ({ color, width } = options || {});
}

function f2(options?: [string, number]) {
    let [str, num] = options || [];
    [str, num] = options || [];
}

declare const tupleFallback: [number, number] | undefined;
const [a, b = a] = tupleFallback ?? [];

declare const objectFallback: { a?: number, b?: number } | undefined;
const { a: objA, b: objB = objA } = objectFallback ?? {};
"#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Did not expect TS2322 from destructuring fallback literals. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2739),
        "Did not expect TS2739 from destructuring fallback literals. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_property_errors_use_named_generic_type_display_for_element_access_receivers() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface A<T> { x: T; }
interface B { m: string; }

var x: any;
var y = x as A<B>[];
var z = y[0].m;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'm' does not exist on type 'A<B>'.")
        }),
        "Expected TS2339 to display the named generic type instead of Lazy(def) internals. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("Lazy(")),
        "Did not expect Lazy(def) internals in TS2339 output. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_literal_key_constraints_do_not_fall_through_to_ts7053() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
let mappedObject: {[K in "foo"]: null | {x: string}} = {foo: {x: "hello"}};
declare function foo<T>(x: T): null | T;

function bar<K extends "foo">(key: K) {
  const element = foo(mappedObject[key]);
  if (element == null)
    return;
  const x = element.x;
}
"#,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 when the generic key constraint is a concrete literal. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_parenthesized_nullish_and_logical_expressions_do_not_emit_false_ts2322() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare const a: string | undefined;
declare const b: string | undefined;
declare const c: string | undefined;

a ?? (b || c);
(a || b) ?? c;
a ?? (b && c);
(a && b) ?? c;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Did not expect TS2322 for parenthesized nullish/logical combinations. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_logical_or_under_type_assertion_does_not_emit_false_ts2322() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
interface Arg<T = any, Params extends Record<string, any> = Record<string, any>> {
    "__is_argument__"?: true;
    meta?: T;
    params?: Params;
}

export function myFunction<T = any, U extends Record<string, any> = Record<string, any>>(arg: Arg<T, U>) {
    return (arg.params || {}) as U;
}
        "#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Did not expect TS2322 from a logical-or branch inside a type assertion. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_string_is_assignable_to_iterable_string_under_es2015() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r##"
function method<T>(iterable: Iterable<T>): T {
    return;
}

var res: string = method("test");
"##,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected the generic return error to remain. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Expected string to satisfy Iterable<string> under ES2015. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_callback_return_mismatch_reports_ts2345_for_identifier_expression_body() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function someGenerics3<T>(producer: () => T) { }
someGenerics3<number>(() => undefined);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // For contextually-typed callbacks (no explicit param annotations), tsc
    // elaborates the return type and reports TS2322 on the body expression.
    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 on the body expression for contextual callback. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_object_literal_argument_prefers_property_ts2322_over_outer_ts2345() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
function foo<T>(x: { bar: T; baz: T }) {
    return x;
}

foo({ bar: 1, baz: '' });
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected property-level TS2322 for generic object literal mismatch. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Did not expect outer TS2345 once object literal property elaboration applies. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_generic_literal_argument_error_reports_widened_direct_mismatch() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function someGenerics9<T>(a: T, b: T, c: T): T {
    return null as any;
}
someGenerics9('', 0, []);
"#,
        CheckerOptions::default(),
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message.contains(
                    "Argument of type 'number' is not assignable to parameter of type 'string'",
                )
        }),
        "Expected TS2345 to report the widened direct primitive mismatch. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_index_signature_and_mapped_type_properties_are_allowed() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface AA {
    [s: string]: number
}

type BB = {
    [P in keyof any]: number
}

declare const a: AA;
declare const b: BB;

delete a.a;
delete a.b;
delete b.a;
delete b.b;
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2790),
        "Did not expect TS2790 for index-signature-like delete operands. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_private_identifier_reports_ts18011() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class A {
    #v = 1;
    constructor() {
        delete this.#v;
    }
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18011),
        "Expected TS18011 for delete on a private identifier. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_readonly_named_property_reports_ts2704() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface A {
    readonly b: number;
}
declare const a: A;
delete a.b;
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 2704),
        "Expected TS2704 for delete on a readonly named property. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2540),
        "Did not expect TS2540 for delete on a readonly named property. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2790),
        "Did not expect TS2790 once readonly delete is detected first. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_readonly_index_signature_still_reports_ts2542() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface B {
    readonly [k: string]: string;
}
declare const b: B;
delete b["test"];
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 2542),
        "Expected TS2542 for delete through a readonly index signature. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2704),
        "Did not expect TS2704 for delete through a readonly index signature. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_class_name_property_reports_ts2704() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
interface Function { readonly name: string; }
class Foo {}
delete Foo.name;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2704),
        "Expected TS2704 for delete on class constructor name. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2790),
        "Did not expect TS2790 for delete on class constructor name. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_nullish_plus_still_reports_ts2365_without_strict_null_checks() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
null + undefined;
null + null;
undefined + undefined;
"#,
        CheckerOptions {
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    );

    let ts2365_count = diagnostics.iter().filter(|(code, _)| *code == 2365).count();
    assert_eq!(
        ts2365_count, 3,
        "Expected TS2365 for each nullish + expression without strictNullChecks. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_semantic_error_operand_still_reports_ts2703() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
enum E { A, B }
delete (E[0] + E["B"]);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            always_strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2703),
        "Expected TS2703 on delete of a semantic-error operand expression. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_enum_member_element_access_reports_ts2704() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
enum E { A, B }
delete E["A"];
"#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 2704),
        "Expected TS2704 for delete on enum member element access. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_optional_chain_reports_ts2790_across_access_forms() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
declare const o1: undefined | { b: string };
delete o1?.b;
delete (o1?.b);

declare const o3: { b: undefined | { c: string } };
delete o3.b?.c;
delete (o3.b?.c);

declare const o6: { b?: { c: { d?: { e: string } } } };
delete o6.b?.["c"].d?.["e"];
delete (o6.b?.["c"].d?.["e"]);
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2790_count = diagnostics.iter().filter(|(code, _)| *code == 2790).count();
    assert_eq!(
        ts2790_count, 6,
        "Expected TS2790 for each delete optional-chain variant. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_delete_plain_properties_respects_exact_optional_property_types() {
    let non_exact = compile_and_get_diagnostics_with_options(
        r#"
interface Foo {
    a: number;
    b: number | undefined;
    c: number | null;
    d?: number;
}
declare const f: Foo;
delete f.a;
delete f.b;
delete f.c;
delete f.d;
"#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let non_exact_ts2790 = non_exact.iter().filter(|(code, _)| *code == 2790).count();
    assert_eq!(
        non_exact_ts2790, 2,
        "Expected TS2790 only for required non-undefined properties without exactOptionalPropertyTypes. Actual: {non_exact:#?}"
    );

    let exact = compile_and_get_diagnostics_with_options(
        r#"
interface Foo {
    a: number;
    b: number | undefined;
    c: number | null;
    e: number | undefined | null;
}
declare const f: Foo;
delete f.a;
delete f.b;
delete f.c;
delete f.e;
"#,
        CheckerOptions {
            strict_null_checks: true,
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );
    let exact_ts2790 = exact.iter().filter(|(code, _)| *code == 2790).count();
    assert_eq!(
        exact_ts2790, 2,
        "Expected TS2790 only for properties without undefined in type (a, c). tsc checks if undefined is assignable to the type regardless of exactOptionalPropertyTypes. Actual: {exact:#?}"
    );
}

#[test]
fn test_ts2403_widens_generic_call_literal_result_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function someGenerics9<T>(a: T, b: T, c: T): T {
    return null as any;
}
var a9a = someGenerics9('', 0, []);
var a9a: {};
"#,
        CheckerOptions::default(),
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2403
                && message.contains("Variable 'a9a' must be of type 'string'")
                && !message.contains("Variable 'a9a' must be of type '\"\"'")
        }),
        "Expected TS2403 to widen the generic call result to string for redeclaration display. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_class_extends_aliased_base_preserves_instance_members() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    value!: T;
}

class Derived extends Base<string> {
    getValue() {
        return this.value;
    }
}

const value: string = new Derived().getValue();
"#,
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no non-lib diagnostics for class inheritance through aliased base symbol, got: {relevant:?}"
    );
}

#[test]
fn test_deeppartial_optional_chain_mixed_property_types_remain_distinct() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;
type DeepInput<T> = DeepPartial<T>;

interface RetryOptions {
    timeout: number;
    retries: number;
    nested: {
        transport: {
            backoff: {
                base: number;
                max: number;
                jitter: number;
            };
        };
        flags: {
            fast: boolean;
            safe: boolean;
        };
    };
}

declare const options: DeepInput<RetryOptions> | undefined;

const base: number = options?.nested?.transport?.backoff?.base ?? 10;
const safe: boolean = options?.nested?.flags?.safe ?? false;
const bad: number = options?.nested?.flags?.safe ?? false;
        "#,
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for boolean-to-number assignment.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_destructure_tuple_with_rest_reports_nullish_not_string_array_property_error() {
    let options = CheckerOptions {
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type NonEmptyStringArray = [string, ...Array<string>];
const strings: NonEmptyStringArray = ['one', 'two'];
const [s0, s1, s2] = strings;
s0.toUpperCase();
s1.toUpperCase();
s2.toUpperCase();
"#,
        options,
    );

    let non_lib: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    let ts2339_count = non_lib.iter().filter(|(code, _)| *code == 2339).count();

    assert_eq!(
        ts2339_count, 0,
        "Expected no TS2339 string[] property error for destructured rest elements, got: {non_lib:?}"
    );

    // s1 and s2 are from the rest region (index >= 1 fixed element), so with
    // noUncheckedIndexedAccess they should be `string | undefined` and calling
    // .toUpperCase() on them should produce TS18048.
    let ts18048_count = non_lib.iter().filter(|(code, _)| *code == 18048).count();
    assert_eq!(
        ts18048_count, 2,
        "Expected 2 TS18048 errors for s1 and s2 possibly undefined; got all diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tuple_destructuring_fixed_tuple_no_ts18048() {
    // Fixed-length tuples should NOT produce TS18048 - all elements are guaranteed to exist
    let options = CheckerOptions {
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare const arr: [string, string];
const [s0, s1] = arr;
s0.toUpperCase();
s1.toUpperCase();
"#,
        options,
    );
    let non_lib: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !non_lib.iter().any(|(code, _)| *code == 18048),
        "Fixed tuple should NOT produce TS18048; got: {non_lib:?}"
    );
}

#[test]
fn test_object_rest_keeps_index_signature_under_no_unchecked_indexed_access() {
    let options = CheckerOptions {
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare const numMapPoint: { x: number, y: number} & { [s: string]: number };
const { x, ...q } = numMapPoint;
x.toFixed();
q.y.toFixed();
q.z.toFixed();
"#,
        options,
    );
    let non_lib: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&non_lib, 2339),
        "Expected no TS2339 for q.z when index signature is preserved; got: {non_lib:?}"
    );
    assert!(
        has_error(&non_lib, 18048),
        "Expected TS18048 for q.z possibly undefined under noUncheckedIndexedAccess; got: {non_lib:?}"
    );
}

#[test]
fn test_branded_primitive_in_mapped_constraint_preserves_literal_keys() {
    // Conformance: TypeScript/tests/cases/compiler/specialIntersectionsInMappedTypes.ts
    //
    // `(string & {}) | "literal"` is the documented "branded primitive" idiom
    // that prevents tsc from absorbing the literal into the wide `string`
    // intrinsic. When used as the key type of a mapped type, the result is
    // a hybrid object whose literal members carry their concrete value type
    // and whose `string & {}` brand expands to a string index signature.
    //
    // Under `noUncheckedIndexedAccess`, accessing a *known* literal key on
    // such an object must yield the value type without `| undefined` (the
    // literal property is concrete), while accessing an unknown key must
    // go through the index signature and add `| undefined` to the result.
    //
    // Before this fix, the `string & {}` intersection collapsed to plain
    // `string` during interning and during evaluator subtype simplification,
    // which absorbed the literal members in the union and turned the mapped
    // type into a bare `{ [x: string]: V }`. That caused tsz to flag known
    // accesses (e.g., `a.left`) as possibly `undefined`, diverging from tsc.
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type Alignment = (string & {}) | "left" | "center" | "right";
type Alignments = Record<Alignment, string>;

declare const a: Alignments;

a.left.length;          // OK — `left` is a literal property, type is `string`
a.center.length;        // OK
a.right.length;         // OK
a.other.length;         // ERROR — falls through index signature, `string | undefined`
"#,
        options,
    );

    let non_lib: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    let undefined_msgs: Vec<_> = non_lib
        .iter()
        .filter(|(code, _)| *code == 18048)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert_eq!(
        undefined_msgs.len(),
        1,
        "Expected exactly one TS18048 for the index-signature access (a.other); \
         literal-key accesses (a.left/a.center/a.right) must NOT trigger TS18048. \
         Actual: {undefined_msgs:?}"
    );
    assert!(
        undefined_msgs[0].contains("a.other"),
        "TS18048 must be reported on `a.other` (the index-signature access), got {undefined_msgs:?}"
    );
    assert!(
        !undefined_msgs
            .iter()
            .any(|m| m.contains("a.left") || m.contains("a.center") || m.contains("a.right")),
        "TS18048 must NOT be reported on literal-key accesses; got {undefined_msgs:?}"
    );
}

#[test]
fn test_branded_primitive_alternate_iteration_var_name() {
    // Same invariant as the test above, but the mapped type uses iteration
    // variable name `K` instead of `P`. Guards against any hardcoded variable
    // name in the printer or solver expansion path (see CLAUDE.md §25).
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..Default::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
type K = "a" | "b" | (string & {});
type M = { [X in K]: number };

declare const m: M;
m.a.toFixed();          // OK
m.b.toFixed();          // OK
m.zzz.toFixed();        // ERROR — index-signature access
"#,
        options,
    );

    let non_lib: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();

    let undefined_msgs: Vec<_> = non_lib
        .iter()
        .filter(|(code, _)| *code == 18048)
        .map(|(_, msg)| msg.clone())
        .collect();

    assert_eq!(
        undefined_msgs.len(),
        1,
        "Expected exactly one TS18048 (for m.zzz). Actual: {undefined_msgs:?}"
    );
    assert!(
        undefined_msgs[0].contains("m.zzz"),
        "TS18048 must be reported on `m.zzz`, got {undefined_msgs:?}"
    );
}

#[test]
fn test_class_extends_inherits_instance_members_via_symbol_path() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    value!: T;
}

class Mid<T> extends Base<T> {}

class Derived extends Mid<string> {}

const ok: string = new Derived().value;
const bad: number = new Derived().value;
        "#,
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected TS2322 for assigning inherited string member to number.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2506),
        "Did not expect circular-base TS2506 in linear inheritance.\nActual diagnostics: {diagnostics:#?}"
    );
}
