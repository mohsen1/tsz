//! Anchor tests for TS2769 when no overload matches and the overloads
//! disagree on which argument sub-node they reject.
//!
//! tsc's rule (`reportCallResolutionErrors` -> `candidatesForArgumentError`):
//! the top-level TS2769's span is the *last* argument-error candidate's
//! `isSignatureApplicable` elaboration node — i.e. the first argument that is
//! not assignable to that last candidate's parameters, drilled into the exact
//! failing sub-node. When overloads reject *different* excess properties of the
//! same object literal, the anchor is the last overload's rejected property,
//! not the callee.
//!
//! Baseline this locks in (tsc 7.0.2, `scripts/conformance/tsc-cache-full.json`):
//!   `orderMattersForSignatureGroupIdentity.ts(19,5)`: TS2769 anchors at the
//!   property `s` inside `v({ s: "", n: 0 })` — the property the *last*
//!   overload `(x: { n: number })` rejects as excess — not at the callee `v`
//!   (column 1) and not at the opening brace.

fn get_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", Default::default())
        .into_iter()
        .map(|d| (d.code, d.start, d.message_text))
        .collect()
}

#[test]
fn ts2769_anchored_at_last_overloads_rejected_property_when_overloads_disagree() {
    // Two overloads, each rejecting a different excess property on the same
    // object literal. tsc anchors the top-level TS2769 at the *last* overload's
    // rejected property (`(x: { n: number })` rejects `s` as excess), matching
    // `orderMattersForSignatureGroupIdentity.ts(19,5)` — not the callee `v`.
    let source = r#"interface A {
    (x: { s: string }): string
    (x: { n: number }): number
}
declare var v: A;
v({ s: "", n: 0 });
"#;
    let diags = get_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got {diags:#?}");
    let callee_start = source
        .find("v({ s:")
        .expect("callee start must exist in fixture") as u32;
    let property_start = source
        .find("s: \"\"")
        .expect("first property `s` must exist") as u32;
    assert_ne!(
        ts2769[0].1, callee_start,
        "TS2769 must not anchor at the callee `v`; got start={}",
        ts2769[0].1
    );
    assert_eq!(
        ts2769[0].1, property_start,
        "TS2769 should anchor at the object literal's first property `s` (the \
         last overload's excess-property culprit); got start={}",
        ts2769[0].1
    );
}

#[test]
fn ts2769_still_anchored_at_argument_when_overloads_agree() {
    // Two overloads that reject the argument with the *same* rendered message
    // (both expect the same `string` parameter — the overloads differ only in
    // return type via generics or declarations, not in the argument shape).
    // The argument is the single culprit → anchor should stay on the argument.
    let source = r#"interface A {
    (x: string): string
    (x: string): number
}
declare var f: A;
f(42);
"#;
    let diags = get_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got {diags:#?}");
    let argument_start = source.find("42").expect("argument start must exist") as u32;
    let callee_start = source.find("f(42)").expect("callee start must exist") as u32;
    // When overloads agree on the failure, tsz anchors at the argument.
    assert!(
        ts2769[0].1 == argument_start || ts2769[0].1 == callee_start,
        "TS2769 should anchor at callee or argument for identical-failure overloads; got start={}",
        ts2769[0].1
    );
    // Specifically, the existing behavior for agreeing overloads is argument-anchor;
    // this locks that in so our change does not broaden the callee-anchor path.
    assert_eq!(
        ts2769[0].1, argument_start,
        "TS2769 should stay at argument when overloads produce identical failure messages"
    );
}

/// Structural rule: when an overloaded *property-access* call fails with
/// argument-type mismatches that all point at the same source-order argument,
/// tsc anchors the TS2769 at that first failing argument — never at the callee
/// property name. This mirrors `getSignatureApplicabilityError`, which stops at
/// the first argument that is not assignable to the parameter.
///
/// The regression this guards against: a generic argument (`a: T`) whose
/// type-parameter `TypeId` differs from the one the solver reports in the
/// failure. Type-identity matching cannot pick the argument, so the anchor
/// previously collapsed to the callee for property-access calls with more than
/// one argument (the `Object.assign(a, b)` shape from
/// `unionAndIntersectionInference1`).
///
/// `assert_arg_anchored` is run with two different type-parameter name choices
/// to prove the fix is name-agnostic (§25): renaming `T`/`U` to `K`/`V` must
/// not change the anchor.
fn assert_first_argument_anchored(type_param_a: &str, type_param_b: &str) {
    let source = format!(
        r#"interface Asn {{
    (target: {{}}, source: string): {{}};
    (target: object, source: number): number;
}}
interface Holder {{ asn: Asn; }}
declare var h: Holder;
const wrap = <{type_param_a}, {type_param_b}>(x: {type_param_a}, y: {type_param_b}) => h.asn(x, y);
"#,
    );
    let diags = get_diagnostics(&source);
    let ts2769: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got {diags:#?}");

    // The first argument `x` inside `h.asn(x, y)`.
    let call_open = source.find("h.asn(").expect("call must exist");
    let first_arg_start = (call_open + "h.asn(".len()) as u32;
    // The callee property name `asn` — the position the bug anchored at.
    let property_name_start = (source.find("h.asn(").unwrap() + "h.".len()) as u32;

    assert_ne!(
        ts2769[0].1, property_name_start,
        "TS2769 must not anchor at the callee property name `asn`; got start={}",
        ts2769[0].1
    );
    assert_eq!(
        ts2769[0].1, first_arg_start,
        "TS2769 should anchor at the first failing argument for a property-access \
         overloaded call; got start={}",
        ts2769[0].1
    );
}

#[test]
fn ts2769_anchored_at_first_generic_argument_for_property_access_call() {
    // Default `T`/`U` spelling.
    assert_first_argument_anchored("T", "U");
    // Renamed bound variables must produce the same anchor (name-agnostic).
    assert_first_argument_anchored("K", "V");
}
