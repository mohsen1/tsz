//! TS7031 for a `var`/`let` array-destructuring declaration whose initializer
//! is a fresh array literal with a direct `null`/`undefined` widening leaf at
//! a leaf binding's position — the destructuring twin of `TS7005`/`TS7010`'s
//! `compound_nullish_widening_implicit_any_ts7005/7010_tests.rs`.
//!
//! Structural rule: `var [a, b] = [undefined, null];` widens each tuple slot
//! independently (unlike the array-to-array BCT path, which widens the whole
//! element union at once) when `strictNullChecks` is off — so slot 0 and slot
//! 1 both become `any`, and an unannotated, initializer-less leaf binding at
//! that position implicitly has that `any` type.
//!
//! Owner: the per-slot widen lives in `types/computation/array_literal.rs`'s
//! tuple-context element loop (shares `expr_is_direct_nullish_widening_leaf`,
//! `types/utilities/mutable_binding_nullish.rs`, with the whole-literal `any`
//! gate TS7005/TS7010 already use). The diagnostic lives in
//! `state/state_checking_members/implicit_any_checks.rs`'s
//! `emit_implicit_any_for_var_destructuring_nullish_array_initializer`.
//!
//! Every row below is pinned against a real `tsc` 6.0.2 oracle
//! (`/opt/node22/bin/tsc`), `--target es2015 --strict false --noImplicitAny true`.

use crate::context::CheckerOptions;
use crate::test_utils::check_with_options_code_messages;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

fn messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, opts())
}

/// The reported repro (`wideningTuples`'s theme, destructuring leg).
#[test]
fn both_nullish_keywords_report_ts7031_on_both_leaves() {
    let source = "var [a, b] = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![
            (
                7031,
                "Binding element 'a' implicitly has an 'any' type.".to_string()
            ),
            (
                7031,
                "Binding element 'b' implicitly has an 'any' type.".to_string()
            ),
        ],
    );
}

/// Renamed binders: the diagnostic text must track the actual identifiers.
#[test]
fn renamed_binders_report_ts7031_with_their_own_names() {
    let source = "var [first, second] = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![
            (
                7031,
                "Binding element 'first' implicitly has an 'any' type.".to_string()
            ),
            (
                7031,
                "Binding element 'second' implicitly has an 'any' type.".to_string()
            ),
        ],
    );
}

/// Mixed: only the nullish slot widens to `any`; the concrete slot keeps its
/// literal-widened type (`number`), so only `b` is implicitly `any`.
#[test]
fn mixed_concrete_and_nullish_reports_ts7031_only_for_nullish_leaf() {
    let source = "var [a, b] = [1, undefined];";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'b' implicitly has an 'any' type.".to_string()
        )],
    );
}

/// Alias/wrapper: nested array-binding pattern against a nested array-literal
/// source recurses per position.
#[test]
fn nested_binding_pattern_recurses_into_nested_array_literal() {
    let source = "var [[a], b] = [[undefined], null];";
    assert_eq!(
        messages(source),
        vec![
            (
                7031,
                "Binding element 'a' implicitly has an 'any' type.".to_string()
            ),
            (
                7031,
                "Binding element 'b' implicitly has an 'any' type.".to_string()
            ),
        ],
    );
}

/// Negative control, oracle-verified: a leaf with its own default initializer
/// takes its type from the default, not the (possibly nullish) literal slot —
/// no diagnostic for that leaf.
#[test]
fn leaf_with_own_default_reports_nothing_for_that_leaf() {
    let source = "var [a, b = 5] = [undefined, undefined];";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'a' implicitly has an 'any' type.".to_string()
        )],
    );
}

/// Negative control, oracle-verified: an element whose type is already `any`
/// through its own declaration — not through nullish widening — must not
/// report TS7031, mirroring the identical TS7005/TS7010 guard.
#[test]
fn already_any_element_reports_nothing_for_that_leaf() {
    let source = "\
declare var y: any;
var [a, b] = [y, undefined];
";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'b' implicitly has an 'any' type.".to_string()
        )],
    );
}

/// Negative control: an explicit type annotation on the pattern makes the
/// declared type authoritative — no implicit-any diagnostic applies at all.
#[test]
fn annotated_pattern_does_not_report_ts7031() {
    let source = "var [a, b]: [any, any] = [undefined, null];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// Negative control: with `strictNullChecks` on, tsc never widens
/// null/undefined regardless of provenance, so this path must not fire.
#[test]
fn strict_null_checks_on_does_not_report_ts7031() {
    let source = "var [a, b] = [undefined, null];";
    let strict_opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    assert_eq!(
        check_with_options_code_messages(source, strict_opts),
        Vec::<(u32, String)>::new()
    );
}

/// Negative control: `noImplicitAny` off suppresses every `TS7xxx`
/// implicit-any diagnostic, including this path.
#[test]
fn no_implicit_any_off_does_not_report_ts7031() {
    let source = "var [a, b] = [undefined, null];";
    let lenient_opts = CheckerOptions {
        strict: false,
        strict_null_checks: false,
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    assert_eq!(
        check_with_options_code_messages(source, lenient_opts),
        Vec::<(u32, String)>::new()
    );
}

/// Negative control: two concrete elements, no widening leaf anywhere — no
/// diagnostic.
#[test]
fn all_concrete_elements_report_nothing() {
    let source = "var [a, b] = [1, \"s\"];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}
