//! TS7031 for a destructuring binding element whose per-slot type is exactly
//! `null`/`undefined` and widens to `any` under non-strict null checks — the
//! destructuring-binding twin of `TS7005`'s
//! `compound_nullish_widening_implicit_any_ts7005_tests.rs` (mutable-binding
//! initializer) and `TS7010`'s
//! `compound_nullish_widening_implicit_any_ts7010_tests.rs` (function return).
//!
//! Structural rule: a destructuring declaration's initializer array literal is
//! evaluated in TUPLE context (`build_contextual_type_from_pattern_with_request`,
//! `state/variable_checking/core.rs`), which takes `array_literal.rs`'s
//! `tuple_context.is_some()` branch — a different seam from the plain-array
//! BCT path TS7005/TS7010 own, and one that deliberately never widens (so
//! `const [first = 0] = [10, 20]` can keep the positional literal `0 | 10`).
//! Each binding element's own type is instead widened later, per-slot, at
//! `state/variable_checking/destructuring.rs`'s
//! `assign_binding_pattern_symbol_types_with_request_reporting` via
//! `flow_boundary::widen_null_undefined_to_any` — that call already existed
//! (it drives the correct *type* of a widened binding), but nothing
//! downstream compared the pre/post-widen type to report the implicit-any.
//!
//! Because this is a per-slot check (not a resulting-type-only check like
//! TS7005/TS7010's `array_element_type(..) == ANY`), it does not need their
//! `array_literal_has_direct_nullish_leaf` provenance walk: an already-`any`
//! source element (`declare var y: any; var [n] = [y];`) has element type
//! `ANY` *before* the widen call too, so `element_type != final_type` is
//! false and nothing fires — no BCT collapse exists at this granularity to
//! hide the provenance.
//!
//! Owner: `state/variable_checking/destructuring.rs`'s
//! `assign_binding_pattern_symbol_types_with_request_reporting`, gated by
//! `state/variable_checking/core.rs`'s `report_widened_binding_any` (mirrors
//! the existing no-initializer TS7031 gate's exclusions: catch variables and
//! for-in/for-of loop variables get their type from a different source, not
//! literal-initializer widening, and an explicit type annotation suppresses
//! it entirely since the annotated type is the declared type, not inferred).
//!
//! Deliberately out of scope for this PR (would need separate oracle-verified
//! rules, not just this widen-comparison): function-parameter destructuring
//! defaults (`function f([x, y] = [undefined, null]) {}` — oracle DOES report
//! TS7031 there too, but through a distinct call path,
//! `types/utilities/core.rs`'s parameter-binding-pattern assignment) and a
//! rest element whose sliced tuple is all-`any` (`var [k, ...rest] =
//! [undefined, null, undefined]` — oracle reports TS7031 on `rest` itself
//! with the whole tuple type `'[any, any]'` displayed, not a bare `any`).
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

/// The reported repro's destructuring-binding leg: both positions are bare
/// nullish keywords.
#[test]
fn array_destructuring_of_nullish_keywords_reports_ts7031_for_each() {
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
        ]
    );
}

/// `const` reports the same as `var`/`let` — a destructuring binding's own
/// element type has no "evolving" mechanism to defer through, matching the
/// TS7005/TS7010 siblings' const/let/var-independence.
#[test]
fn const_array_destructuring_reports_ts7031() {
    let source = "const [c, d] = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![
            (
                7031,
                "Binding element 'c' implicitly has an 'any' type.".to_string()
            ),
            (
                7031,
                "Binding element 'd' implicitly has an 'any' type.".to_string()
            ),
        ]
    );
}

/// Object destructuring goes through the same per-slot widen-and-report seam.
#[test]
fn object_destructuring_of_nullish_properties_reports_ts7031() {
    let source = "var {g, h} = {g: undefined, h: null};";
    assert_eq!(
        messages(source),
        vec![
            (
                7031,
                "Binding element 'g' implicitly has an 'any' type.".to_string()
            ),
            (
                7031,
                "Binding element 'h' implicitly has an 'any' type.".to_string()
            ),
        ]
    );
}

/// A nested pattern's leaf reports too — `report_widened_any` threads
/// unchanged through the recursive nested-pattern call.
#[test]
fn nested_object_destructuring_leaf_reports_ts7031() {
    let source = "var {p: {q}} = {p: {q: undefined}};";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'q' implicitly has an 'any' type.".to_string()
        )]
    );
}

/// A binding element covered by its own default value never widens to `any`
/// (the default's concrete type wins), so only the uncovered sibling reports —
/// matching the existing default-value TS7031 tests just above this file's
/// sibling coverage for the no-initializer case.
#[test]
fn default_covered_element_does_not_report_but_uncovered_sibling_does() {
    let source = "var [i = 1, j] = [undefined, null];";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'j' implicitly has an 'any' type.".to_string()
        )]
    );
}

/// Negative control (regression, oracle-verified): an element whose type is
/// already `any` through its own declaration — not through nullish widening —
/// must not report TS7031. The pre-widen `element_type` is already `ANY`
/// here, so `element_type != final_type` is false.
#[test]
fn already_any_element_reports_nothing() {
    let source = "\
declare var y: any;
var [n] = [y];
";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// A mixed literal reduces the non-nullish sibling to its own concrete type
/// (unaffected) while the nullish position still reports — this is a
/// per-element check, not a BCT-based one, so it does not share TS7005's
/// mixed-literal exclusion (`[1, undefined]` reports nothing for the *whole
/// array* under TS7005, but here `p` alone still implicitly widens to `any`).
#[test]
fn mixed_literal_reports_only_the_nullish_position() {
    let source = "var [o, p] = [1, undefined];";
    assert_eq!(
        messages(source),
        vec![(
            7031,
            "Binding element 'p' implicitly has an 'any' type.".to_string()
        )]
    );
}

/// An explicit type annotation on the pattern suppresses TS7031 entirely —
/// the annotated type is declared, not inferred, so no widening step runs.
#[test]
fn annotated_pattern_reports_nothing() {
    let source = "var [x, y]: [undefined, null] = [undefined, undefined];";
    assert_eq!(messages(source), Vec::<(u32, String)>::new());
}

/// `strictNullChecks: true` never widens `null`/`undefined` to `any` at all,
/// so no TS7031 fires regardless of `noImplicitAny` — mirrors the
/// TS7005/TS7010 siblings' `!strict_null_checks()` gate.
#[test]
fn strict_null_checks_reports_nothing() {
    let source = "var [a, b] = [undefined, null];";
    let strict_opts = CheckerOptions {
        strict: false,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    assert_eq!(
        check_with_options_code_messages(source, strict_opts),
        Vec::<(u32, String)>::new()
    );
}
