use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_diagnostics, check_with_options};

#[test]
fn pick_rejects_unconstrained_and_broad_key_type_parameters() {
    let source = r#"
interface Shape {
    name: string;
    width: number;
}
interface Named {
    name: string;
}

type Pick<T, K extends keyof T> = { [P in K]: T[P] };

function f1<T>() {
    let y: Pick<Shape, T>;
}

function f2<T extends string | number>() {
    let y: Pick<Shape, T>;
}

function f3<T extends keyof Shape>() {
    let y: Pick<Shape, T>;
}

function f4<T extends keyof Named>() {
    let y: Pick<Shape, T>;
}
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2344)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        ts2344.len(),
        2,
        "expected TS2344 only for unconstrained T and T extends string | number, got: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .all(|message| message.contains("does not satisfy the constraint 'keyof Shape'")),
        "TS2344 should point at the Pick key constraint: {ts2344:#?}"
    );
}

#[test]
fn invalid_mapped_key_alias_reports_definition_error_not_assignment_cascade() {
    let source = r#"
type Foo2<T, F extends keyof T> = {
    pf: { [P in F]?: T[P] },
    pt: { [P in T]?: T[P] },
};
type O = { x: number, y: boolean };
let f: Foo2<O, "x"> = {
    pf: { x: 7 },
    pt: { x: 7, y: false },
};
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2322
            && diag
                .message_text
                .contains("is not assignable to type 'string | number | symbol'")),
        "expected the invalid mapped key type diagnostic at the alias definition, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2322
            && diag
                .message_text
                .contains("is not assignable to type '{ [P in O]?: O[P] | undefined; }'")),
        "invalid mapped key aliases should not cascade into assignment TS2322: {diagnostics:#?}"
    );
}

#[test]
fn record_key_constraint_displays_primitive_key_union() {
    let source = r#"
type AudioData = string | number | symbol;
type Record<K extends keyof any, T> = { [P in K]: T };
type T = Record<object, number>;
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2344)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert!(
        ts2344
            .iter()
            .any(|message| message.contains("constraint 'PropertyKey'")),
        "Record's key constraint should display PropertyKey, got: {diagnostics:#?}"
    );
    assert!(
        ts2344.iter().all(|message| !message.contains("AudioData")),
        "Record's key constraint must not be repainted by unrelated lib names: {diagnostics:#?}"
    );
}

#[test]
fn pick_rejects_broad_key_type_parameter_by_itself() {
    let source = r#"
interface Shape {
    name: string;
    width: number;
}

type Pick<T, K extends keyof T> = { [P in K]: T[P] };

function f2<T extends string | number>() {
    let y: Pick<Shape, T>;
}
"#;

    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2344),
        "expected TS2344 for T extends string | number against keyof Shape, got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_types_check_named_property_values_before_string_index_fallback() {
    // Locks in TS2322 for plain target + Partial application. The bare
    // homomorphic-mapped target (`{ [P in keyof T2]: T2[P] }`) is a known
    // follow-up: target_is_mapped_or_mapped_application doesn't currently
    // route through the new named-property check for that shape — see
    // mapped_object_literals.rs::target_is_mapped_or_mapped_application.
    let source = r#"
type T2 = { a?: number, [key: string]: any };
type Partial<T> = { [P in keyof T]?: T[P] };

let x1: T2 = { a: 'no' };
let x2: Partial<T2> = { a: 'no' };
"#;

    let diagnostics = check_source_diagnostics(source);
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .map(|diag| diag.message_text.as_str())
        .collect();

    assert_eq!(
        messages.len(),
        2,
        "expected TS2322 for plain and Partial mapped targets, got: {diagnostics:#?}"
    );
    assert!(
        messages
            .iter()
            .all(|message| message.contains("Type 'string' is not assignable to type 'number'")),
        "named property diagnostics should use the explicit property type, got: {messages:#?}"
    );
}

#[test]
fn pick_preserves_optional_property_undefined_for_present_assignment() {
    let source = r#"
interface Foo {
    a: string;
    b?: number;
}

type Pick<T, K extends keyof T> = { [P in K]: T[P] };

declare function setState<T, K extends keyof T>(obj: T, props: Pick<T, K>): void;

let foo: Foo = { a: "hello", b: 42 };
setState(foo, { b: undefined });
"#;

    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            exact_optional_property_types: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2322
            && diag.message_text.contains("'undefined'")
            && diag.message_text.contains("'number'")),
        "Pick<T, K> should preserve optional-property undefined when exactOptionalPropertyTypes is off.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn remapped_intersection_callback_excess_property_display_matches_contextual_target() {
    let source = r#"
type Action<TEvent extends { type: string }> = (ev: TEvent) => void;

interface MachineConfig<TEvent extends { type: string }> {
  schema: {
    events: TEvent;
  };
  on?: {
    [K in TEvent["type"] as K extends Uppercase<string> ? K : never]?: Action<TEvent extends { type: K } ? TEvent : never>;
  } & {
    "*"?: Action<TEvent>;
  };
}

declare function createMachine<TEvent extends { type: string }>(
  config: MachineConfig<TEvent>
): void;

createMachine({
  schema: {
    events: {} as { type: "FOO" } | { type: "bar" },
  },
  on: {
    bar: (ev) => {
      ev;
    },
  },
});
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2353 = diagnostics
        .iter()
        .find(|diag| diag.code == 2353)
        .unwrap_or_else(|| panic!("expected TS2353, got: {diagnostics:#?}"));

    assert!(
        ts2353.message_text.contains(
            r#"{ FOO?: Action<{ type: "FOO"; }> | undefined; } & { "*"?: Action<{ type: "FOO"; } | { type: "bar"; }> | undefined; }"#
        ),
        "TS2353 should display the narrowed mapped member and wildcard branch, got: {}",
        ts2353.message_text
    );
}
