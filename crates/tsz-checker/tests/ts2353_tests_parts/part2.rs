#[test]
fn fbounded_true_excess_property_still_errors() {
    let ts2353 = ts2353_diags(&format!(
        "{TREE_BTREE_DECLS}const bt: BTree = {{ value: 1, children: [], extra: true }};"
    ));
    assert_eq!(
        ts2353.len(),
        1,
        "F-bounded: 'extra' is a genuine excess property and must produce TS2353; got: {ts2353:?}"
    );
    assert!(
        ts2353[0].1.contains("'extra'"),
        "TS2353 message should mention 'extra'; got: {ts2353:?}"
    );
}

#[test]
fn fbounded_non_self_referential_generic_no_ts2353() {
    let ts2353 = ts2353_diags(
        r#"
interface Container<T> {
    items: T[];
}
interface StringContainer extends Container<string> {
    name: string;
}
const sc: StringContainer = {
    name: "test",
    items: [],
};
"#,
    );
    assert!(
        ts2353.is_empty(),
        "Non-F-bounded: 'items' is an inherited property and must not trigger TS2353; got: {ts2353:?}"
    );
}

#[test]
fn ternary_branch_object_literal_reports_excess_property() {
    let diags = get_diagnostics(
        r#"
interface I { a: number }
declare const cond: boolean;
const v: I = cond ? { a: 1, b: 2 } : { a: 3 };
"#,
    );
    let excess: Vec<_> = diags
        .iter()
        .filter(|d| (d.0 == 2353 || d.0 == 2322) && d.1.contains("'b'"))
        .collect();
    assert!(
        !excess.is_empty(),
        "Expected excess-property diagnostic for 'b' in a ternary branch, got: {diags:?}",
    );
}

#[test]
fn nullish_coalescing_right_object_literal_reports_excess_property() {
    let diags = get_diagnostics(
        r#"
interface I { a: number }
declare const d: I | undefined;
const v: I = d ?? { a: 1, b: 2 };
"#,
    );
    let excess: Vec<_> = diags
        .iter()
        .filter(|d| (d.0 == 2353 || d.0 == 2322) && d.1.contains("'b'"))
        .collect();
    assert!(
        !excess.is_empty(),
        "Expected excess-property diagnostic for 'b' on the right of `??`, got: {diags:?}",
    );
}

#[test]
fn non_contextual_binary_rhs_does_not_report_property_excess() {
    let diags = get_diagnostics(
        r#"
interface I { a: number }
const v: I = (0 as any) + { a: 1, b: 2 };
"#,
    );
    assert!(
        diags.iter().all(|d| d.0 != 2353
            && !d
                .1
                .contains("Object literal may only specify known properties")),
        "Did not expect property-level excess checking through `+`, got: {diags:?}",
    );
}

#[test]
fn ternary_branch_excess_property_uses_renamed_property() {
    // Test matrix item: the rule applies regardless of property spelling.
    let diags = get_diagnostics(
        r#"
interface MyShape { name: string }
declare const cond: boolean;
const v: MyShape = cond ? { name: "a", age: 30 } : { name: "b" };
"#,
    );
    let excess: Vec<_> = diags
        .iter()
        .filter(|d| (d.0 == 2353 || d.0 == 2322) && d.1.contains("'age'"))
        .collect();
    assert!(
        !excess.is_empty(),
        "Expected excess-property diagnostic for 'age' in a renamed-property ternary, got: {diags:?}",
    );
}

#[test]
fn ternary_branch_nested_object_literal_reports_inner_excess_property() {
    // Test matrix item: nested literal in a branch must still be checked.
    let diags = get_diagnostics(
        r#"
interface I { a: { b: number } }
declare const c: boolean;
const v: I = c ? { a: { b: 1, c: 2 } } : { a: { b: 2 } };
"#,
    );
    let excess: Vec<_> = diags
        .iter()
        .filter(|d| (d.0 == 2353 || d.0 == 2322) && d.1.contains("'c'"))
        .collect();
    assert!(
        !excess.is_empty(),
        "Expected excess-property diagnostic for inner 'c' in a nested ternary literal, got: {diags:?}",
    );
}

#[test]
fn ternary_branches_without_excess_property_stay_clean() {
    // Negative control: matching literals in both branches must not emit
    // any TS2353/TS2322 — the rule fires only when a fresh member has an
    // actual excess property against the target.
    let diags = get_diagnostics(
        r#"
interface I { a: number }
declare const cond: boolean;
const v: I = cond ? { a: 1 } : { a: 2 };
"#,
    );
    assert!(
        diags.iter().all(|d| d.0 != 2353 && d.0 != 2322),
        "Did not expect any excess-property diagnostic for a clean ternary, got: {diags:?}",
    );
}

#[test]
fn ternary_branch_object_literal_in_call_argument_reports_excess_property() {
    // Test matrix item: the rule must fire on call-argument context, where
    // the failure surfaces as TS2345 with the excess elaboration.
    let diags = get_diagnostics(
        r#"
interface I { a: number }
declare function take(x: I): void;
declare const c: boolean;
take(c ? { a: 1, b: 2 } : { a: 3 });
"#,
    );
    assert!(
        diags
            .iter()
            .any(|d| matches!(d.0, 2345 | 2353 | 2322) && d.1.contains('\'')),
        "Expected an excess-property diagnostic for the call-argument ternary, got: {diags:?}",
    );
}

#[test]
fn return_statement_conditional_branch_reports_excess_property() {
    // Test matrix item: same structural rule applies on `return` branches —
    // tsc still excess-checks each conditional branch against the declared
    // return type even though the union result is what flows up.
    let diags = get_diagnostics(
        r#"
interface I { a: number }
declare const c: boolean;
function f(): I {
    return c ? { a: 1, b: 2 } : { a: 3 };
}
"#,
    );
    let excess: Vec<_> = diags
        .iter()
        .filter(|d| (d.0 == 2353 || d.0 == 2322) && d.1.contains("'b'"))
        .collect();
    assert!(
        !excess.is_empty(),
        "Expected excess-property diagnostic for 'b' returned in a conditional branch, got: {diags:?}",
    );
}

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
