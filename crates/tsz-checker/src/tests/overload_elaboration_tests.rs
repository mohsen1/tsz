//! Regression tests for tsc's per-overload `TS2772` elaboration under a
//! `TS2769` ("No overload matches this call.") diagnostic.
//!
//! When a call matches no overload and 2 or 3 candidate signatures reached
//! argument checking, tsc wraps each candidate's applicability error in a
//! `Overload {n} of {total}, '{signature}', gave the following error.` chain
//! node (`TS2772`) — in declaration order, with the applicability error nested
//! one level deeper. A single failing candidate collapses to a plain `TS2345`
//! (no `TS2769`), and `>3` candidates use tsc's distinct "last overload" shape,
//! left as the flat fallback here.
//!
//! Before the fix tsz defined the `TS2772` string but never emitted it: it
//! flattened every candidate's argument error directly under `TS2769` and, due
//! to the related-info `(file, start, depth, message)` sort, rendered them out
//! of declaration order.
//!
//! See `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
//! (`error_no_overload_matches_at`) and
//! `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`
//! (`RelatedInformationPolicy::OVERLOAD_CHAINS`).

use crate::test_utils::check_source_diagnostics;

/// The `(depth, code, message)` triples of the sole `TS2769` diagnostic's
/// related information, in emission order.
fn overload_chain(source: &str) -> Vec<(u8, u32, String)> {
    let diags = check_source_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "Expected exactly one TS2769. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2769[0]
        .related_information
        .iter()
        .map(|r| (r.depth, r.code, r.message_text.clone()))
        .collect()
}

/// Two failing overloads: both wrapped in `TS2772`, in declaration order, each
/// header at depth 0 with its applicability error nested at depth 1. The
/// signature is the `signatureToString` colon form, not the `=>` function-type
/// form.
#[test]
fn two_failing_overloads_wrap_each_candidate_in_declaration_order() {
    let chain = overload_chain(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
f(true);
"#,
    );

    assert_eq!(
        chain,
        vec![
            (
                0,
                2772,
                "Overload 1 of 2, '(x: number): number', gave the following error.".to_string()
            ),
            (
                1,
                2345,
                "Argument of type 'boolean' is not assignable to parameter of type 'number'."
                    .to_string()
            ),
            (
                0,
                2772,
                "Overload 2 of 2, '(x: string): string', gave the following error.".to_string()
            ),
            (
                1,
                2345,
                "Argument of type 'boolean' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
        ],
        "expected two declaration-ordered TS2772 chains with the colon signature form"
    );
}

/// Three failing overloads: all wrapped, `of 3`, and declaration order is
/// preserved rather than re-sorted. The bodies' alphabetical order
/// (`boolean` < `number` < `string`) differs from declaration order
/// (`number`, `string`, `boolean`); the fix must keep declaration order.
#[test]
fn three_failing_overloads_preserve_declaration_order() {
    let chain = overload_chain(
        r#"
declare function g(x: number): number;
declare function g(x: string): string;
declare function g(x: boolean): boolean;
g({});
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 3, '(x: number): number', gave the following error.",
            "Overload 2 of 3, '(x: string): string', gave the following error.",
            "Overload 3 of 3, '(x: boolean): boolean', gave the following error.",
        ],
        "expected declaration-ordered `of 3` headers, got: {chain:?}"
    );

    // Bodies stay under their header in declaration order (number, string,
    // boolean) — not the alphabetical (boolean, number, string) the old
    // unconditional sort produced.
    let bodies: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2345)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        bodies,
        vec![
            "Argument of type '{}' is not assignable to parameter of type 'number'.",
            "Argument of type '{}' is not assignable to parameter of type 'string'.",
            "Argument of type '{}' is not assignable to parameter of type 'boolean'.",
        ],
        "expected declaration-ordered bodies, got: {chain:?}"
    );
}

/// Anti-hardcoding: the wrapping is structural, independent of the callee and
/// parameter binder names. Renaming them only changes the rendered signature.
#[test]
fn wrapping_is_independent_of_binder_names() {
    let chain = overload_chain(
        r#"
declare function pick(value: number): number;
declare function pick(other: string): string;
pick(true);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '(value: number): number', gave the following error.",
            "Overload 2 of 2, '(other: string): string', gave the following error.",
        ],
        "signature renders the declared parameter names, got: {chain:?}"
    );
}

