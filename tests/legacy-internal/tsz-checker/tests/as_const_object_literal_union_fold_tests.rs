//! A const assertion keeps its operand's fresh object-literal identity.
//!
//! Structural rule: when an object literal under a `const` assertion fails
//! against a union-of-objects target, tsc reports the same fresh-literal fold
//! as a bare literal — the failing property directly beneath the head with NO
//! `Type 'S' is not assignable to type '<member>'.` member frame — because
//! `checkAssertionWorker` preserves the operand's `FreshLiteral` object flag.
//! tsz does this through the solver's `ConstAssertionVisitor`, whose readonly
//! object rebuild preserves the source shape's flags, declaring symbol, and
//! (const-asserted) display provenance instead of laundering the type into an
//! anonymous non-fresh object.
//!
//! A REFERENCE to a const-asserted value (a variable) and an `as <type>`
//! assertion are not fresh, so both keep the member frame.
//!
//! Every expectation is oracle-pinned against the pinned typescript@7.0.2 via
//! `scripts/conformance/oracle.sh --strict --noEmit` (byte-identical output).
//! Property and binder names vary across cases so the behavior is structural,
//! not keyed to an identifier spelling.

use crate::test_utils::check_source_diagnostics;

fn chain_texts(source: &str, code: u32) -> (String, Vec<(u8, String)>) {
    let diags = check_source_diagnostics(source);
    let diag = diags
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("expected a TS{code} diagnostic, got: {diags:?}"));
    (
        diag.message_text.clone(),
        diag.related_information
            .iter()
            .map(|info| (info.depth, info.message_text.clone()))
            .collect(),
    )
}

#[test]
fn as_const_union_fold_has_no_member_frame() {
    let (head, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r: R = { p: 1, q: 4 } as const;
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ readonly p: 1; readonly q: 4; }' is not assignable to type 'R'."),
        "head should keep readonly literal properties, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'q' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "as-const source should fold like a fresh literal, with no member frame"
    );
}

#[test]
fn parenthesized_as_const_folds_identically() {
    let (head, chain) = chain_texts(
        r#"
type Pick2 = { u: "l"; w: 8 } | { u: "m"; w: 9 };
const pk: Pick2 = ({ u: "l", w: 9 }) as const;
"#,
        2322,
    );
    assert!(
        head.contains(r#"Type '{ readonly u: "l"; readonly w: 9; }' is not assignable"#),
        "parenthesized as-const head should keep readonly literals, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'w' are incompatible.".to_string()),
            (1, "Type '9' is not assignable to type '8'.".to_string()),
        ],
    );
}

#[test]
fn as_const_discriminant_matched_arm_decides_folded_property() {
    // `mode: "a"` narrows to arm one; the fold reports `lvl` against that arm.
    let (head, chain) = chain_texts(
        r#"
type Cfg = { mode: "a"; lvl: 1 } | { mode: "b"; lvl: 2 };
const c: Cfg = { mode: "a", lvl: 2 } as const;
"#,
        2322,
    );
    assert!(
        head.contains(r#"Type '{ readonly mode: "a"; readonly lvl: 2; }' is not assignable"#),
        "head should keep readonly literal properties, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'lvl' are incompatible.".to_string()),
            (1, "Type '2' is not assignable to type '1'.".to_string()),
        ],
    );
}

#[test]
fn satisfies_as_const_folds_under_ts1360() {
    let (head, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r = { p: 1, q: 4 } as const satisfies R;
"#,
        1360,
    );
    assert!(
        head.contains(
            "Type '{ readonly p: 1; readonly q: 4; }' does not satisfy the expected type 'R'."
        ),
        "satisfies head should keep readonly literals, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'q' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
    );
}

#[test]
fn ts2345_argument_as_const_keeps_readonly_head_and_folds() {
    let (head, chain) = chain_texts(
        r#"
type R2 = { a: 1; b: 2 } | { a: 3; b: 4 };
declare function callit(r: R2): void;
callit({ a: 1, b: 4 } as const);
"#,
        2345,
    );
    assert!(
        head.contains(
            "Argument of type '{ readonly a: 1; readonly b: 4; }' is not assignable to \
             parameter of type 'R2'."
        ),
        "argument head should keep readonly literal properties, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (0, "Types of property 'b' are incompatible.".to_string()),
            (1, "Type '4' is not assignable to type '2'.".to_string()),
        ],
    );
}

#[test]
fn variable_reference_to_as_const_value_keeps_member_frame() {
    // A reference is NOT fresh: the member frame stays, byte-identical to tsc.
    let (head, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const tmp = { p: 1, q: 4 } as const;
const r: R = tmp;
"#,
        2322,
    );
    assert!(
        head.contains("Type '{ readonly p: 1; readonly q: 4; }' is not assignable to type 'R'."),
        "reference head should keep readonly literal properties, got: {head}"
    );
    assert_eq!(
        chain,
        vec![
            (
                0,
                "Type '{ readonly p: 1; readonly q: 4; }' is not assignable to type \
                 '{ p: 1; q: 2; }'."
                    .to_string()
            ),
            (1, "Types of property 'q' are incompatible.".to_string()),
            (2, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "non-fresh reference must keep the member frame"
    );
}

#[test]
fn type_assertion_source_keeps_member_frame() {
    // `as <type>` yields the asserted type, which is not fresh.
    let (_, chain) = chain_texts(
        r#"
type R = { p: 1; q: 2 } | { p: 3; q: 4 };
const r: R = { p: 1, q: 4 } as { p: 1; q: 4 };
"#,
        2322,
    );
    assert_eq!(
        chain,
        vec![
            (
                0,
                "Type '{ p: 1; q: 4; }' is not assignable to type '{ p: 1; q: 2; }'.".to_string()
            ),
            (1, "Types of property 'q' are incompatible.".to_string()),
            (2, "Type '4' is not assignable to type '2'.".to_string()),
        ],
        "an `as <type>` source is not fresh and must keep the member frame"
    );
}

#[test]
fn excess_property_check_still_fires_through_as_const() {
    let diags = check_source_diagnostics(
        r#"
type S = { a: 1 };
const s: S = { a: 1, b: 2 } as const;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "excess property check must fire through as const, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "EPC replaces the assignability error, got: {diags:?}"
    );
}

#[test]
fn as_const_declared_type_keeps_property_literals() {
    // Freshness surviving the assertion must not re-enable literal widening at
    // the declaration: `tmp.p` stays `1`.
    let (head, _) = chain_texts(
        r#"
const tmp = { p: 1 } as const;
const chk: 2 = tmp.p;
"#,
        2322,
    );
    assert!(
        head.contains("Type '1' is not assignable to type '2'."),
        "as-const property literal must be preserved at the declared type, got: {head}"
    );
}
