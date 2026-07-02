//! TS2322 tuple length-mismatch elaboration parity with `tsc`.
//!
//! When a closed tuple source has a different fixed length than a tuple target,
//! `tsc` keeps the `TS2322` headline and attaches a nested reason line:
//!   - source longer than target allows -> `Source has N element(s) but target
//!     allows only M.` (`TS2619`)
//!   - source shorter than target requires -> `Source has N element(s) but
//!     target requires M.` (`TS2618`)
//!
//! The solver already produces the arity `SubtypeFailureReason`; this exercises
//! the checker render boundary that previously dropped the reason at the
//! top-level (`depth == 0`) and rendered non-`tsc` wording when nested.
//!
//! Binder/type-parameter names are varied across cases so the rendering is
//! proven structural, not keyed on a fixture identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn ts2322(diags: &[Diagnostic]) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

/// Return the message of the first related-information line carrying `code`,
/// or panic with the observed related lines for a legible failure.
fn related_msg(diag: &Diagnostic, code: u32) -> &str {
    diag.related_information
        .iter()
        .find(|r| r.code == code)
        .unwrap_or_else(|| {
            panic!(
                "expected a related TS{code} line; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        })
        .message_text
        .as_str()
}

/// A closed target with a *trailing optional* element reports its **minimum**
/// required length, not its raw slot count. `[alpha: number, beta?: string]`
/// requires 1, so an empty source is `Source has 0 element(s) but target
/// requires 1.` (previously mis-reported "requires 2", counting the optional
/// slot). Regression for the closed-tuple arity gate (`TS2618`).
#[test]
fn closed_target_trailing_optional_reports_min_length() {
    let source = r#"
type Empty = [];
const pair: [alpha: number, beta?: string] = (null as unknown as Empty);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2618),
        "Source has 0 element(s) but target requires 1."
    );
}

/// A closed target with several trailing optionals still reports the minimum
/// (`[first, second, third?, fourth?]` requires 2). A one-element source is
/// `Source has 1 element(s) but target requires 2.` (previously "requires 4").
#[test]
fn closed_target_multiple_trailing_optionals_reports_min_length() {
    let source = r#"
type Solo = [number];
const quad: [first: number, second: number, third?: number, fourth?: number] =
    (null as unknown as Solo);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2618),
        "Source has 1 element(s) but target requires 2."
    );
}

/// An all-optional source that is longer than a closed target reports the
/// target's minimum with the "source may have fewer" wording (`TS2620`),
/// because the source is not guaranteed to supply the required elements.
#[test]
fn all_optional_longer_source_reports_target_requires_may_have_fewer() {
    let source = r#"
type Loose = [first?: number, second?: number, third?: number];
const strict: [x: number, y: number] = (null as unknown as Loose);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2620),
        "Target requires 2 element(s) but source may have fewer."
    );
}

/// A source that satisfies the target's minimum but is longer, and whose extra
/// element is optional, reports the "source may have more" wording (`TS2621`).
#[test]
fn longer_source_with_trailing_optional_reports_target_allows_only_may_have_more() {
    let source = r#"
type Extra = [one: number, two: number, three?: number];
const shorter: [x: number, y: number] = (null as unknown as Extra);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2621),
        "Target allows only 2 element(s) but source may have more."
    );
}

/// When arities are compatible but a source element is optional at a position
/// the target requires, tsc reports the element-flag mismatch (`TS2623`)
/// *ahead of* any element-type comparison — the source may not provide a value
/// at that position at all.
#[test]
fn optional_source_element_at_required_target_position_reports_no_match() {
    let source = r#"
type Half = [head: number, tail?: number];
const full: [x: number, y: number] = (null as unknown as Half);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2623),
        "Source provides no match for required element at position 1 in target."
    );
}

/// Anti-regression: a physically-present array-*literal* element is Required,
/// so an over-long literal against a closed target with an optional slot still
/// reports `TS2619` ("Source has N …but target allows only M"), not the
/// variadic-flavored `TS2621`. The literal `[1, "x", true]` has minimum length
/// 3 regardless of the target's optional second slot.
#[test]
fn overlong_array_literal_against_optional_target_reports_allows_only() {
    let diags = check_strict("const t: [number, string?] = [1, \"x\", true];\n");
    let diag = ts2322(&diags);
    assert_eq!(
        related_msg(diag, 2619),
        "Source has 3 element(s) but target allows only 2."
    );
}