/// Constructor (`new`) overloads are wrapped identically, and tsc's
/// `signatureToString` renders them in the call-signature colon form
/// (`(x: number): C`) — no `new` prefix.
#[test]
fn constructor_overloads_wrap_in_call_signature_form() {
    let chain = overload_chain(
        r#"
declare class C {
    constructor(x: number);
    constructor(x: string);
}
new C(true);
"#,
    );

    let headers: Vec<&String> = chain
        .iter()
        .filter(|(_, code, _)| *code == 2772)
        .map(|(_, _, m)| m)
        .collect();
    assert_eq!(
        headers,
        vec![
            "Overload 1 of 2, '(x: number): C', gave the following error.",
            "Overload 2 of 2, '(x: string): C', gave the following error.",
        ],
        "constructor overloads render as call signatures returning the class, got: {chain:?}"
    );
    assert!(
        headers.iter().all(|h| !h.contains("new ")),
        "constructor overload headers must not carry a `new` prefix, got: {headers:?}"
    );
}

/// The literal-source generalization stays applied inside each wrapped error:
/// a fresh boolean-literal argument widens to `boolean` against non-singleton
/// parameters, exactly as on the single-overload TS2345 path.
#[test]
fn literal_source_generalization_survives_wrapping() {
    let chain = overload_chain(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
f(true);
"#,
    );

    assert!(
        chain
            .iter()
            .all(|(_, _, m)| !m.contains("Argument of type 'true'")),
        "the raw boolean literal must not leak into the wrapped elaboration, got: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|(_, code, m)| *code == 2345 && m.contains("Argument of type 'boolean'")),
        "expected the widened `boolean` source inside a wrapped body, got: {chain:?}"
    );
}

/// A single failing signature collapses to a plain `TS2345` with no `TS2769`
/// wrapper at all (tsc's single-candidate branch).
#[test]
fn single_signature_stays_plain_ts2345() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: number): number;
f(true);
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected a plain TS2345, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2769),
        "a single failing signature must not produce TS2769, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2772),
        "a single failing signature must not produce TS2772, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// The `(depth, message)` pairs of the chain, for tests that assert shape
/// and text but not per-line codes.
fn chain_lines(source: &str) -> Vec<(u8, String)> {
    overload_chain(source)
        .into_iter()
        .map(|(depth, _, message)| (depth, message))
        .collect()
}

/// Borrowing view of a chain for comparison against literal expectations.
fn as_pairs(chain: &[(u8, String)]) -> Vec<(u8, &str)> {
    chain.iter().map(|(d, m)| (*d, m.as_str())).collect()
}

/// The messages of every chain line at exactly `depth`.
fn lines_at_depth(chain: &[(u8, String)], depth: u8) -> Vec<&str> {
    chain
        .iter()
        .filter(|(d, _)| *d == depth)
        .map(|(_, m)| m.as_str())
        .collect()
}

/// The witness from #15387: a callback parameter incompatibility nests the
/// full relation reason chain under each candidate's `TS2772` header — the
/// `Types of parameters 'a' and 'x' are incompatible.` frame and the
/// contravariant leaf — exactly as the single-signature `TS2345` path
/// renders them (differential-verified against tsc 6.0.2).
#[test]
fn callback_parameter_mismatch_nests_reason_chain_per_candidate() {
    let chain = chain_lines(
        r#"
declare function each(cb: (x: string) => void): void;
declare function each(cb: (x: number, i: number) => void): void;
each((a: boolean) => {});
"#,
    );

    assert_eq!(
        as_pairs(&chain),
        vec![
            (
                0,
                "Overload 1 of 2, '(cb: (x: string) => void): void', gave the following error."
            ),
            (
                1,
                "Argument of type '(a: boolean) => void' is not assignable to parameter of type '(x: string) => void'."
            ),
            (2, "Types of parameters 'a' and 'x' are incompatible."),
            (3, "Type 'string' is not assignable to type 'boolean'."),
            (
                0,
                "Overload 2 of 2, '(cb: (x: number, i: number) => void): void', gave the following error."
            ),
            (
                1,
                "Argument of type '(a: boolean) => void' is not assignable to parameter of type '(x: number, i: number) => void'."
            ),
            (2, "Types of parameters 'a' and 'x' are incompatible."),
            (3, "Type 'number' is not assignable to type 'boolean'."),
        ],
        "expected the full per-candidate reason chain under each TS2772 header"
    );
}

/// A missing-property failure nests its `Property 'p' is missing …` line
/// under each candidate, and the two candidates keep their distinct leaves
/// (dedupe is off under `OVERLOAD_CHAINS`).
#[test]
fn missing_property_reason_chain_nests_under_each_header() {
    let chain = chain_lines(
        r#"
declare function take(o: { alpha: string }): void;
declare function take(o: { beta: number }): void;
declare const arg: { gamma: boolean };
take(arg);
"#,
    );

    assert_eq!(
        lines_at_depth(&chain, 2),
        vec![
            "Property 'alpha' is missing in type '{ gamma: boolean; }' but required in type '{ alpha: string; }'.",
            "Property 'beta' is missing in type '{ gamma: boolean; }' but required in type '{ beta: number; }'.",
        ],
        "expected one missing-property leaf under each header, got: {chain:?}"
    );
}

