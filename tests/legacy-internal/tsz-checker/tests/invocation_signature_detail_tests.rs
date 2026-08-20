//! Regression tests for the `tsc` `invocationErrorDetails` chain link beneath
//! a non-callable / non-constructable expression error.
//!
//! Structural rule (verified against `tsc` 6.0.2): when a `new` or call target
//! has no construct / call signatures, `tsc`'s `resolveNewExpression` /
//! `resolveCallExpression` append a chained note beneath the headline —
//! `Type 'X' has no construct signatures.` (TS2761) under
//! `This expression is not constructable.` (TS2351), and
//! `Type 'X' has no call signatures.` (TS2757) under
//! `This expression is not callable.` (TS2349). The source is shown as its
//! apparent type (`number` -> `Number`, `1` -> `Number`, `object` -> `{}`),
//! matching `typeToString(getApparentType(type))`.
//!
//! tsz emitted only the headline and dropped the note at every single-type
//! site. The fix builds the note through the shared
//! `invocation_signature_detail` reporter helper for both surfaces, guarding
//! out union sources (whose distinct `Not all constituents …` / `No
//! constituent …` shapes are a separate, ordering-coupled concern). Display
//! only — the diagnostic code and count are unchanged.

use crate::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// Run the strict pipeline with the default libs loaded, so a bare primitive
/// source resolves to its boxed wrapper interface (`Number`, `String`, ...) for
/// display — the behavior a real compilation observes.
fn check(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions::default(),
        &load_default_lib_files(),
    )
}

/// Collect a single diagnostic's full text (headline plus every related note,
/// newline-joined) for `code`, asserting exactly one such diagnostic exists.
fn detail(source: &str, code: u32) -> String {
    let diags = check(source);
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

// ── TS2351: not constructable ───────────────────────────────────────────────

#[test]
fn new_number_value_notes_boxed_wrapper() {
    let text = detail("const widget = 1;\nnew widget();\n", 2351);
    assert!(
        text.contains("This expression is not constructable."),
        "headline missing: {text:?}"
    );
    assert!(
        text.contains("Type 'Number' has no construct signatures."),
        "apparent-typed note missing (number -> Number): {text:?}"
    );
}

#[test]
fn new_string_literal_value_notes_boxed_wrapper() {
    // A fresh string-literal source is widened to its boxed wrapper for display.
    let text = detail("const banner = \"hello\";\nnew banner();\n", 2351);
    assert!(
        text.contains("Type 'String' has no construct signatures."),
        "string literal should display as String: {text:?}"
    );
}

#[test]
fn new_object_value_notes_structural_shape() {
    let text = detail("const bag = {};\nnew bag();\n", 2351);
    assert!(
        text.contains("Type '{}' has no construct signatures."),
        "object should display structurally: {text:?}"
    );
}

#[test]
fn new_interface_value_notes_interface_name() {
    // Binder names vary across cases so the note follows the apparent type, not
    // any fixed identifier (anti-hardcoding).
    let text = detail(
        "interface Gadget {}\ndeclare const thing: Gadget;\nnew thing();\n",
        2351,
    );
    assert!(
        text.contains("Type 'Gadget' has no construct signatures."),
        "interface name should display: {text:?}"
    );
}

// ── TS2349: not callable ────────────────────────────────────────────────────

#[test]
fn call_number_value_notes_boxed_wrapper() {
    let text = detail("const tally = 42;\ntally();\n", 2349);
    assert!(
        text.contains("This expression is not callable."),
        "headline missing: {text:?}"
    );
    assert!(
        text.contains("Type 'Number' has no call signatures."),
        "apparent-typed note missing (number -> Number): {text:?}"
    );
}

#[test]
fn call_object_value_notes_structural_shape() {
    let text = detail("const crate = {};\ncrate();\n", 2349);
    assert!(
        text.contains("Type '{}' has no call signatures."),
        "object should display structurally: {text:?}"
    );
}

#[test]
fn call_interface_value_notes_interface_name() {
    let text = detail(
        "interface Sprocket {}\ndeclare const part: Sprocket;\npart();\n",
        2349,
    );
    assert!(
        text.contains("Type 'Sprocket' has no call signatures."),
        "interface name should display: {text:?}"
    );
}

// ── Controls ────────────────────────────────────────────────────────────────

#[test]
fn valid_construct_emits_no_diagnostic() {
    // A real construct signature must not produce a spurious note.
    let diags = check("class Engine {}\nnew Engine();\n");
    assert!(
        !diags.iter().any(|d| d.code == 2351),
        "valid `new` must not emit TS2351: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn valid_call_emits_no_diagnostic() {
    let diags = check("function ping() {}\nping();\n");
    assert!(
        !diags.iter().any(|d| d.code == 2349),
        "valid call must not emit TS2349: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
