//! Regression tests for the *return-type* elaboration of a structural member
//! whose two signatures differ only in what they return.
//!
//! Structural rule: when an object member's relation fails on its return type,
//! `tsc` renders one of two shapes and never the historical
//! `Return type 'X' is not assignable to 'Y'.` phrasing (which appears in zero
//! `tsc` baselines):
//!
//! Both method syntax (`f(): T`) and function-typed-property syntax
//! (`f: () => T`) collapse to the same TS2201 frame
//! (`The types returned by 'f()' are incompatible between these types.`) and
//! drill straight into the inner relation — the member relation reduces to a
//! call-signature comparison in either case. The name suffix is `()` when both
//! signatures take zero parameters and `(...)` when either carries parameters
//! (tsc `reportIncompatibleCallSignatureReturn`).
//!
//! Owner: the shared `relation -> reason -> diagnostic` render path
//! (`render_property_type_mismatch` -> `render_member_return_type_mismatch`),
//! so the same chain serves TS2322 (assignment) and TS2345 (call argument).
//!
//! The rule is structural, so the cases vary binder/member names where a name
//! reaches the rendered output.

use crate::test_utils::{check_with_options, strict_checker_options};

/// Full elaboration text (primary message plus every related-information line,
/// in order) of the single diagnostic with `code` in `source`, under strict
/// options.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_with_options(source, strict_checker_options());
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// Method syntax, direct assignment: the TS2201 head replaces the
/// `Types of property` frame and drills straight into the inner leaf — no
/// `Return type ...` line, no duplicated trailing leaf.
#[test]
fn method_return_mismatch_uses_types_returned_by_head() {
    let text = elaboration(
        r#"
interface Source { compute(): string; }
interface Target { compute(): number; }
declare const src: Source;
const tgt: Target = src;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Source' is not assignable to type 'Target'.\n\
         The types returned by 'compute()' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// Same rule, different binder/member names — locks the shape as structural,
/// not a fixture spelling (CLAUDE.md anti-hardcoding gate).
#[test]
fn method_return_mismatch_member_name_independent() {
    let text = elaboration(
        r#"
interface Producer { emit(): boolean; }
interface Consumer { emit(): string; }
declare const p: Producer;
const c: Consumer = p;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Producer' is not assignable to type 'Consumer'.\n\
         The types returned by 'emit()' are incompatible between these types.\n\
         Type 'boolean' is not assignable to type 'string'.",
    );
}

/// Property syntax (function-typed property) collapses to the SAME TS2201 head
/// as method syntax — tsc drills straight into the return relation and never
/// keeps the `Types of property` header or the `() => S` / `() => T`
/// function-type line for a return-only mismatch.
#[test]
fn property_function_return_mismatch_uses_types_returned_by_head() {
    let text = elaboration(
        r#"
interface Source { compute: () => string; }
interface Target { compute: () => number; }
declare const src: Source;
const tgt: Target = src;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Source' is not assignable to type 'Target'.\n\
         The types returned by 'compute()' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// When either signature carries parameters the name suffix is `(...)`, for both
/// method and function-typed-property syntax (tsc
/// `reportIncompatibleCallSignatureReturn`). Params are compatible; only the
/// return differs.
#[test]
fn method_with_parameters_return_mismatch_uses_ellipsis_suffix() {
    let text = elaboration(
        r#"
interface Source { compute(flag: boolean): string; }
interface Target { compute(flag: boolean): number; }
declare const src: Source;
const tgt: Target = src;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Source' is not assignable to type 'Target'.\n\
         The types returned by 'compute(...)' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

#[test]
fn property_function_with_parameters_return_mismatch_uses_ellipsis_suffix() {
    let text = elaboration(
        r#"
interface Source { compute: (flag: boolean) => string; }
interface Target { compute: (flag: boolean) => number; }
declare const src: Source;
const tgt: Target = src;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Source' is not assignable to type 'Target'.\n\
         The types returned by 'compute(...)' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// A parameter mismatch (not a return mismatch) must NOT collapse — it keeps the
/// `Types of property` / function-type elaboration, matching tsc. Guards against
/// over-broadening the return-type collapse.
#[test]
fn property_function_parameter_mismatch_keeps_generic_elaboration() {
    let diags = check_with_options(
        r#"
interface Source { consume: (x: string) => void; }
interface Target { consume: (x: number) => void; }
declare const src: Source;
const tgt: Target = src;
"#,
        strict_checker_options(),
    );
    let ts2322 = diags
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    let lines: Vec<&str> = std::iter::once(ts2322.message_text.as_str())
        .chain(
            ts2322
                .related_information
                .iter()
                .map(|i| i.message_text.as_str()),
        )
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Types of property 'consume' are incompatible")),
        "parameter mismatch keeps the property header, got: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| !l.contains("The types returned by")),
        "parameter mismatch must not collapse to the returns frame, got: {lines:?}"
    );
}

/// The historical `Return type 'X' is not assignable to 'Y'.` phrasing — which
/// `tsc` never emits — must not appear for either member form, and the inner
/// leaf must appear exactly once (the prior double-elaboration emitted a stray
/// sibling copy).
#[test]
fn method_return_mismatch_no_return_type_phrasing_and_no_duplicate_leaf() {
    let diags = check_with_options(
        r#"
interface Source { compute(): string; }
interface Target { compute(): number; }
declare const src: Source;
const tgt: Target = src;
"#,
        strict_checker_options(),
    );
    let ts2322 = diags
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322");
    let lines: Vec<&str> = std::iter::once(ts2322.message_text.as_str())
        .chain(
            ts2322
                .related_information
                .iter()
                .map(|i| i.message_text.as_str()),
        )
        .collect();
    assert!(
        lines.iter().all(|l| !l.starts_with("Return type ")),
        "tsc never emits the 'Return type ...' framing, got: {lines:?}"
    );
    let leaf_count = lines
        .iter()
        .filter(|l| l.contains("Type 'string' is not assignable to type 'number'."))
        .count();
    assert_eq!(
        leaf_count, 1,
        "inner leaf must appear exactly once (no duplicate sibling), got: {lines:?}"
    );
}

/// Call-argument surface (TS2345) shares the same render path, so a method
/// return mismatch on an argument produces the same TS2201 chain.
#[test]
fn method_return_mismatch_on_call_argument_uses_same_chain() {
    let text = elaboration(
        r#"
interface Source { compute(): string; }
interface Target { compute(): number; }
declare const src: Source;
declare function take(t: Target): void;
take(src);
"#,
        2345,
    );
    assert_eq!(
        text,
        "Argument of type 'Source' is not assignable to parameter of type 'Target'.\n\
         The types returned by 'compute()' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// Nested one level deep: an outer property whose own type carries the failing
/// method. The method head must sit beneath the outer `Types of property`
/// frame, with the inner leaf one level deeper still.
#[test]
fn nested_method_return_mismatch_indents_under_outer_property() {
    let text = elaboration(
        r#"
interface Inner { compute(): string; }
interface InnerN { compute(): number; }
interface Outer { node: Inner; }
interface OuterN { node: InnerN; }
declare const o: Outer;
const n: OuterN = o;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Outer' is not assignable to type 'OuterN'.\n\
         Types of property 'node' are incompatible.\n\
         The types returned by 'compute()' are incompatible between these types.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}

/// When the return relation is itself structural (the returned objects differ in
/// a property), the TS2201 head drills into that property chain rather than a
/// scalar leaf.
#[test]
fn method_return_mismatch_drills_structural_return() {
    let text = elaboration(
        r#"
interface Source { build(): { value: string }; }
interface Target { build(): { value: number }; }
declare const src: Source;
const tgt: Target = src;
"#,
        2322,
    );
    assert_eq!(
        text,
        "Type 'Source' is not assignable to type 'Target'.\n\
         The types returned by 'build()' are incompatible between these types.\n\
         Types of property 'value' are incompatible.\n\
         Type 'string' is not assignable to type 'number'.",
    );
}