/// Constructor (`new`) overload candidates nest the same reason chains as
/// call overloads.
#[test]
fn constructor_overloads_nest_reason_chains() {
    let chain = chain_lines(
        r#"
declare class Widget {
    constructor(cb: (x: string) => void);
    constructor(cb: (x: number) => void);
}
new Widget((a: boolean) => {});
"#,
    );

    let depth2plus: Vec<(u8, &str)> = as_pairs(&chain)
        .into_iter()
        .filter(|(depth, _)| *depth >= 2)
        .collect();
    assert_eq!(
        depth2plus,
        vec![
            (2, "Types of parameters 'a' and 'x' are incompatible."),
            (3, "Type 'string' is not assignable to type 'boolean'."),
            (2, "Types of parameters 'a' and 'x' are incompatible."),
            (3, "Type 'number' is not assignable to type 'boolean'."),
        ],
        "expected reason chains under both constructor candidates, got: {chain:?}"
    );
}

/// Overloaded *call signatures on an interface* resolve through the solver's
/// callable path; its candidate failures now carry their declared signature
/// too, so the set wraps and chains identically to declared function
/// overloads.
#[test]
fn callable_interface_overloads_wrap_and_chain() {
    let chain = chain_lines(
        r#"
interface Callable {
    (cb: (x: string) => void): void;
    (cb: (x: number, i: number) => void): void;
}
declare const invoke: Callable;
invoke((a: boolean) => {});
"#,
    );

    assert_eq!(
        lines_at_depth(&chain, 0),
        vec![
            "Overload 1 of 2, '(cb: (x: string) => void): void', gave the following error.",
            "Overload 2 of 2, '(cb: (x: number, i: number) => void): void', gave the following error.",
        ],
        "expected TS2772 headers on the callable-interface path, got: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|(depth, m)| *depth == 2
                && m == "Types of parameters 'a' and 'x' are incompatible."),
        "expected the parameter-incompatibility frame on the callable-interface path, got: {chain:?}"
    );
}

/// A scalar leaf mismatch has no deeper elaboration in tsc; the wrapped
/// candidates must not grow synthetic chain lines below depth 1.
#[test]
fn scalar_leaf_candidates_carry_no_extra_chain() {
    let chain = chain_lines(
        r#"
declare function s(x: string): void;
declare function s(x: number): void;
s(true);
"#,
    );

    assert!(
        chain.iter().all(|(depth, _)| *depth <= 1),
        "scalar leaf candidates must not carry a reason chain, got: {chain:?}"
    );
}

/// Anti-hardcoding: the chain is structural — renaming the callee, callback
/// parameters, and target parameters only changes the rendered names.
#[test]
fn reason_chain_is_independent_of_binder_names() {
    let chain = chain_lines(
        r#"
declare function visit(handler: (item: string) => void): void;
declare function visit(handler: (entry: number, pos: number) => void): void;
visit((row: boolean) => {});
"#,
    );

    assert!(
        chain.iter().any(|(depth, m)| *depth == 2
            && m == "Types of parameters 'row' and 'item' are incompatible."),
        "expected the renamed-binder parameter frame, got: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|(depth, m)| *depth == 3
                && m == "Type 'string' is not assignable to type 'boolean'."),
        "expected the contravariant leaf under the renamed frame, got: {chain:?}"
    );
}

/// Tuple-argument candidates drill to the offending position under each
/// header, mirroring the single-signature TS2345 positional chain.
#[test]
fn tuple_argument_candidates_nest_positional_chains() {
    let chain = chain_lines(
        r#"
declare function pair(a: [string, number]): void;
declare function pair(a: [number, string]): void;
declare const t: [boolean, boolean];
pair(t);
"#,
    );

    assert_eq!(
        lines_at_depth(&chain, 2),
        vec![
            "Type at position 0 in source is not compatible with type at position 0 in target.",
            "Type at position 0 in source is not compatible with type at position 0 in target.",
        ],
        "expected a positional disambiguator under each header, got: {chain:?}"
    );
    assert!(
        chain
            .iter()
            .any(|(depth, m)| *depth == 3
                && m == "Type 'boolean' is not assignable to type 'string'."),
        "expected the position-0 leaf under the first header, got: {chain:?}"
    );
}

/// More than three failing candidates keep tsc's distinct top-level shape:
/// tsz leaves them as the flat fallback (no `TS2772` wrappers), and the
/// top-level `TS2769` is unchanged.
#[test]
fn more_than_three_overloads_stay_flat() {
    let diags = check_source_diagnostics(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
declare function f(x: boolean): boolean;
declare function f(x: symbol): symbol;
f({});
"#,
    );

    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "expected the top-level TS2769 to be unchanged"
    );
    assert!(
        !ts2769[0].related_information.iter().any(|r| r.code == 2772),
        "the >3-candidate case must not emit TS2772 wrappers, got: {:?}",
        ts2769[0]
            .related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}
