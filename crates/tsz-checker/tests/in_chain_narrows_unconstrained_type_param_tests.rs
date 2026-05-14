//! Tests that the truthy branch of `prop in x` narrows an unconstrained
//! type parameter to `T & Record<prop, unknown>`, so that subsequent `in`
//! checks (or property accesses) in the same `&&` chain do not re-emit
//! invalid-RHS diagnostics.
//!
//! Regression for `conditionalTypeDoesntSpinForever.ts`: tsz used to
//! emit an invalid-RHS diagnostic for every
//! `in` operator in an `&&` chain when the operand was an unconstrained
//! type parameter, instead of just for the first one. tsc narrows
//! after each successful `in`, so subsequent checks see `T & object`
//! and pass the operand-type check.
//!
//! Bare type parameters report TS2322 ("Type 'T' is not assignable to
//! type 'object'") rather than TS2638; intersections such as `T & {}`
//! that surface a `NonNullable<T>` apparent type stay on the TS2638
//! path. The truthy-chain narrowing invariant is the same — only the
//! diagnostic code emitted at the first `in` differs.

use tsz_checker::diagnostics as crate_diag;

#[test]
fn in_operator_narrows_mapped_union_member_property_type() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T
    ? K extends keyof Y
      ? Y[K]
      : never
    : U[K]
};

interface NodeA { type: 'A'; name: string }
interface NodeB { type: 'B'; id: number }

type Nodes = NodeA | NodeB;
type Replaced = ReplaceKeys<Nodes, 'name', { name: number }>;

declare const r: Replaced;

