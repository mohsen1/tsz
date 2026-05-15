use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

/// Alias: default options already have `strict_null_checks: true`.
fn check_source_with_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source_diagnostics(source)
}

fn check_source_without_strict_null(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn mapped_type_template_key_constraint_argument_displays_remapped_shape() {
    let diagnostics = check_source_with_strict_null(
        r#"
function foo<T extends { [K in keyof T as `${Extract<K, string>}y`]: number }>(foox: T) { }

const c = { x: 1 };

foo(c);
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains(
            "Argument of type '{ x: 1; }' is not assignable to parameter of type '{ xy: number; }'."
        ),
        "expected remapped constraint shape in TS2345, got: {diag:?}"
    );
}

#[test]
fn correlated_mapped_handler_call_does_not_report_never_parameter() {
    let diagnostics = check_source_with_strict_null(
        r#"
type TypeMap = {
    foo: string,
    bar: number
};

type Keys = keyof TypeMap;
type HandlerMap = { [P in Keys]: (x: TypeMap[P]) => void };

declare const handlers: HandlerMap;

type DataEntry<K extends Keys = Keys> = { [P in K]: {
    type: P,
    data: TypeMap[P]
}}[K];

function process<K extends Keys>(data: DataEntry<K>[]) {
    data.forEach(block => {
        handlers[block.type](block.data);
    });
}
"#,
    );

    assert!(
        diagnostics.iter().all(|diag| diag.code != 2345),
        "Expected no TS2345 for correlated handler call. Got: {diagnostics:?}"
    );
}

#[test]
fn mapped_parameter_property_mismatch_displays_instantiated_property_slice() {
    let mut parser = tsz_parser::parser::ParserState::new("test.ts".to_string(), String::new());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let property_name = checker.ctx.types.intern_string("y");
    let target_property_type = checker
        .ctx
        .types
        .union2(tsz_solver::TypeId::NUMBER, tsz_solver::TypeId::UNDEFINED);
    let reason = tsz_solver::SubtypeFailureReason::PropertyTypeMismatch {
        property_name,
        source_property_type: tsz_solver::TypeId::STRING,
        target_property_type,
        nested_reason: None,
    };

    let display = checker
        .mapped_property_mismatch_parameter_display(
            "{ [x in K]?: Lower<T>[] | undefined; }",
            Some(&reason),
        )
        .expect("expected mapped property display rewrite");

    assert_eq!(display, "{ y?: number | undefined; }");
}

#[test]
fn generic_call_parameter_display_trims_unmatched_trailing_type_arg_close() {
    let diagnostics = check_source_with_strict_null(
        r#"
declare class StateNode<TContext, in out TEvent extends { type: string }> {
    _storedEvent: TEvent;
    _action: ActionObject<TEvent>;
    _state: StateNode<TContext, any>;
}

interface ActionObject<TEvent extends { type: string }> {
    exec: (meta: StateNode<any, TEvent>) => void;
}

declare function createMachine<TEvent extends { type: string }>(action: ActionObject<TEvent>): StateNode<any, any>;
declare const qq: ActionObject<{ type: "PLAY"; value: number }>;

createMachine<{ type: "PLAY"; value: number } | { type: "RESET" }>(qq);
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains(
            "parameter of type 'ActionObject<{ type: \"PLAY\"; value: number; } | { type: \"RESET\"; }>'."
        ),
        "expected balanced ActionObject target display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("}>>'."),
        "TS2345 target display must not include an unmatched trailing `>`: {diag:?}"
    );
}

