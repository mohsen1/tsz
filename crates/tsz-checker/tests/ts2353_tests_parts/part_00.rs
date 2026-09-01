// ─── Excess-property vs property-type-mismatch precedence ─────────────────────
//
// tsc reports a present-in-target property type mismatch (TS2322) ahead of an
// excess-property error (TS2353) from the same object literal, but the excess
// error still preempts a missing-required-property failure. The resulting
// precedence is: present-property mismatch > excess (TS2353) > missing required.
// Binder names are varied across cases so the rule is exercised structurally
// rather than against any fixed identifier.

fn codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags.iter().map(|d| d.0).collect()
}

#[test]
fn property_mismatch_outranks_excess_property() {
    // `value: number` is a present-in-target mismatch; `extra` is excess.
    // tsc reports only TS2322 for `value`.
    let diags = get_diagnostics(
        r#"
declare let target: { value: string };
target = { value: 1, extra: true };
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "Expected TS2322 for the value mismatch, got: {diags:?}",
    );
    assert!(
        !codes(&diags).contains(&2353),
        "Excess-property TS2353 must be suppressed when a present property mismatches, got: {diags:?}",
    );
}

#[test]
fn property_mismatch_outranks_excess_when_excess_listed_first() {
    // Source-order independence: the excess property appears before the
    // mismatching one, yet tsc still reports the mismatch and suppresses excess.
    let diags = get_diagnostics(
        r#"
declare let widget: { label: string };
widget = { junk: true, label: 42 };
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "Expected TS2322, got: {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&2353),
        "Excess TS2353 must be suppressed regardless of property order, got: {diags:?}",
    );
}

#[test]
fn excess_property_reported_when_all_present_properties_match() {
    // No present-property mismatch → tsc reports the excess property (TS2353).
    let diags = get_diagnostics(
        r#"
declare let config: { mode: string };
config = { mode: "fast", verbose: true };
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected exactly one TS2353 for the excess property, got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'verbose'"),
        "Expected excess 'verbose', got: {ts2353:?}"
    );
    assert!(
        !codes(&diags).contains(&2322),
        "No TS2322 expected here, got: {diags:?}"
    );
}

#[test]
fn excess_property_outranks_missing_required_property() {
    // The source lacks the required `name` AND has an excess `bogus`.
    // tsc reports the excess property (TS2353), not the missing one.
    let diags = get_diagnostics(
        r#"
declare let record: { name: string };
record = { bogus: 1 };
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected TS2353 to preempt the missing-property failure, got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'bogus'"),
        "Expected excess 'bogus', got: {ts2353:?}"
    );
}

#[test]
fn property_mismatch_outranks_excess_for_intersection_target() {
    let diags = get_diagnostics(
        r#"
type Combined = { left: string } & { right: number };
declare let slot: Combined;
slot = { left: 1, right: 2, dangling: true };
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "Expected TS2322 for intersection member, got: {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&2353),
        "Excess TS2353 must be suppressed for intersection targets too, got: {diags:?}",
    );
}

#[test]
fn property_mismatch_outranks_excess_for_mapped_target() {
    let diags = get_diagnostics(
        r#"
type Pair = { [Key in "first" | "second"]: string };
declare let pair: Pair;
pair = { first: "ok", second: 7, third: true };
"#,
    );
    assert!(
        codes(&diags).contains(&2322),
        "Expected TS2322 for mapped target member, got: {diags:?}"
    );
    assert!(
        !codes(&diags).contains(&2353),
        "Excess TS2353 must be suppressed for mapped targets too, got: {diags:?}",
    );
}

#[test]
fn nested_excess_still_reported_without_a_real_mismatch() {
    // The nested object literal has an excess `noise` but no type mismatch, so
    // the outer literal must NOT be wrongly suppressed — the nested excess is
    // still reported.
    let diags = get_diagnostics(
        r#"
declare let outer: { branch: { keep: string } };
outer = { branch: { keep: "ok", noise: 1 } };
"#,
    );
    let ts2353: Vec<_> = diags.iter().filter(|d| d.0 == 2353).collect();
    assert_eq!(
        ts2353.len(),
        1,
        "Expected the nested excess 'noise' to still be reported, got: {diags:?}",
    );
    assert!(
        ts2353[0].1.contains("'noise'"),
        "Expected excess 'noise', got: {ts2353:?}"
    );
    assert!(
        !codes(&diags).contains(&2322),
        "No spurious TS2322 expected, got: {diags:?}"
    );
}

// Issue #13076: the excess-property target display must be formatted from the
// annotation's lowered TypeId, not sliced from raw source text. Source-text
// slicing keeps non-object union members (`string`) that tsc strips, and breaks
// on multiline annotations and generic aliases.

#[test]
fn excess_property_union_annotation_strips_non_object_member_for_display() {
    // tsc checks the object literal against the only object-like union member
    // (`Book`) and displays `Book`, dropping the `string` member. The source-text
    // pipeline used to echo the written `Book | string`.
    let source = r#"
interface Book { title: string }
const b: Book | string = { title: "x", extra: 1 };
"#;
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'Book'") && !ts2353.1.contains("string"),
        "Expected TS2353 to display the stripped object member 'Book', got: {}",
        ts2353.1
    );
}

#[test]
fn excess_property_multiline_generic_alias_annotation_renders_canonically() {
    // A generic alias annotation spread across lines must render as the
    // canonical `Wrap<number>` rather than a source slice that carries the
    // newline/whitespace of the written `Wrap<\n  number\n>`.
    let source = "
type Wrap<T> = { value: T };
const x: Wrap<
  number
> = { value: 1, extra: 2 };
";
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'Wrap<number>'"),
        "Expected canonical generic alias display, got: {}",
        ts2353.1
    );
    assert!(
        !ts2353.1.contains('\n'),
        "Expected no embedded newline from source slicing, got: {}",
        ts2353.1
    );
}

#[test]
fn excess_property_multiline_object_annotation_renders_single_line_display() {
    // A multiline inline object annotation must render as the canonical
    // single-line `{ a: number; }`, not the raw multiline source slice.
    let source = "
const x: {
  a: number;
} = { a: 1, b: 2 };
";
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353).expect("expected TS2353");
    assert!(
        ts2353.1.contains("'{ a: number; }'"),
        "Expected canonical single-line object display, got: {}",
        ts2353.1
    );
    assert!(
        !ts2353.1.contains('\n'),
        "Expected no embedded newline from source slicing, got: {}",
        ts2353.1
    );
}