/// Source longer than a closed target -> `target allows only M` (`TS2619`).
#[test]
fn too_many_elements_attaches_allows_only_reason() {
    let diags = check_strict("const pair: [number] = [1, 2];\n");
    let diag = ts2322(&diags);
    assert!(
        diag.message_text
            .contains("Type '[number, number]' is not assignable to type '[number]'"),
        "headline should be the assignability message; got: {}",
        diag.message_text
    );
    let reason = diag
        .related_information
        .iter()
        .find(|r| r.code == 2619)
        .unwrap_or_else(|| {
            panic!(
                "expected a TS2619 arity reason; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        reason.message_text,
        "Source has 2 element(s) but target allows only 1."
    );
}

/// Source shorter than a closed target -> `target requires M` (`TS2618`).
#[test]
fn too_few_elements_attaches_requires_reason() {
    let diags = check_strict("const triple: [number, string, boolean] = [1, \"x\"];\n");
    let diag = ts2322(&diags);
    let reason = diag
        .related_information
        .iter()
        .find(|r| r.code == 2618)
        .unwrap_or_else(|| {
            panic!(
                "expected a TS2618 arity reason; related: {:?}",
                diag.related_information
                    .iter()
                    .map(|r| (r.code, &r.message_text))
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        reason.message_text,
        "Source has 2 element(s) but target requires 3."
    );
}

/// A renamed tuple alias on the source side keeps the alias headline AND still
/// carries the arity reason — proves the reason is not keyed to literal sources.
#[test]
fn aliased_tuple_source_still_carries_arity_reason() {
    let source = r#"
type Coords = [number, number];
const single: [number] = (null as unknown as Coords);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert!(
        diag.message_text.contains("Type 'Coords'"),
        "source alias name must be preserved in the headline; got: {}",
        diag.message_text
    );
    assert!(
        diag.related_information.iter().any(|r| r.code == 2619
            && r.message_text == "Source has 2 element(s) but target allows only 1."),
        "aliased source must still attach the arity reason; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

/// A union source whose failing member is a closed-tuple arity mismatch must
/// elaborate that member beneath the union line — the member header
/// (`Type '[2, 3]' is not assignable to type '[number]'.`) and the arity leaf
/// (`Source has 2 element(s) but target allows only 1.`), matching tsc. The
/// union elaboration previously dropped this whole chain, leaving only the
/// bare `Type '[2, 3] | [4]' …` headline.
#[test]
fn union_member_closed_tuple_arity_elaborates_with_header_and_leaf() {
    // `Pick`-free distributive identity keeps `[2, 3] | [4]` as a written union
    // member set so the failing `[2, 3]` member survives to elaboration.
    let source = r#"
type Identity<T> = T extends unknown ? T : never;
const target: [number] = (null as unknown as Identity<[2, 3] | [4]>);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert!(
        diag.message_text
            .contains("is not assignable to type '[number]'"),
        "headline targets the 1-tuple; got: {}",
        diag.message_text
    );
    let related: Vec<(u32, &str)> = diag
        .related_information
        .iter()
        .map(|r| (r.code, r.message_text.as_str()))
        .collect();
    assert!(
        related.iter().any(|(code, msg)| *code == 2322
            && msg.contains("Type '[2, 3]'")
            && msg.contains("is not assignable to type '[number]'")),
        "expected the failing-member header line; related: {related:?}"
    );
    assert!(
        related.iter().any(|(code, msg)| *code == 2619
            && *msg == "Source has 2 element(s) but target allows only 1."),
        "expected the nested arity leaf; related: {related:?}"
    );
}

/// The member header sits one indent above the arity leaf (header depth < leaf
/// depth), so the chain reads union -> member -> arity rather than collapsing.
#[test]
fn union_member_tuple_arity_chain_is_nested_in_order() {
    let source = r#"
type Pass<T> = T extends unknown ? T : never;
const target: [string] = (null as unknown as Pass<[string, string] | [boolean]>);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    let header = diag
        .related_information
        .iter()
        .find(|r| r.code == 2322 && r.message_text.contains("Type '[string, string]'"))
        .expect("member header present");
    let leaf = diag
        .related_information
        .iter()
        .find(|r| r.code == 2619)
        .expect("arity leaf present");
    assert!(
        leaf.depth > header.depth,
        "arity leaf must nest beneath the member header; header depth {} leaf depth {}",
        header.depth,
        leaf.depth
    );
}

/// Renamed binders/aliases must not change the structural elaboration — proves
/// the chain is keyed on shape, not identifiers.
#[test]
fn union_member_tuple_arity_elaboration_is_structural_under_renaming() {
    let source = r#"
type Echo<Element> = Element extends unknown ? Element : never;
type WideRow = [number, number, number];
type NarrowRow = [number];
const slot: NarrowRow = (null as unknown as Echo<WideRow | [number]>);
"#;
    let diags = check_strict(source);
    let diag = ts2322(&diags);
    assert!(
        diag.related_information.iter().any(|r| r.code == 2619
            && r.message_text == "Source has 3 element(s) but target allows only 1."),
        "renamed aliases must still attach the arity leaf; related: {:?}",
        diag.related_information
            .iter()
            .map(|r| (r.code, &r.message_text))
            .collect::<Vec<_>>()
    );
}

/// Anti-regression: a union whose members are all assignable must not error.
#[test]
fn union_member_tuple_all_assignable_has_no_arity_diagnostic() {
    let source = r#"
type Keep<T> = T extends unknown ? T : never;
const ok: [number] = (null as unknown as Keep<[1] | [2]>);
"#;
    let diags = check_strict(source);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "all-assignable union members must not error; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Anti-regression: matching arity must NOT produce a TS2322 or a spurious
/// arity reason.
#[test]
fn matching_arity_has_no_tuple_arity_diagnostic() {
    let diags = check_strict("const exact: [number, string] = [1, \"x\"];\n");
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "matching-arity tuple assignment must not error; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        !diags
            .iter()
            .flat_map(|d| d.related_information.iter())
            .any(|r| r.code == 2618 || r.code == 2619),
        "matching-arity assignment must not emit an arity reason"
    );
}