if ('name' in r) {
  const name: number = r.name;
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected mapped-union `in` narrowing to preserve `name: number`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_narrows_renamed_mapped_union_member_property_type() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
type Rewrite<UnionType, SelectedKey, Overrides> = {
  [Field in keyof UnionType]: Field extends SelectedKey
    ? Field extends keyof Overrides
      ? Overrides[Field]
      : never
    : UnionType[Field]
};

type Left = { kind: 'left'; label: string };
type Right = { kind: 'right'; count: number };

type Rewritten = Rewrite<Left | Right, 'label', { label: boolean }>;

declare const item: Rewritten;

if ('label' in item) {
  const label: boolean = item.label;
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected renamed mapped-union `in` narrowing to preserve `label: boolean`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_false_branch_excludes_required_mapped_union_property() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
type ReplaceKeys<U, T, Y> = {
  [K in keyof U]: K extends T
    ? K extends keyof Y
      ? Y[K]
      : never
    : U[K]
};

interface NodeA { type: 'A'; name: string }
interface NodeB { type: 'B'; id: number }

type Replaced = ReplaceKeys<NodeA | NodeB, 'name', { name: number }>;

declare const r: Replaced;

if ('name' in r) {
  const name: number = r.name;
} else {
  const id: number = r.id;
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected mapped-union `in` false branch to exclude the required property member, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_does_not_distribute_direct_concrete_keyof_union_mapped_type() {
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(
        r#"
type Left = { left: string };
type Right = { right: number };

type Direct = { [Key in keyof (Left | Right)]: boolean };

declare const direct: Direct;

if ('left' in direct) {
  const left: boolean = direct.left;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == 2322
            && diagnostic
                .message_text
                .contains("Type 'unknown' is not assignable")),
        "Expected direct concrete `keyof` union mapped type to narrow `left` as unknown, got {diagnostics:#?}"
    );
}

fn in_rhs_assignability_diagnostic_count(diagnostics: &[crate_diag::Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code == 2322 && d.message_text.contains("assignable to type 'object'"))
        .count()
}

#[test]
fn in_chain_emits_ts2322_only_at_first_link_for_unconstrained_type_param() {
    // tsc routes bare type parameters through TS2322 (assignability to
    // `object`) rather than TS2638 ("may represent a primitive"); only
    // intersections that surface a `NonNullable<T>`-style apparent type
    // keep the TS2638 path. The narrowing invariant remains the same:
    // exactly one diagnostic at the first `in`, none afterwards.
    let source = r#"
const f = <T>(x: T) => "a" in x && "b" in x && "c" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let count = in_rhs_assignability_diagnostic_count(&diagnostics);
    assert_eq!(
        count,
        1,
        "expected exactly 1 TS2322 (only the first `in` on an unconstrained T), got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2322 || d.code == 2638)
            .map(|d| (d.start, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn in_chain_narrowing_keys_off_token_kind_not_param_or_property_names() {
    // The fix is structural: any unconstrained type parameter should narrow
    // via `in` regardless of the names chosen for the parameter or the
    // property literals.
    for (tparam, props) in [
        ("T", ["a", "b", "c"]),
        ("U", ["x", "y", "z"]),
        ("MyParam", ["foo", "bar", "baz"]),
        ("_", ["__a", "__b", "__c"]),
    ] {
        let p0 = props[0];
        let p1 = props[1];
        let p2 = props[2];
        let source = format!(
            r#"const f = <{tparam}>(x: {tparam}) => "{p0}" in x && "{p1}" in x && "{p2}" in x;"#
        );
        let diagnostics = tsz_checker::test_utils::check_source_diagnostics(&source);
        let count = in_rhs_assignability_diagnostic_count(&diagnostics);
        assert_eq!(
            count,
            1,
            "param={tparam} props={props:?}: expected 1 TS2322, got {} ({:?})",
            count,
            diagnostics
                .iter()
                .filter(|d| d.code == 2322 || d.code == 2638)
                .map(|d| d.message_text.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn in_chain_in_or_chain_still_emits_per_link() {
    // The narrowing only applies in the `&&` truthy chain. In a `||` chain
    // each `in` check is independent (the right side runs when the left
    // failed) so the operand-type error must still fire for each one.
    let source = r#"
const f = <T>(x: T) => "a" in x || "b" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let count = in_rhs_assignability_diagnostic_count(&diagnostics);
    assert_eq!(
        count,
        2,
        "|| chain should report each `in` independently, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2322 || d.code == 2638)
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn constrained_type_param_keeps_existing_behavior() {
    // T extends object — operand type already valid, no TS2322 expected.
    let source = r#"
const f = <T extends object>(x: T) => "a" in x && "b" in x;
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "constrained `T extends object` should not produce TS2322 for `in`, got: {:?}",
        ts2322
            .iter()
            .map(|d| d.message_text.clone())
            .collect::<Vec<_>>()
    );
}

fn ts2638_count(source: &str) -> usize {
    tsz_checker::test_utils::check_source_codes(source)
        .into_iter()
        .filter(|&code| code == 2638)
        .count()
}

#[test]
fn in_operator_rejects_generic_nonnullable_intersections() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<P>(a: P & {}) {
  "foo" in a;
}

type NonNull<T> = T & {};
function g<T>(a: NonNull<T>) {
  "foo" in a;
}
"#,
    );

    let ts2638: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2638)
        .collect();
    assert_eq!(
        ts2638.len(),
        2,
        "Expected TS2638 for both generic non-nullable RHS values, got {diagnostics:#?}"
    );
    assert!(
        ts2638
            .iter()
            .any(|(_, message)| message.contains("NonNullable<P>")),
        "Expected diagnostic to mention NonNullable<P>, got {diagnostics:#?}"
    );
    assert!(
        ts2638
            .iter()
            .any(|(_, message)| message.contains("NonNull<T>")),
        "Expected diagnostic to mention NonNull<T>, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_allows_object_constrained_intersections() {
    let source = r#"
function object_rhs(a: object) {
  "foo" in a;
}

function object_intersection<T>(a: T & object) {
  "foo" in a;
}

function non_empty_object_intersection<T>(a: T & { value: number }) {
  "foo" in a;
}

interface EmptyInterface {}
function empty_interface_intersection<T>(a: T & EmptyInterface) {
  "foo" in a;
}
"#;

    let diagnostics = tsz_checker::test_utils::check_source_code_messages(source);
    assert!(
        diagnostics.is_empty(),
        "Expected object-constrained RHS values to be accepted, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_rejects_empty_object_type_alias_intersection() {
    let source = r#"
type Empty = {};
function alias_intersection<T>(a: T & Empty) {
  "foo" in a;
}
"#;

    assert_eq!(
        ts2638_count(source),
        1,
        "Expected empty object type alias intersection to emit TS2638"
    );
}

#[test]
fn in_operator_narrows_unique_symbol_property_presence_on_object() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
const sym = Symbol();
function f(x: object) {
  if ("a" in x && 1 in x && sym in x) {
    x[sym];
  }
}
"#,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 7053),
        "Expected unique-symbol `in` narrowing to suppress TS7053, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2322_for_bare_generic_rhs() {
    // tsc emits `Type 'T' is not assignable to type 'object'` (TS2322)
    // for bare type parameters on the right side of `in`, NOT TS2638.
    // The narrowing on the truthy branch must still expose property `a`.
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<T>(x: T) {
  if ("a" in x) {
    x.a;
  }
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("'T'") && message.contains("'object'")
        }),
        "Expected TS2322 against `object` for bare generic `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for bare generic `in` RHS (intersection-only path), got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected `in` narrowing to expose property `a`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2322_for_generic_union_rhs() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<T>(x: T | { a: string }) {
  "k" in x;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("Type '")
                && message.contains("' is not assignable to type 'object'.")
                && message.contains("T")
                && message.contains("{ a: string; }")
        }),
        "Expected TS2322 for generic union `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for generic union `in` RHS, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_false_branch_generic_union_reports_ts2322_not_ts2638() {
    // Mirrors the conditional builder pattern from
    // conditionalTypeDoesntSpinForever.ts without copying the full fixture.
    // The false branch of `&&` can have a flow type like
    // `T | (T & Record<"a", unknown>)`; tsc still reports the nested `in`
    // operand via TS2322 rather than TS2638.
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
type Builder<T> =
  T extends { a: any, b: any } ? {} : {
    has: (k: string | number | symbol) => k is keyof T
  };

const f = <T>(x: T): Builder<T> =>
  ("a" in x && "b" in x ? {} : {
    has: (k: string | number | symbol) => k in x
  }) as Builder<T>;
"#,
    );

    let in_rhs_object_assignability = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("'T'") && message.contains("'object'")
        })
        .count();
    assert_eq!(
        in_rhs_object_assignability, 2,
        "Expected TS2322 for the condition RHS and nested false-branch RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2638),
        "Expected no TS2638 for false-branch generic union RHS, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_reports_ts2638_for_truthiness_guarded_generic_rhs() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function f<T>(x: T) {
  if (x && "a" in x) {
    x.a;
  }
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2638 && message.contains("NonNullable<T>") }),
        "Expected TS2638 against `NonNullable<T>` for truthiness-guarded generic `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2322 && message.contains("'T'") && message.contains("'object'")
        }),
        "Expected no bare TS2322 for truthiness-guarded generic `in` RHS, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected `in` narrowing to expose property `a`, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_keeps_generic_and_unknown_property_narrowing_through_truthiness_chains() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function unknownCase(x: unknown) {
  if (x && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x) {
    x.a;
  }
}

function genericCase<T>(x: T) {
  if (x && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x) {
    x.a;
  }
  if (x && typeof x === "object" && "a" in x && "b" in x && "c" in x) {
    x.a;
    x.b;
    x.c;
  }
}
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2638 && message.contains("{}") }),
        "Expected TS2638 for unknown or truthiness-narrowed unknown, got {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2638 && message.contains("NonNullable<T>") }),
        "Expected TS2638 for truthiness-narrowed generic T, got {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected property accesses to use the `in`-narrowed record shape, got {diagnostics:#?}"
    );
}

#[test]
fn in_operator_keeps_unknown_property_after_explicit_null_check() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
function test(x: unknown) {
  if (typeof x === "object" && x !== null && "foo" in x) {
    const val = x.foo;
  }
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "Expected explicit null check plus `in` narrowing to expose `foo`, got {diagnostics:#?}"
    );
}
