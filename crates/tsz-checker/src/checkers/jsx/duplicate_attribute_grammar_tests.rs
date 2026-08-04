//! TS17001 — `JSX elements cannot have multiple attributes with the same name.`
//!
//! Structural rule: when a JSX opening element repeats an attribute name, tsc
//! reports TS17001 on the *second* occurrence's name node and stops checking
//! that element's grammar; tsz does this in `check_grammar_jsx_element`, the
//! same per-element grammar pass that already owned TS17000.
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --jsx preserve --target es2022 --lib es2022`), the
//! version `scripts/conformance/typescript-versions.json` pins as `default`.
//!
//! The matrix deliberately varies the binder names (`a`/`b`, `alpha`/`beta`,
//! `data-x`, `ns:local`) so no expectation can be satisfied by matching a
//! particular identifier.

use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_source;

fn check_jsx(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;

    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_codes(source: &str) -> Vec<u32> {
    check_jsx(source).iter().map(|d| d.code).collect()
}

/// Preamble giving the unit harness (which runs with no lib) a JSX namespace.
/// `any`-typed intrinsics keep the element's *type* checking silent so each
/// assertion below observes the grammar pass alone.
const JSX_PREAMBLE: &str = r#"
        declare namespace JSX {
          interface Element { kind: string }
          interface IntrinsicElements {
            div: any;
            span: any;
          }
        }
"#;

fn check_jsx_with_preamble(body: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_jsx(&format!("{JSX_PREAMBLE}{body}"))
}

fn codes_with_preamble(body: &str) -> Vec<u32> {
    check_jsx_codes(&format!("{JSX_PREAMBLE}{body}"))
}

const TS17001: u32 =
    diagnostic_codes::JSX_ELEMENTS_CANNOT_HAVE_MULTIPLE_ATTRIBUTES_WITH_THE_SAME_NAME;
const TS17000: u32 = diagnostic_codes::JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION;

// ---------------------------------------------------------------------------
// Positive: the name repeats.
// ---------------------------------------------------------------------------

#[test]
fn repeated_attribute_name_on_a_self_closing_intrinsic_reports_ts17001() {
    let codes = codes_with_preamble(r#"const e = <div alpha="1" alpha="2" />;"#);
    assert_eq!(
        codes,
        vec![TS17001],
        "a repeated attribute name is TS17001 and nothing else"
    );
}

#[test]
fn repeated_attribute_name_on_a_paired_intrinsic_reports_ts17001() {
    let codes = codes_with_preamble(r#"const e = <span beta="1" beta="2">text</span>;"#);
    assert_eq!(
        codes,
        vec![TS17001],
        "the paired open/close form is checked the same as the self-closing form"
    );
}

/// tsc anchors on the *second* occurrence's name node
/// (`grammarErrorOnNode(name, ...)`), not on the first and not on the whole
/// attribute. The span must therefore cover exactly the repeated name.
#[test]
fn ts17001_anchors_on_the_second_occurrence_name_node() {
    let source = format!("{JSX_PREAMBLE}const e = <div gamma=\"1\" gamma=\"2\" />;");
    let diagnostics = check_jsx(&source);
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.code == TS17001)
        .expect("expected TS17001");

    let start = diagnostic.start as usize;
    let reported = &source[start..start + diagnostic.length as usize];
    assert_eq!(reported, "gamma", "TS17001 spans the repeated name only");

    let first = source.find("gamma").expect("first occurrence");
    assert!(
        start > first,
        "TS17001 must anchor on the SECOND occurrence (at {start}), not the first (at {first})"
    );
}

/// A component tag reaches a different resolution path than an intrinsic tag,
/// but tsc runs `checkGrammarJsxElement` for both.
#[test]
fn repeated_attribute_name_on_a_component_reports_ts17001() {
    let codes = codes_with_preamble(
        r#"
        function Widget(props: { delta?: string }): JSX.Element {
          return null as any;
        }
        const e = <Widget delta="1" delta="2" />;
        "#,
    );
    assert!(
        codes.contains(&TS17001),
        "a component tag gets the same grammar check as an intrinsic one, got: {codes:?}"
    );
}

/// tsc reports TS17001 from the grammar pass, which runs even when the tag
/// name itself fails to resolve — so the unresolved-tag TS2304 and TS17001
/// coexist rather than one suppressing the other.
#[test]
fn repeated_attribute_name_still_reports_when_the_tag_is_unresolved() {
    let codes = codes_with_preamble(r#"const e = <Missing epsilon="1" epsilon="2" />;"#);
    assert!(
        codes.contains(&TS17001),
        "the grammar pass does not depend on the tag resolving, got: {codes:?}"
    );
}

/// Shorthand (initializer-less) attributes are still named attributes.
#[test]
fn repeated_shorthand_attribute_name_reports_ts17001() {
    let codes = codes_with_preamble(r#"const e = <div zeta zeta />;"#);
    assert_eq!(
        codes,
        vec![TS17001],
        "an absent initializer must not skip the name comparison"
    );
}

/// Hyphenated names are ordinary JSX identifiers, not a special form.
#[test]
fn repeated_hyphenated_attribute_name_reports_ts17001() {
    let codes = codes_with_preamble(r#"const e = <div data-token="1" data-token="2" />;"#);
    assert_eq!(codes, vec![TS17001]);
}

/// A namespaced name collides only with the identical namespace *and* local
/// name. Keying on the local half alone (or on a null key, which is the shape
/// of tsc's own legacy `name.escapedText` read) would make every namespaced
/// pair collide — see the negative case below.
#[test]
fn repeated_namespaced_attribute_name_reports_ts17001() {
    let codes = codes_with_preamble(r#"const e = <div xlink:href="1" xlink:href="2" />;"#);
    assert_eq!(codes, vec![TS17001]);
}

/// A spread between two occurrences does NOT clear the seen-name set: tsc
/// `continue`s past a `JsxSpreadAttribute` without touching the map.
#[test]
fn a_spread_between_two_occurrences_does_not_reset_the_seen_set() {
    let codes = codes_with_preamble(
        r#"
        declare const rest: { theta?: string };
        const e = <div theta="1" {...rest} theta="2" />;
        "#,
    );
    assert!(
        codes.contains(&TS17001),
        "an intervening spread must not clear the seen names, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback: the name does not repeat.
// ---------------------------------------------------------------------------

#[test]
fn distinct_attribute_names_report_nothing() {
    let diagnostics = check_jsx_with_preamble(r#"const e = <div alpha="1" beta="2" gamma="3" />;"#);
    assert!(
        diagnostics.is_empty(),
        "distinct names are clean, got: {diagnostics:?}"
    );
}

/// Comparison is case-sensitive — JSX attribute names are not folded.
#[test]
fn attribute_names_differing_only_in_case_report_nothing() {
    let diagnostics = check_jsx_with_preamble(r#"const e = <div Alpha="1" alpha="2" />;"#);
    assert!(
        diagnostics.is_empty(),
        "`Alpha` and `alpha` are distinct names, got: {diagnostics:?}"
    );
}

/// Two namespaced names sharing a local half are distinct.
#[test]
fn namespaced_names_with_different_namespaces_report_nothing() {
    let diagnostics =
        check_jsx_with_preamble(r#"const e = <div one:shared="1" two:shared="2" />;"#);
    assert!(
        diagnostics.is_empty(),
        "`one:shared` and `two:shared` are distinct names, got: {diagnostics:?}"
    );
}

/// Repeated *spread* attributes are never duplicates — tsc skips them before
/// the name comparison, and they have no name to compare.
#[test]
fn repeated_spread_attributes_report_no_ts17001() {
    let codes = codes_with_preamble(
        r#"
        declare const rest: { iota?: string };
        const e = <div {...rest} {...rest} />;
        "#,
    );
    assert!(
        !codes.contains(&TS17001),
        "spreads carry no name and can never duplicate, got: {codes:?}"
    );
}

/// Each element gets its own seen-name set.
#[test]
fn the_same_name_on_sibling_elements_reports_nothing() {
    let diagnostics = check_jsx_with_preamble(
        r#"const e = <div kappa="1"><span kappa="2" /><span kappa="3" /></div>;"#,
    );
    assert!(
        diagnostics.is_empty(),
        "the seen-name set is per opening element, got: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Ordering: TS17001 vs TS17000, and the one-report-per-element rule.
// ---------------------------------------------------------------------------

/// tsc reports at most one grammar error per opening element and returns, so
/// only the FIRST repeat is reported even when several names repeat.
#[test]
fn only_the_first_repeated_name_is_reported_per_element() {
    let codes = codes_with_preamble(r#"const e = <div mu="1" nu="2" mu="3" xi="4" nu="5" />;"#);
    assert_eq!(
        codes,
        vec![TS17001],
        "one grammar report per element, anchored at the first repeat"
    );
}

#[test]
fn a_name_repeated_three_times_reports_ts17001_once() {
    let codes = codes_with_preamble(r#"const e = <div omicron="1" omicron="2" omicron="3" />;"#);
    assert_eq!(codes, vec![TS17001]);
}

/// Within one attribute tsc tests the name BEFORE the initializer, so a repeat
/// outranks that same attribute's empty `{}`.
#[test]
fn a_repeated_name_outranks_its_own_empty_initializer() {
    let codes = codes_with_preamble(r#"const e = <div pi="1" pi={} />;"#);
    assert_eq!(
        codes,
        vec![TS17001],
        "the duplicate-name test runs before the empty-expression test"
    );
}

/// Conversely, an earlier attribute's empty `{}` returns before a later pair
/// is ever compared — so TS17000 wins on ordering, not on precedence.
#[test]
fn an_earlier_empty_initializer_preempts_a_later_repeated_name() {
    let codes = codes_with_preamble(r#"const e = <div rho={} sigma="1" sigma="2" />;"#);
    assert_eq!(
        codes,
        vec![TS17000],
        "the first attribute's empty initializer returns before `sigma` repeats"
    );
}

/// The pre-existing TS17000 behaviour is unchanged when no name repeats.
#[test]
fn empty_initializer_alone_still_reports_ts17000() {
    let codes = codes_with_preamble(r#"const e = <div tau={} />;"#);
    assert_eq!(codes, vec![TS17000]);
}
