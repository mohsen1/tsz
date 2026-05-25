//! Regression tests for issue #9779: `typeof x.prop` of a `const` whose
//! initializer is contextually typed (via an explicit annotation or a
//! `satisfies` target) must keep the contextually fixed literal type in
//! conditional-type `extends` checks, not the widened base type.
//!
//! Structural rule: when a `const` has an object-literal initializer AND a
//! contextual type that fixes a property to a literal, `typeof base.prop`
//! resolves to that literal everywhere — including as the `extends` source of
//! a conditional type — matching the assignment view and `typeof base`.

use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_strict(source)
}

fn assert_no_ts2322(source: &str, label: &str) {
    let diags = check(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

fn assert_has_ts2322(source: &str, label: &str) {
    let diags = check(source);
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "[{label}] expected a TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn annotation_preserves_literal_in_conditional() {
    assert_no_ts2322(
        r#"
const x: { a: 1 } = { a: 1 };
type A = typeof x.a;
type R = [A] extends [1] ? "yes" : "no";
const probe: R = "yes";
"#,
        "annotation literal property",
    );
}

#[test]
fn satisfies_preserves_literal_in_conditional() {
    assert_no_ts2322(
        r#"
const x = { a: 1 } satisfies { a: 1 };
type A = typeof x.a;
type R = [A] extends [1] ? "yes" : "no";
const probe: R = "yes";
"#,
        "satisfies literal property",
    );
}

#[test]
fn union_literal_property_preserved() {
    assert_no_ts2322(
        r#"
const obj: { x: "a" | "b" } = { x: "a" };
type R = ["a" | "b"] extends [typeof obj.x] ? "yes" : "no";
const probe: R = "yes";
"#,
        "union literal property",
    );
}

#[test]
fn renamed_boolean_literal_property_preserved() {
    // Renamed property + a different literal kind (boolean) proves the rule is
    // structural, not keyed off the `a`/`1` spelling in the reported repro.
    assert_no_ts2322(
        r#"
const x: { flag: true } = { flag: true };
type R = [typeof x.flag] extends [true] ? "yes" : "no";
const probe: R = "yes";
"#,
        "renamed boolean literal property",
    );
}

#[test]
fn satisfies_renamed_string_literal_property_preserved() {
    assert_no_ts2322(
        r#"
const cfg = { mode: "fast" } satisfies { mode: "fast" };
type R = [typeof cfg.mode] extends ["fast"] ? "yes" : "no";
const probe: R = "yes";
"#,
        "satisfies renamed string literal property",
    );
}

#[test]
fn no_annotation_still_widens() {
    // Negative/fallback control: without a contextual type the property literal
    // widens (object-literal property widening), so `typeof x.a` is `number`
    // and `[A] extends [1]` is false. Assigning the true branch must error.
    assert_has_ts2322(
        r#"
const x = { a: 1 };
type A = typeof x.a;
type R = [A] extends [1] ? "yes" : "no";
const bad: R = "yes";
"#,
        "no annotation widens",
    );
}

#[test]
fn annotation_widening_member_stays_widened() {
    // Control: an annotation that itself uses the widened type must keep the
    // member widened. `typeof x.a` is `number`, so `[A] extends [1]` is false.
    assert_has_ts2322(
        r#"
const x: { a: number } = { a: 1 };
type A = typeof x.a;
type R = [A] extends [1] ? "yes" : "no";
const bad: R = "yes";
"#,
        "annotation widening member",
    );
}

#[test]
fn assignment_view_of_member_unchanged() {
    // Control: the assignment view of the property was always correct (`1`).
    // Assigning it to `5` must still error.
    assert_has_ts2322(
        r#"
const x: { a: 1 } = { a: 1 };
const bad: 5 = x.a;
"#,
        "assignment view of member",
    );
}