#[test]
fn emits_ts2721_for_calling_null() {
    let diagnostics = check_source_with_strict_null("null();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2721),
        "Expected TS2721 for `null()`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2722_for_calling_undefined() {
    let diagnostics = check_source_with_strict_null("undefined();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2722),
        "Expected TS2722 for `undefined()`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2723_for_calling_null_or_undefined() {
    let diagnostics = check_source_with_strict_null("let f: null | undefined;\nf();");
    assert!(
        diagnostics.iter().any(|d| d.code == 2723),
        "Expected TS2723 for calling `null | undefined`, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts2349_without_strict_null_checks() {
    // Without strictNullChecks, null/undefined are in every type's domain,
    // so we should get TS2349 (not callable) instead of TS2721/2722/2723.
    let diagnostics = check_source_without_strict_null("null();");
    let has_2349 = diagnostics.iter().any(|d| d.code == 2349);
    let has_272x = diagnostics.iter().any(|d| (2721..=2723).contains(&d.code));
    assert!(
        has_2349 && !has_272x,
        "Expected TS2349 (not TS272x) without strictNullChecks, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn emits_ts6234_not_ts2721_for_generic_getter_returning_null() {
    // When a generic class has a getter that returns null, calling it should
    // emit TS6234 (not callable because it's a get accessor), not TS2721
    // (cannot invoke object which is possibly null). The getter accessor
    // diagnostic takes priority over the nullish diagnostic.
    let diagnostics = check_source_with_strict_null(
        r#"
class C<T, U> {
    x: T;
    get y() {
        return null;
    }
    set y(v: U) { }
    fn() { return this; }
    constructor(public a: T, private b: U) { }
}
var c = new C(1, '');
var r6 = c.y();
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6234),
        "Expected TS6234 for calling getter `c.y()`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2721),
        "Should NOT emit TS2721 for calling getter on generic class, got: {codes:?}"
    );
}

#[test]
fn tdz_callee_still_checks_argument_type_against_declared_signature() {
    let diagnostics =
        check_source_with_strict_null("f(true);\nconst f: (a: number) => void = null as any;");
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2448),
        "expected TDZ diagnostic for forward const call, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2345
                && d.message_text.contains(
                    "Argument of type 'boolean' is not assignable to parameter of type 'number'.",
                )
        }),
        "expected TS2345 from recovered declared callee signature, got: {diagnostics:?}"
    );
}

#[test]
fn generic_optional_array_parameter_diagnostic_uses_array_shorthand() {
    let diagnostics = check_source_with_strict_null(
        r#"
interface Utils {
   fold<T, S>(c: Array<T>, folder?: (s: S, t: T) => T, init?: S): T;
}

declare var utils: Utils;

utils.fold();
utils.fold(null);
utils.fold(null, null);
utils.fold(null, null, null);
"#,
    );

    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        3,
        "expected three TS2345 diagnostics, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2554),
        "expected TS2554 for the zero-argument call, got: {diagnostics:#?}"
    );
    assert!(
        ts2345
            .iter()
            .all(|d| d.message_text.contains("parameter of type 'unknown[]'")),
        "expected TS2345 parameter display to use `unknown[]`, got: {ts2345:#?}"
    );
    assert!(
        ts2345
            .iter()
            .all(|d| !d.message_text.contains("Array<unknown>")),
        "TS2345 parameter display should not use `Array<unknown>`, got: {ts2345:#?}"
    );
}

#[test]
fn tdz_callee_still_checks_minimum_argument_count() {
    let diagnostics = check_source_with_strict_null(
        "b();\ntype Test = (arg: unknown) => arg is string;\nconst b: Test = null as any;",
    );

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == 2554 && d.message_text == "Expected 1 arguments, but got 0."),
        "expected TS2554 from recovered declared callee signature, got: {diagnostics:?}"
    );
}

#[test]
fn emits_ts6234_for_non_generic_getter_call() {
    // Non-generic class: calling a getter should emit TS6234
    let diagnostics = check_source_with_strict_null(
        r#"
class C {
    x: string;
    get y() {
        return 1;
    }
    set y(v) { }
    constructor(public a: number, private b: number) { }
}
var c = new C(1, 2);
var r6 = c.y();
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&6234),
        "Expected TS6234 for calling getter `c.y()`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2721) && !codes.contains(&2349),
        "Should NOT emit TS2721 or TS2349 for getter call, got: {codes:?}"
    );
}

#[test]
fn emits_ts2722_for_optional_method_call() {
    // When an optional method is called without optional chaining,
    // its type includes undefined, so TS2722 should be emitted.
    let diagnostics = check_source_with_strict_null(
        r#"
interface Foo {
    optionalMethod?(x: number): string;
}
declare let foo: Foo;
foo.optionalMethod(1);
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2722),
        "Expected TS2722 for calling optional method without ?., got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2345_argument_mismatch_anchors_argument_node() {
    let source = r#"
declare function takes(value: string): void;
takes(123);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    let arg_start = source.find("123").expect("expected argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2345 should anchor at the argument"
    );
    assert_eq!(diag.length, 3, "TS2345 should cover only the argument span");
}

#[test]
fn ts2345_zero_argument_never_rest_call_uses_empty_tuple_display() {
    let source = r#"
declare let foo: (...args: never) => void;
foo();
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("Argument of type '[]' is not assignable to parameter of type 'never'."),
        "Expected empty argument list display for zero-argument never-rest call, got: {diag:?}"
    );
}

