//! Regression tests for `noUncheckedIndexedAccess` write semantics.
//!
//! Under NUIA, the READ type of an index-signature lookup widens to
//! `T | undefined`, but the WRITE type must remain `T`. Without this
//! distinction, writing `undefined` to a non-undefined index-signature
//! value type silently succeeds, and assignments otherwise rejected by
//! TS2322 leak through.
//!
//! Source: `conformance/pedantic/noUncheckedIndexedAccess.ts`
//! ("Writes don't allow 'undefined'; all should be errors").

use crate::test_utils::diagnostic_codes;
use tsz_common::options::checker::CheckerOptions;

fn diags_for_strict_nuia(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_unchecked_indexed_access: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

/// Read-side: NUIA widens an index-signature lookup to `T | undefined`,
/// so assigning the result to a non-undefined variable emits TS2322.
/// This locks in the read-side behavior the write-fix must not regress.
#[test]
fn nuia_read_widens_to_undefined_and_emits_ts2322_when_assigned_to_strict_type() {
    let source = r#"
declare const strMap: { [s: string]: boolean };
const x: boolean = strMap["k"];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2322),
        "NUIA read must emit TS2322 for boolean|undefined → boolean. Got: {codes:?}",
    );
}

/// Anti-hardcoding cover: same shape, renamed identifier, different
/// value type. Confirms the read-widening rule isn't keyed off
/// `strMap`/`boolean`.
#[test]
fn nuia_read_widens_to_undefined_renamed_identifier_and_value_type() {
    let source = r#"
declare const lookupTable: { [k: string]: number };
const v: number = lookupTable.foo;
"#;
    let diags = diags_for_strict_nuia(source);
    let codes = diagnostic_codes(&diags);
    assert!(
        codes.contains(&2322),
        "Renamed: NUIA read must emit TS2322 for number|undefined → number. Got: {codes:?}",
    );
}

/// Negative control: `T | undefined` slot accepts `undefined` from NUIA.
/// Ensures NUIA still produces a usable `T | undefined` for the read path.
#[test]
fn nuia_read_to_t_or_undefined_slot_does_not_emit_ts2322() {
    let source = r#"
declare const strMap: { [s: string]: boolean };
const x: boolean | undefined = strMap["k"];
"#;
    let diags = diags_for_strict_nuia(source);
    let codes = diagnostic_codes(&diags);
    assert!(
        !codes.contains(&2322),
        "boolean|undefined slot must accept NUIA-widened read. Got: {codes:?}",
    );
}

#[test]
fn nuia_generic_index_signature_return_message_keeps_undefined() {
    let source = r#"
function generic1<T extends { [s: string]: boolean }>(arg: T): boolean {
    return arg["blah"];
}
"#;
    let diags = diags_for_strict_nuia(source);
    let ts2322 = diags
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for NUIA-widened generic index signature return");
    assert!(
        ts2322
            .message_text
            .contains("Type 'boolean | undefined' is not assignable to type 'boolean'."),
        "NUIA return diagnostic should preserve the read-side undefined, got: {ts2322:?}",
    );
}
