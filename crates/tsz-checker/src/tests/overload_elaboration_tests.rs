//! Regression tests for tsc's per-overload `TS2772`/`TS2770` elaboration under
//! a `TS2769` ("No overload matches this call.") diagnostic.
//!
//! When a call matches no overload, tsc elaborates the candidates that matched
//! arity but failed argument checks (`candidatesForArgumentError`): 2 or 3
//! such candidates each get an `Overload {i} of {N}, '{signature}', gave the
//! following error.` chain node (`TS2772`) — in declaration order, with the
//! applicability error nested one level deeper — and four or more collapse to
//! a single `The last overload gave the following error.` node (`TS2770`)
//! wrapping only the last candidate. `{i}` is the candidate's 1-based position
//! among the argument-error candidates while `{N}` counts every overload;
//! arity-failing overloads never appear in the chain but still count toward
//! `{N}`. A single argument-error candidate collapses to a plain `TS2345`
//! (no `TS2769`).
//!
//! Before the fix tsz defined the `TS2772`/`TS2770` strings but never emitted
//! them: it flattened every candidate's argument error directly under `TS2769`
//! and, due to the related-info `(file, start, depth, message)` sort, rendered
//! them out of declaration order.
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

/// Chain rendering must not perturb checking of the enclosing expression:
/// rendering a nested call's overload failure runs display recovery, which
/// may only *read* already-computed node types. Forcing a fresh computation
/// re-entered the still-unresolved outer `.map(...)` call and typed its
/// callback without a contextual type, leaking a spurious `TS7006`
/// (regressed `arrayConcatMap.ts` in conformance).
#[test]
fn chain_rendering_does_not_leak_diagnostics_into_enclosing_call() {
    let diags = check_source_diagnostics(
        r#"
interface Out {
    pick(cb: (value: string) => void): Out;
}
declare function make(items: { tag: number }[]): Out;
declare function make(items: { tag: string }[]): Out;
declare const seed: { other: boolean }[];
var r = make(seed).pick(v => {});
"#,
    );

    assert!(
        !diags.iter().any(|d| d.code == 7006),
        "chain rendering must not leak TS7006 into the enclosing call, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        diags.iter().any(|d| d.code == 2769),
        "the nested no-overload-match itself must still report, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Assert the `TS2770` collapse shape: exactly one header line plus the last
/// candidate's `TS2345` naming `last_param_ty` — no `TS2772` headers, no
/// lines from the earlier candidates (differential-verified against
/// tsc 6.0.2).
fn assert_last_overload_collapse(source: &str, last_param_ty: &str) {
    let chain = overload_chain(source);
    assert_eq!(
        chain,
        vec![
            (
                0,
                2770,
                "The last overload gave the following error.".to_string()
            ),
            (
                1,
                2345,
                format!(
                    "Argument of type 'symbol' is not assignable to parameter of type '{last_param_ty}'."
                )
            ),
        ],
        "expected a single TS2770 chain holding only the last candidate's error"
    );
}

/// Four or more argument-error candidates collapse to a single `TS2770`
/// header wrapping only the *last* candidate's error.
#[test]
fn four_or_more_overloads_collapse_to_last_overload_header() {
    assert_last_overload_collapse(
        r#"
declare function f(x: number): number;
declare function f(x: string): string;
declare function f(x: boolean[]): boolean;
declare function f(x: object): symbol;
declare const sym: symbol;
f(sym);
"#,
        "object",
    );
}

/// Shared scenario for the arity-exclusion rule: a `string`/arity/`boolean`
/// overload triple called with a `symbol` argument. The arity candidate is
/// excluded from the chain but still counts toward `{N}`, so the third
/// declaration renders as `Overload 2 of 3` and the arity error line
/// disappears entirely (differential-verified against tsc 6.0.2). Invoked
/// with two distinct binder-name sets so the rule is proven structural.
fn assert_arity_exclusion_chain(callee: &str, param: &str, extra: &str) {
    let chain = overload_chain(&format!(
        r#"
declare function {callee}({param}: string): void;
declare function {callee}({param}: number, {extra}: number): void;
declare function {callee}({param}: boolean): void;
declare const sym: symbol;
{callee}(sym);
"#
    ));

    assert_eq!(
        chain,
        vec![
            (
                0,
                2772,
                format!("Overload 1 of 3, '({param}: string): void', gave the following error.")
            ),
            (
                1,
                2345,
                "Argument of type 'symbol' is not assignable to parameter of type 'string'."
                    .to_string()
            ),
            (
                0,
                2772,
                format!("Overload 2 of 3, '({param}: boolean): void', gave the following error.")
            ),
            (
                1,
                2345,
                "Argument of type 'symbol' is not assignable to parameter of type 'boolean'."
                    .to_string()
            ),
        ],
        "expected the arity candidate dropped from the chain yet counted in {{N}}"
    );
}

/// An arity-failing overload interleaved among argument-error candidates is
/// excluded from the chain but still counts toward `{N}`.
#[test]
fn arity_failing_overload_is_excluded_from_chain_but_counted_in_total() {
    assert_arity_exclusion_chain("g", "a", "b");
}

/// Anti-hardcoding: the exclusion and numbering are structural — renaming the
/// callee and every binder changes nothing but the rendered signature text.
#[test]
fn collapse_and_exclusion_are_independent_of_binder_names() {
    assert_arity_exclusion_chain("visitNode", "entry", "extra");
}

/// The `TS2770` collapse keys on the *argument-error* candidate count, not the
/// raw failure count: four argument-error candidates plus an interleaved
/// arity failure still collapse, and the last *argument-error* candidate in
/// declaration order is the one shown.
#[test]
fn four_argument_error_candidates_plus_arity_still_collapse() {
    assert_last_overload_collapse(
        r#"
declare function r(a: string): void;
declare function r(a: number, b: number): void;
declare function r(a: boolean): void;
declare function r(a: object): void;
declare function r(a: number[]): void;
declare const sym: symbol;
r(sym);
"#,
        "number[]",
    );
}

/// The collapsed last candidate keeps its relation reason chain nested under
/// its applicability error, exactly as a `TS2772`-wrapped candidate does
/// (differential-verified against tsc 6.0.2).
#[test]
fn last_overload_collapse_nests_reason_chain() {
    let chain = chain_lines(
        r#"
declare function q(cb: (x: string) => void): void;
declare function q(cb: (x: number) => void): void;
declare function q(cb: (x: boolean) => void): void;
declare function q(cb: (x: object) => void): void;
q((x: symbol) => {});
"#,
    );

    assert_eq!(
        as_pairs(&chain),
        vec![
            (0, "The last overload gave the following error."),
            (
                1,
                "Argument of type '(x: symbol) => void' is not assignable to parameter of type '(x: object) => void'."
            ),
            (2, "Types of parameters 'x' and 'x' are incompatible."),
            (3, "Type 'object' is not assignable to type 'symbol'."),
        ],
        "expected the last candidate's reason chain under the TS2770 header"
    );
}

/// Constructor (`new`) overload sets collapse through the same policy.
#[test]
fn constructor_overloads_collapse_to_last_overload_header() {
    assert_last_overload_collapse(
        r#"
interface Maker {
    new (a: string): object;
    new (a: number): object;
    new (a: boolean[]): object;
    new (a: object): object;
}
declare const Maker: Maker;
declare const sym: symbol;
new Maker(sym);
"#,
        "object",
    );
}

/// A lone argument-error candidate among arity-failing overloads still
/// collapses to a plain `TS2345` (tsc reports the single applicability error
/// directly, with no `TS2769` head), regardless of how many overloads failed
/// on arity.
#[test]
fn lone_argument_error_candidate_with_arity_failures_stays_plain_ts2345() {
    let diags = check_source_diagnostics(
        r#"
declare function p(a: string): void;
declare function p(a: number, b: number): void;
declare function p(a: boolean, b: boolean, c: boolean): void;
declare const sym: symbol;
p(sym);
"#,
    );

    assert!(
        diags.iter().any(|d| d.code == 2345),
        "expected a plain TS2345, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2769),
        "a lone argument-error candidate must not produce TS2769, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