#[test]
fn ts2345_contextual_callback_display_preserves_explicit_alias_annotations() {
    let source = r#"
type ClassNameObject = { [key: string]: boolean | undefined };
declare function reduceClassNameObject(
    cb: (obj: ClassNameObject, key: string) => ClassNameObject,
): void;

export function css<S extends { [K in keyof S]: string }>(styles: S): string {
  reduceClassNameObject((obj: ClassNameObject, key: keyof S) => {
    const exportedClassName = styles[key];
    obj[exportedClassName] = true;
    return obj;
  });
  return "";
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("Argument of type '(obj: ClassNameObject, key: keyof S) => ClassNameObject'"),
        "Expected source callback display to preserve explicit alias annotations, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("parameter of type '(obj: ClassNameObject, key: string) => ClassNameObject'"),
        "Expected target callback display to preserve instantiated alias annotations, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("error"),
        "Callback display should not collapse explicit annotations to `error`, got: {diag:?}"
    );
}

#[test]
fn ts2345_callback_target_display_preserves_unresolved_qualified_type_name() {
    let source = r#"
declare function readdir(
    accept: (stat: fs.Stats, name: string) => boolean,
): void;
readdir(() => {});
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("parameter of type '(stat: fs.Stats, name: string) => boolean'"),
        "Expected unresolved qualified annotation to keep its source name, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("stat: error"),
        "Unresolved qualified annotation should not display as `error`, got: {diag:?}"
    );
}

#[test]
fn ts2345_object_literal_contextual_typing_ignores_object_prototype_members() {
    let source = r#"
interface I {
    value: string;
    toString: (t: string) => string;
}
declare function f2(args: I): void;
f2({ value: '' });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics when only Object.prototype members are missing, got: {diagnostics:?}"
    );
}

#[test]
fn ts2345_object_literal_contextual_typing_still_reports_real_missing_property() {
    let source = r#"
interface I {
    value: string;
    toString: (t: string) => string;
}
declare function f2(args: I): void;
f2({ toString: (s: string) => s });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected TS2345 when a real required property is missing, got: {diagnostics:?}"
    );
}

#[test]
fn ts2345_generic_call_parameter_display_preserves_instantiated_alias_name() {
    let source = r#"
namespace Underscore {
    export interface Iterator<T, U> {
        (value: T, index: any, list: any): U;
    }

    export interface Static {
        all<T>(list: T[], iterator?: Iterator<T, boolean>, context?: any): boolean;
        identity<T>(value: T): T;
    }
}

declare var _: Underscore.Static;
_.all([true, 1, null, 'yes'], _.identity);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text
            .contains("parameter of type 'Iterator<string | number | boolean | null, boolean>'"),
        "Expected instantiated alias name in parameter display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("parameter of type '(value:"),
        "Parameter display should not expand the iterator alias, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_parameter_display_preserves_semantic_nullable_union() {
    let source = r#"
declare function takes(value: boolean | null | undefined): void;
takes(0);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type '0'"),
        "TS2345 should preserve direct literal call-argument display, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("parameter of type 'boolean | null | undefined'"),
        "TS2345 should preserve semantic nullable unions in parameter display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("parameter of type 'boolean'."),
        "TS2345 should not strip nullable union members from non-optional parameters, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_normalizes_negative_zero_literal() {
    let source = r#"
declare function takes(value: boolean | null | undefined): void;
takes(-0);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type '0'"),
        "TS2345 should normalize -0 to 0 in literal call-argument display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '-0'"),
        "TS2345 should not preserve -0 text once the literal type normalizes to 0, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_non_union_target() {
    let source = r#"
declare function takes(value: string): void;
takes(2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'number'"),
        "TS2345 should widen direct numeric literals for non-union parameter targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '2'"),
        "TS2345 should not preserve literal numeric text for non-union parameter targets, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_object_with_null_target() {
    // When the target union mixes a non-primitive (`object`) with `null`/`undefined`,
    // tsc widens the literal call argument (`1` → `number`). The literal-preservation
    // path is reserved for unions whose members are all primitives — `object | null`
    // is not "literal sensitive" so the source should display as the widened form.
    let source = r#"
declare function takes(value: object | null): void;
takes(1);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'number'"),
        "TS2345 should widen direct numeric literals when the union target mixes a non-primitive with null, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '1'"),
        "TS2345 should not preserve literal numeric text against a non-literal-sensitive union, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_object_target_param_name_independent() {
    // Same rule, with a different name choice for the parameter — locks that
    // the rule is purely structural (target union has a non-primitive member),
    // not tied to any user-chosen identifier.
    let source = r#"
