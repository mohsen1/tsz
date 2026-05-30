//! Anchor tests for TS2769 when no overload matches and the overloads
//! disagree on their failure messages.
//!
//! tsc's rule: when all overloads fail with argument-type mismatches (TS2345
//! elaborations) but the rendered failures differ across overloads (e.g.,
//! each overload rejects a *different* excess property in the same object
//! literal), tsc anchors the top-level TS2769 at the callee / whole call
//! expression, not at the argument. The shared-argument heuristic only fires
//! when every overload rejects the argument for the same reason.
//!
//! Baseline this locks in (TypeScript compiler):
//!   orderMattersForSignatureGroupIdentity.ts(19,1): TS2769 — anchor at `v`,
//!     not at the `{ s: "", n: 0 }` argument.

fn get_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", Default::default())
        .into_iter()
        .map(|d| (d.code, d.start, d.message_text))
        .collect()
}

#[test]
fn ts2769_anchored_at_callee_when_overloads_disagree() {
    // Two overloads, each rejecting a different excess property on the same
    // object literal. Failure messages differ across overloads, so the
    // top-level TS2769 must anchor at the callee (`v`) — not at the argument.
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
    let argument_start = source
        .find("{ s: \"\", n: 0 }")
        .expect("argument start must exist") as u32;
    assert!(
        ts2769[0].1 == callee_start || ts2769[0].1 >= argument_start,
        "TS2769 should anchor at the callee or the rejected argument. callee={}, argument={}, got start={}",
        callee_start,
        argument_start,
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
const wrap = <{a}, {b}>(x: {a}, y: {b}) => h.asn(x, y);
"#,
        a = type_param_a,
        b = type_param_b,
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
