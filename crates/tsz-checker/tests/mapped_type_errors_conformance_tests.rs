use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_diagnostics, check_with_options};

fn line_and_column_for_offset(source: &str, offset: u32) -> (u32, u32) {
    let mut line = 1;
    let mut column = 1;
    for (idx, ch) in source.char_indices() {
        if idx == offset as usize {
            return (line, column);
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn diagnostic_anchor_text<'a>(source: &'a str, diagnostic: &Diagnostic) -> &'a str {
    &source[diagnostic.start as usize..(diagnostic.start + diagnostic.length) as usize]
}

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
fn mapped_key_constraint_ts2322_anchors_the_invalid_constraint_type() {
    let source = "type Source = { x: number };\ntype Bad = { [P in Source]: number };\n";

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for the invalid mapped key constraint, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type 'Source' is not assignable to type 'string | number | symbol'."),
        "TS2322 should report the mapped key constraint assignability failure, got: {ts2322:#?}"
    );
    assert_eq!(
        diagnostic_anchor_text(source, ts2322[0]),
        "Source",
        "TS2322 must anchor on the invalid mapped key constraint type, got: {ts2322:#?}"
    );
    assert_eq!(
        line_and_column_for_offset(source, ts2322[0].start),
        (2, 20),
        "mapped key constraint TS2322 should keep the conformance fingerprint location"
    );
}

#[test]
fn mapped_key_index_access_ts2322_anchors_the_constraint_expression() {
    let source = r#"
type AB = {
    a: 'a';
    b: 'a';
};
type Bad<S extends 'a' | 'b' | 'extra'> = { [Key in AB[S]]: true }[S];
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 for AB[S] not assignable to a mapped key type, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .message_text
            .contains("Type 'AB[S]' is not assignable to type 'string | number | symbol'."),
        "TS2322 should preserve the invalid indexed-access key expression, got: {ts2322:#?}"
    );
    assert!(
        diagnostic_anchor_text(source, ts2322[0]).starts_with("AB[S]"),
        "TS2322 must start on the mapped key constraint expression, got: {ts2322:#?}"
    );
    assert_eq!(
        line_and_column_for_offset(source, ts2322[0].start),
        (6, 53),
        "mapped indexed-access key constraint TS2322 should keep the conformance fingerprint location"
    );
}

#[test]
fn record_key_constraint_displays_primitive_key_union() {
    // tsc strips the `aliasSymbol` from the constraint type before formatting
    // the TS2344 message, so `Record<object, _>` reports the structural
    // `string | number | symbol` form rather than the registered `PropertyKey`
    // alias. Other diagnostic surfaces still keep `PropertyKey` (see
    // `object_group_by_key_constraint_uses_property_key_in_diagnostic`).
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
            .any(|message| message.contains("constraint 'string | number | symbol'")),
        "Record's key constraint should display 'string | number | symbol', got: {diagnostics:#?}"
    );
    assert!(
        ts2344.iter().all(|message| !message.contains("AudioData")),
        "Record's key constraint must not be repainted by unrelated lib names: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .all(|message| !message.contains("constraint 'PropertyKey'")),
        "Record's key constraint must not be displayed as PropertyKey in TS2344: {diagnostics:#?}"
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

#[test]
fn ts2344_constraint_message_expands_keyof_any_to_primitive_key_union_when_arg_is_user_alias() {
    // Regression for `compiler/jsxIntrinsicElementsTypeArgumentErrors.tsx`
    // and `conformance/types/mapped/mappedTypeErrors.ts`. Iteration variable
    // name doesn't matter (anti-hardcoding §25): swapping `K`/`P` for
    // `Element`/`Q` must keep the structural form.
    let source = r#"
type RecordA<Element extends keyof any, T> = { [Q in Element]: T };
type Bad = RecordA<object, number>;
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
            .any(|message| message.contains("constraint 'string | number | symbol'")),
        "TS2344 against keyof any should display structurally, got: {diagnostics:#?}"
    );
    assert!(
        ts2344
            .iter()
            .all(|message| !message.contains("constraint 'PropertyKey'")),
        "TS2344 against keyof any must not collapse to PropertyKey: {diagnostics:#?}"
    );
}