declare function f(o: object | null): void;
f("hello");
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'string'"),
        "TS2345 should widen string literals against object | null target, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '\"hello\"'"),
        "TS2345 should not preserve literal string text against object | null, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_parameter_display_strips_null_for_non_primitive_union() {
    // When the target union mixes a non-primitive member (`object`) with
    // `null`, tsc shows just the non-primitive part in the TS2345 target
    // display. The structural rule is "strip nullish only when the remaining
    // member set contains at least one non-primitive type."
    let source = r#"
declare function takes(value: object | null): void;
takes(1);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("parameter of type 'object'"),
        "TS2345 should strip `null` from `object | null` when target reduces to a non-primitive, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("parameter of type 'object | null'"),
        "TS2345 should not preserve `object | null` in target display, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_parameter_display_strips_null_param_name_independent() {
    // Locks the rule is structural — using a different parameter name produces
    // the same target display.
    let source = r#"
declare function g(thing: object | null): void;
g(false);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("parameter of type 'object'"),
        "TS2345 should strip `null` from non-primitive union regardless of parameter name, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_literal_for_optional_parameter_target() {
    let source = r#"
interface Item {
    name: string;
}
declare function takes(value?: Item): void;
takes("abc");
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'string'"),
        "TS2345 should widen direct literals for optional parameter targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type '\"abc\"'"),
        "TS2345 should not preserve literal text when the union comes only from optionality, got: {diag:?}"
    );
}

#[test]
fn ts2345_call_argument_display_widens_boolean_literal_for_non_boolean_union_target() {
    let source = r#"
declare function takes(...value: (number | string)[]): void;
takes(1, 2, "hello", true);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("Argument of type 'boolean'"),
        "TS2345 should widen boolean literals for non-boolean union targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type 'true'"),
        "TS2345 should not preserve boolean literal text for non-boolean union targets, got: {diag:?}"
    );
}

#[test]
fn ts2345_array_literal_call_argument_display_widens_boolean_literal_element() {
    let source = r#"
declare const test1:
  | ((...args: [a: string | number, b: number | boolean]) => void)
  | ((...args: [c: number | boolean, d: string | boolean]) => void);

test1(42, [true]);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains(
            "Argument of type 'boolean[]' is not assignable to parameter of type 'boolean'."
        ),
        "TS2345 should widen boolean literal array elements for non-literal targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type 'true[]'"),
        "TS2345 should not preserve boolean literal array elements here, got: {diag:?}"
    );
}

#[test]
fn ts2345_array_literal_tuple_overflow_elaborates_element_mismatch_to_ts2322() {
    let source = r#"
function a5([a, b, [[c]]]) { }
a5([1, 2, "string", false, true]);
a5([1, 2]);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| {
            d.code == 2322
                && d.message_text
                    .contains("Type 'string' is not assignable to type '[[any]]'.")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected element-level TS2322 for overflowing tuple literal call, got: {diagnostics:?}"
            )
        });
    assert!(
        ts2322.start > 0,
        "expected TS2322 to anchor to the mismatched element"
    );

    let has_outer_overflow_ts2345 = diagnostics.iter().any(|d| {
        d.code == 2345
            && d.message_text
                .contains("Argument of type '[number, number, string, false, true]'")
    });
    assert!(
        !has_outer_overflow_ts2345,
        "should suppress outer TS2345 when tuple overflow literal has a concrete element mismatch, got: {diagnostics:?}"
    );

    let has_short_tuple_ts2345 = diagnostics.iter().any(|d| {
        d.code == 2345
            && d.message_text
                .contains("Argument of type '[number, number]'")
    });
    assert!(
        has_short_tuple_ts2345,
        "should still report TS2345 for the short tuple argument, got: {diagnostics:?}"
    );
}

#[test]
fn ts2322_array_literal_elaboration_widens_destructuring_default_sources() {
    let source = r#"
function b6([a, z, y] = [undefined, null, undefined]) { }
b6(["string", 1, 2]);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();

    for expected in [
        "Type 'string' is not assignable to type 'undefined'.",
        "Type 'number' is not assignable to type 'null'.",
        "Type 'number' is not assignable to type 'undefined'.",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "expected widened TS2322 message `{expected}`, got: {messages:#?}"
        );
    }

    assert!(
        messages.iter().all(|message| {
            !message.contains("Type '\"string\"'")
                && !message.contains("Type '1'")
                && !message.contains("Type '2'")
        }),
        "array literal elaboration should not preserve literal source display here, got: {messages:#?}"
    );
}

