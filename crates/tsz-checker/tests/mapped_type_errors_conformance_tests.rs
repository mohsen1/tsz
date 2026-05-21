use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_diagnostics, check_with_options, has_any_diagnostic_code, has_diagnostic_code,
    line_column_for_offset,
};

fn diagnostic_anchor_text<'a>(source: &'a str, diagnostic: &Diagnostic) -> &'a str {
    assert_eq!(
        diagnostic.file, "test.ts",
        "expected mapped-type conformance diagnostics to stay in test.ts, got: {diagnostic:#?}"
    );
    let start = diagnostic.start as usize;
    let end = start.saturating_add(diagnostic.length as usize);
    source.get(start..end).unwrap_or_else(|| {
        panic!(
            "diagnostic span [{start}..{end}) is out of bounds or not on char boundaries for source length {}: {diagnostic:#?}",
            source.len()
        )
    })
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
        line_column_for_offset(source, ts2322[0].start),
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
        line_column_for_offset(source, ts2322[0].start),
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

#[test]
fn mapped_without_as_is_not_assignable_to_mapped_with_template_literal_as() {
    // Rule: { [K in keyof T]: T[K] } is not a subtype of
    //       { [K in keyof T as `get_${string & K}`]: T[K] }
    // because the target renames every key.  tsc emits TS2322 here.
    let source = r#"
type WithRaw<T>       = { [K in keyof T]: T[K] };
type WithGetPrefix<T> = { [K in keyof T as `get_${string & K}`]: T[K] };
function test<T>(x: WithRaw<T>): WithGetPrefix<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 when assigning a non-`as` mapped type to a key-renaming mapped type, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_without_as_is_not_assignable_to_mapped_with_template_literal_as_renamed_binders() {
    // Same rule, different iteration-variable names (`Item`/`Q` instead of `K`).
    // Proves the check is structural, not name-dependent.
    let source = r#"
type Plain<T>    = { [Item in keyof T]: T[Item] };
type Prefixed<T> = { [Q in keyof T as `set_${string & Q}`]: T[Q] };
function bad<T>(x: Plain<T>): Prefixed<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 for renamed-binder variant of key-renaming mismatch, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_with_as_is_not_assignable_to_mapped_without_as() {
    // Inverse direction: a key-renaming source cannot satisfy a plain (no-`as`) target
    // because the source only has `get_*` keys, not the original ones.
    let source = r#"
type WithGetPrefix<T> = { [K in keyof T as `get_${string & K}`]: T[K] };
type WithRaw<T>       = { [K in keyof T]: T[K] };
function bad<T>(x: WithGetPrefix<T>): WithRaw<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 when assigning a key-renamed mapped type to a plain mapped type, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn two_mapped_types_same_template_literal_as_are_assignable() {
    // Positive case: both sides carry the same `as` clause — the key spaces
    // agree and the assignment must succeed with no diagnostic.
    let source = r#"
type Prefixed<T> = { [K in keyof T as `get_${string & K}`]: T[K] };
function ok<T>(x: Prefixed<T>): Prefixed<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "Expected no TS2322 when both mapped types carry the same `as` clause, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn two_plain_mapped_types_same_constraint_are_assignable() {
    // Positive case: two mapped types without `as` clauses and the same
    // constraint are mutually assignable.
    let source = r#"
type A<T> = { [K in keyof T]: T[K] };
type B<T> = { [P in keyof T]: T[P] };
function ok<T>(x: A<T>): B<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "Expected no TS2322 for two plain homomorphic mapped types, got: {diagnostics:#?}"
    );
}

#[test]
fn concrete_mapped_without_as_not_assignable_to_concrete_mapped_with_as() {
    // Concrete (non-generic) variant: constraint is a literal union.
    // { [K in "x" | "y"]: number } is not assignable to
    // { [K in "x" | "y" as `get_${K}`]: number } because the key sets differ.
    // For concrete mapped types tsc expands to an object literal and reports
    // TS2739 (missing properties) or TS2741 rather than TS2322.
    let source = r#"
type Keys = "x" | "y";
type Raw     = { [K in Keys]: number };
type Renamed = { [K in Keys as `get_${K}`]: number };
declare const raw: Raw;
const r: Renamed = raw;
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        has_any_diagnostic_code(&diagnostics, &[2322, 2739, 2741]),
        "Expected an assignment error (TS2322/TS2739/TS2741) for concrete mapped-without-as \
         assigned to mapped-with-as, got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_with_different_as_clauses_are_not_assignable() {
    // Two mapped types that both have `as` clauses but different key-remapping
    // expressions are not mutually assignable.
    let source = r#"
type GetPrefixed<T>  = { [K in keyof T as `get_${string & K}`]: T[K] };
type SetPrefixed<T>  = { [K in keyof T as `set_${string & K}`]: T[K] };
function bad<T>(x: GetPrefixed<T>): SetPrefixed<T> {
    return x;
}
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Expected TS2322 when two mapped types have different `as` key-remapping clauses, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn identity_as_clause_is_assignable_to_plain_mapped_type() {
    // `{ [K in keyof T as K]: T[K] }` is structurally equivalent to
    // `{ [K in keyof T]: T[K] }`. The identity `as K` clause must NOT produce TS2322.
    let source = r#"
type IdentityMapped<T> = { [K in keyof T as K]: T[K] };
type PlainMapped<T>    = { [K in keyof T]: T[K] };
function okA<T>(x: IdentityMapped<T>): PlainMapped<T> { return x; }
function okB<T>(x: PlainMapped<T>): IdentityMapped<T> { return x; }
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "Expected NO TS2322 for identity `as K` mapped type assigned to plain mapped type, \
         got: {diagnostics:#?}"
    );
}

#[test]
fn identity_as_clause_renamed_binders_is_assignable_to_plain_mapped_type() {
    // Same as above but with renamed iteration variables (P vs Q) to prove
    // the fix is structural, not name-dependent.
    let source = r#"
type IdentityMapped<T> = { [P in keyof T as P]: T[P] };
type PlainMapped<T>    = { [Q in keyof T]: T[Q] };
function okA<T>(x: IdentityMapped<T>): PlainMapped<T> { return x; }
function okB<T>(x: PlainMapped<T>): IdentityMapped<T> { return x; }
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !has_diagnostic_code(&diagnostics, 2322),
        "Expected NO TS2322 for identity `as P` mapped type (renamed binders) \
         assigned to plain mapped type, got: {diagnostics:#?}"
    );
}