#[test]
fn ts2322_array_literal_elaboration_preserves_same_primitive_literal_targets() {
    let diagnostics = check_source_with_strict_null(
        r#"
function takesLiteral(value: ["a"]) { }
takesLiteral(["b"]);
"#,
    );
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();

    assert!(
        messages
            .iter()
            .any(|message| message.contains("Type '\"b\"' is not assignable to type '\"a\"'.")),
        "same-primitive literal targets should preserve the source literal, got: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.contains("Type 'string' is not assignable to type '\"a\"'.")),
        "same-primitive literal targets should not widen to the primitive source, got: {messages:#?}"
    );
}

#[test]
fn inferred_generic_call_suppresses_ts2345_when_other_argument_is_error() {
    let source = r#"
declare let anythingAny: any;
function foo1<T extends number>(...a: T[]) { }
foo1(1, 2, "string", anythingAny);
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().all(|d| d.code != 2345),
        "inferred generic call should suppress cascading TS2345 when another argument is error-like (`any`), got: {diagnostics:?}"
    );
}

#[test]
fn explicit_generic_call_keeps_ts2345_with_other_error_arguments() {
    let source = r#"
declare let anythingAny: any;
function foo1<T extends number>(...a: T[]) { }
foo1<number>(1, "string", anythingAny);
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "explicit generic instantiation should still report TS2345 for mismatched typed arguments even with `any` arguments, got: {diagnostics:?}"
    );
}

#[test]
fn ts2322_optional_function_property_target_display_omits_synthetic_undefined() {
    // Note: the original repro used `Promise<number[]>` / `Promise<string>`
    // for the property return types. Without lib loaded that lowers to
    // `Application(UnresolvedTypeName('Promise'), [...])`, which is now
    // recognised as an error type by `is_error_type` and short-circuits
    // the assignability check. To keep
    // exercising the optional-property display invariant without relying
    // on the cascading-error path, we substitute a locally declared
    // generic alias so the property types are fully resolved.
    let source = r#"
type Box<T> = { value: T };

interface Stuff {
    a?: () => Box<number[]>;
    b: () => Box<string>;
}

function foo(): Stuff | string {
    return {
        a() { return [123] },
        b: () => "hello",
    }
}
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");

    assert!(
        diag.message_text.contains("type '() =>"),
        "Expected optional property diagnostic to keep the non-nullish callable target, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("| undefined"),
        "Optional property mismatch should not inject synthetic undefined, got: {diag:?}"
    );
}

#[test]
fn object_literal_call_argument_uses_shared_epc_rules_for_generic_intersections() {
    let source = r#"
declare function take<T>(value: { nested: T & { a: number } }): void;
take({ nested: { a: 1, extra: 2 } });
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "generic intersections should capture extra nested properties without TS2353/TS2345, got: {diagnostics:?}"
    );
}

#[test]
fn contextual_object_literal_assertion_does_not_emit_early_excess_property_errors() {
    let source = r#"
var foo = <{ id: number; }> { id: 4, name: "as" };
"#;

    let diagnostics = check_source_with_strict_null(source);
    assert!(
        diagnostics.is_empty(),
        "type assertions should not emit early object-literal TS2353 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn ts2554_excess_argument_span_starts_at_first_excess_argument() {
    let source = r#"
declare function takes(): void;
takes(1, 2);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2554)
        .expect("expected TS2554");

    let first_excess = source.find("1, 2").expect("expected excess arguments") as u32;
    assert_eq!(
        diag.start, first_excess,
        "TS2554 should start at the first excess argument"
    );
    assert_eq!(
        diag.length, 4,
        "TS2554 should cover the contiguous excess-argument span"
    );
}

#[test]
fn ts2345_object_literal_argument_shows_widened_property_types() {
    // tsc shows widened types in TS2345 messages: `{ e: number; m: number }`
    // not `{ e: 1; m: 1 }`. This matches tsc's behavior of widening fresh
    // object literal types in assignability error messages.
    let source = r#"
declare function foo(x: string): void;
foo({ e: 1, m: 1 });
"#;

    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains("{ e: number; m: number; }"),
        "TS2345 should show widened property types (number not 1). Got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("{ e: 1"),
        "TS2345 should NOT show literal property types. Got: {}",
        diag.message_text
    );
}

#[test]
fn ts2345_missing_property_keeps_literal_key_but_widens_object_type_arg() {
    let source = r#"
interface ListProps<T, K extends keyof T> {
    items: T[];
    itemKey: K;
    prop: number;
}
declare const Component: <T, K extends keyof T>(x: ListProps<T, K>) => void;
Component({ items: [{ name: ' string' }], itemKey: 'name' });
"#;

    let diagnostics = check_source_with_strict_null(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "missing required property should suppress per-property TS2322, got: {diagnostics:?}"
    );
    let message = &ts2345[0].message_text;
    assert!(
        message.contains("Argument of type '{ items: { name: string; }[]; itemKey: \"name\"; }'"),
        "TS2345 source display should widen nested object literals but preserve literal key, got: {message}"
    );
    assert!(
        message.contains("parameter of type 'ListProps<{ name: string; }, \"name\">'"),
        "TS2345 target display should widen the inferred object type argument, got: {message}"
    );
}

#[test]
fn ts2345_explicit_type_args_display_uses_correct_overload() {
    // When calling an overloaded method with explicit type arguments, the error
    // message should display the parameter type from the overload whose type
    // parameter count matches the explicit type arguments, not the first overload.
    // Bug: `_.map<number, string, Date>(c2, rf1)` showed `=> any` (from the 2-param
    // overload) instead of `=> Date` (from the 3-param overload).
    let source = r#"
interface Pair<A, B> { first: A; second: B; }

interface Combinators {
    map<T, U>(c: Pair<T, U>, f: (x: T, y: U) => any): Pair<any, any>;
    map<T, U, V>(c: Pair<T, U>, f: (x: T, y: U) => V): Pair<T, V>;
}

declare var _: Combinators;
declare var c2: Pair<number, string>;
var rf1 = (x: number, y: string): string => { return "hello" };
var r5b = _.map<number, string, boolean>(c2, rf1);
"#;

    let diagnostics = check_source_with_strict_null(source);
    let codes: Vec<(u32, &str)> = diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.as_str()))
        .collect();
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| panic!("expected TS2345, got: {codes:?}"));

    assert!(
        diag.message_text
            .contains("parameter of type '(x: number, y: string) => boolean'"),
        "Expected the 3-param overload's parameter type with boolean, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("=> any"),
        "Should not show => any from the wrong overload, got: {}",
        diag.message_text
    );
}

#[test]
fn ts2322_anchors_at_arrow_body_when_assigning_to_generic_function_type_alias() {
    // Regression test for elaboration in `try_elaborate_assignment_source_error`:
    // when assigning an expression-bodied arrow to a *direct* generic function-
    // type target (e.g. `EnvFunction = <T>() => T`), tsc anchors the TS2322 at
    // the body expression and reports the body type vs the target's generic
    // return type. Previously, `try_elaborate_function_arg_return_error` skipped
    // elaboration whenever the expected return type contained type parameters,
    // which is correct during generic-call inference but wrong for direct
    // assignment (where the type parameter is bound by the target's own
    // signature, not by an outer call's inference state).
    let diagnostics = check_source_with_strict_null(
        r#"
type EnvFunction = <T>() => T;
declare const simple: string | number;
const env: EnvFunction = () => simple;
"#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for arrow body return-type mismatch, got: {codes:?}"
    );

    // The TS2322 should be anchored at the body expression `simple`, not on the
    // outer `const env = ...` declaration.
    let outer_anchor = "const env: EnvFunction = () => simple;";
    let body_anchor = "simple";
    let source_text_offset_of = |pat: &str| {
        let s = "
type EnvFunction = <T>() => T;
declare const simple: string | number;
const env: EnvFunction = () => simple;
";
        s.find(pat).expect("substring exists in fixture")
    };
    let body_offset = {
        // The fixture references `simple` twice (declare + arrow body).
        // Use the second occurrence (the arrow body).
        let s = "
type EnvFunction = <T>() => T;
declare const simple: string | number;
const env: EnvFunction = () => simple;
";
        let first = s.find(body_anchor).unwrap();
        first + 1 + s[first + 1..].find(body_anchor).unwrap()
    };
    let outer_offset = source_text_offset_of(outer_anchor);

    let ts2322_starts: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.start)
        .collect();
    assert!(
        ts2322_starts.iter().any(|&s| s as usize == body_offset),
        "Expected at least one TS2322 anchored at arrow body `simple` (offset \
         {body_offset}), got starts: {ts2322_starts:?}"
    );
    assert!(
        !ts2322_starts.iter().all(|&s| s as usize == outer_offset),
        "TS2322 should not anchor only on the outer declaration (offset \
         {outer_offset}), got: {ts2322_starts:?}"
    );
}

#[test]
fn ts2322_skips_arrow_body_elaboration_during_generic_call_inference() {
    // Guard the negative case: during generic call inference, the expected
    // callback return type may still reference uninstantiated type parameters
    // (e.g., `B` from `compose<A, B, C>`). Elaborating against such placeholders
    // would produce false TS2322s like "Type 'T[]' is not assignable to type
    // 'B'". `try_elaborate_function_arg_return_error_with_options` retains the
    // unresolved-holes skip on the call path (`allow_unresolved_holes = false`).
    //
    // This test exercises a generic pipe-style helper similar to
    // genericContextualTypes1: passing arrow callbacks into a generic call
    // should not produce spurious body-level TS2322 diagnostics on the
    // callbacks' identifier bodies.
    let diagnostics = check_source_with_strict_null(
        r#"
declare function pipe<A, B, C>(ab: (a: A) => B, bc: (b: B) => C): (a: A) => C;
declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };
const f1 = pipe(list, box);
"#,
    );
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        0,
        "Generic call inference must not produce body-level TS2322s, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pipe_no_false_ts2345_on_concrete_arg_against_unresolved_callable_param() {
    // `pipe(() => true, b => 42)` should emit no errors.
    // The first argument `() => true` is a concrete callable; the expected type
    // `(...args: A) => B` has type params A, B from pipe's inference context.
    // When A and B are still unresolved, we must defer (not report TS2345 or TS2322).
    let diagnostics = check_source_with_strict_null(
        r#"
declare function pipe<A extends any[], B, C>(f: (...args: A) => B, g: (x: B) => C): (...args: A) => C;
let g5 = pipe(() => true, b => 42);
let g6 = pipe(x => "hello", s => s.length);
let g8 = pipe((x: number, y: string) => 42, x => "" + x);
"#,
    );
    let false_positives: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .collect();
    assert!(
        false_positives.is_empty(),
        "pipe() with concrete callback against generic params must emit no TS2322/TS2345, got: {:?}",
        false_positives
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pipe_contextual_return_refines_overload_callbacks_progressively() {
    let diagnostics = check_source_with_strict_null(
        r#"
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;
type Fn = (n: number) => number;
const fn30: Fn = pipe(
    x => x + 1,
    x => x * 2,
);
"#,
    );
    let false_positives: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2362)
        .collect();
    assert!(
        false_positives.is_empty(),
        "Contextual pipe overload should refine B from the first callback before checking the second, got: {:?}",
        false_positives
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nested_generic_call_callee_receives_outer_call_context() {
    let diagnostics = check_source_with_strict_null(
        r#"
declare function map<T, U>(transform: (t: T) => U): (arr: T[]) => U[];
declare const identity: <T>(value: T) => T;
const arr1: string[] = map(identity)(['a']);
"#,
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Nested generic callee should infer string[] from the outer call arguments/context, got: {:?}",
        ts2322
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pipe_concrete_callback_holes_only_in_expected_no_ts2322_in_body() {
    // Specifically guards against the arrow-body TS2322 false positive.
    // `() => true` against `(...args: A) => B` should NOT elaborate into the body
    // and emit TS2322 "Type 'boolean' is not assignable to type 'B'".
    let diagnostics = check_source_with_strict_null(
        r#"
declare function pipe<A extends any[], B, C>(f: (...args: A) => B, g: (x: B) => C): (...args: A) => C;
let g5 = pipe(() => true, b => 42);
"#,
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Arrow body must not be elaborated to TS2322 when expected return type has unresolved type params from outer context, got: {:?}",
        ts2322.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn contextual_generic_callback_preserves_inner_call_argument_mismatch() {
    let diagnostics = check_source(
        r#"
type Values<T> = T[keyof T];
type EventObject = { type: string };

interface ActorLogic<TEvent extends EventObject> {
  transition: (ev: TEvent) => unknown;
}

type UnknownActorLogic = ActorLogic<never>;

interface ProvidedActor {
  src: string;
  logic: UnknownActorLogic;
}

interface ActionFunction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface AssignAction<TActor extends ProvidedActor> {
  (): void;
  _out_TActor?: TActor;
}

interface MachineConfig<TActor extends ProvidedActor> {
  entry?: ActionFunction<TActor>;
}

declare function assign<TActor extends ProvidedActor>(
  _: (spawn: (actor: TActor["src"]) => void) => {},
): AssignAction<TActor>;

type ToProvidedActor<TActors extends Record<string, UnknownActorLogic>> =
  Values<{
    [K in keyof TActors & string]: {
      src: K;
      logic: TActors[K];
    };
  }>;

declare function setup<
  TActors extends Record<string, UnknownActorLogic> = {},
>(implementations?: {
  actors?: { [K in keyof TActors]: TActors[K] };
}): {
  createMachine: <
    const TConfig extends MachineConfig<ToProvidedActor<TActors>>,
  >(
    config: TConfig,
  ) => void;
};

declare const counterLogic: ActorLogic<{ type: "INCREMENT" }>;

setup({
  actors: { counter: counterLogic },
}).createMachine({
  entry: assign((spawn) => {
    spawn("counter");
    spawn("alarm");
    return {};
  }),
});
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345 for the inner spawn call, got: {diagnostics:#?}"
    );
    let message = &ts2345[0].message_text;
    assert!(
        message.contains(
            "Argument of type '\"alarm\"' is not assignable to parameter of type '\"counter\"'."
        ),
        "expected inner literal mismatch, got: {message}"
    );
    assert!(
        !message.contains("(spawn:"),
        "outer callback mismatch must be suppressed when the inner call explains the failure, got: {message}"
    );
}

#[test]
fn ts2345_widens_literals_when_constraint_chains_to_unconstrained_type_param() {
    // For `<T, U extends T>(x: T, y: U)` called with two fresh literal
    // arguments that widen to different primitives (e.g. `foo(1, '')`),
    // tsc widens both sides of the TS2345 diagnostic to their primitive
    // bases, regardless of whether the return annotation exposes `U`.
    //
    // Use two name choices for the bound variables to guard against
    // hardcoded-name regressions (per the anti-hardcoding directive).
    for (t_name, u_name, return_type) in &[
        ("T", "U", "U"),
        ("T", "U", "void"),
        ("X", "P", "P"),
        ("X", "P", "void"),
    ] {
        let source = format!(
            "function foo<{t_name}, {u_name} extends {t_name}>\
             (x: {t_name}, y: {u_name}): {return_type} {{ return y as any; }}\nfoo(1, '');\n"
        );
        let diagnostics = check_source_with_strict_null(&source);
        let diag = diagnostics
            .iter()
            .find(|d| d.code == 2345)
            .unwrap_or_else(|| {
                panic!("expected TS2345 for {t_name}/{u_name} variant, got: {diagnostics:?}")
            });
        assert!(
            diag.message_text.contains(
                "Argument of type 'string' is not assignable to parameter of type 'number'."
            ),
            "TS2345 must widen both source and target to their primitive bases when the \
             type parameter's declared-constraint chain ends at an unconstrained type \
             parameter ({t_name}/{u_name} variant), got: {diag:?}"
        );
    }
}

#[test]
fn ts2345_preserves_literals_when_type_param_has_no_declared_constraint() {
    // Inverse of the widening case above: for `<T>(x: T, y: T)` called
    // with two fresh literal arguments whose primitive bases differ
    // (`foo(1, '')`), tsc preserves the literal candidate display
    // (`'""' / '1'`). T has no declared constraint at all, so the
    // first-wins inference picks `1` and the literal `''` is reported
    // against it without widening.
    let source = r#"
function foo<T>(x: T, y: T): T { return y; }
foo(1, '');
"#;
    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345 for unconstrained type parameter");
    assert!(
        diag.message_text
            .contains("Argument of type '\"\"' is not assignable to parameter of type '1'."),
        "TS2345 must preserve literal candidate display when the type parameter has no \
         declared constraint, got: {diag:?}"
    );
}

#[test]
fn ts2345_widens_unconstrained_implementation_param_hidden_from_return() {
    // `fixTypeParameterInSignatureWithRestParameters.ts`: for an implemented
    // generic function whose type parameter is not exposed through the return
    // type, tsc widens the second fresh literal candidate after the first
    // argument fixes the primitive base.
    let source = r#"
function bar<T>(item1: T, item2: T) { }
bar(1, "");
"#;
    let diagnostics = check_source_with_strict_null(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345 for unconstrained implementation parameter");
    assert!(
        diag.message_text
            .contains("Argument of type 'string' is not assignable to parameter of type 'number'."),
        "TS2345 must widen source and target when the implementation signature hides the \
         unconstrained type parameter from its return surface, got: {diag:?}"
    );
}

#[test]
fn ts2322_skips_arrow_body_elaboration_for_object_property_in_generic_call() {
    // Guard against the indirect-caller variant of the unresolved-holes regression:
    // when the arrow appears as a property value inside an object literal that is
    // itself an argument to a generic call, `try_elaborate_assignment_source_error`
    // is invoked from `try_elaborate_object_literal_properties_with_source` with a
    // `target_prop_type` that still contains uninstantiated type parameters (`U`
    // here). The new arrow interception must skip in this case to preserve the
    // unresolved-holes guard the call-argument path relies on.
    let diagnostics = check_source_with_strict_null(
        r#"
declare function foo<T, U>(opts: { transform: (x: T) => U }): U;
const r = foo({ transform: (x: string) => x.length });
"#,
    );
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count,
        0,
        "Arrow inside object-literal arg of generic call must not raise TS2322 \
         from indirect elaboration, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
