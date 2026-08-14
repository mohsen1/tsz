//! Regression coverage: an `any`-typed computed destructuring key needs a
//! source that permits a dynamic index, exactly like the equivalent element
//! access `obj[k]`.
//!
//! `{ [k]: v } = obj` desugars to `v = obj[k]`, so an `any` key is a valid
//! index only when `obj` permits a dynamic index: `obj` is itself `any`
//! (already exempted upstream before this check runs), or `obj` structurally
//! carries a string/number index signature (directly, or through a generic
//! type parameter's constraint). A concrete source with no index signature
//! (`{}`, `{ a: T }`) still reports TS2538, matching tsc.
//!
//! Oracle-verified against `typescript@6.0.2`: a bare/`any`/`unknown`-
//! constrained type parameter source reports TS2538 even though the
//! constraint is (or resolves to) `any` — this destructuring-specific check
//! does not defer to the type parameter the way plain element access does.
//! Binder names are varied so the rule stays structural.

use crate::CheckerOptions;
use crate::test_utils::{
    check_source_with_libs, diagnostic_codes, load_default_lib_files, non_strict_checker_options,
};

fn codes_with(source: &str, options: CheckerOptions) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        options,
        &load_default_lib_files(),
    ))
}

fn codes(source: &str) -> Vec<u32> {
    codes_with(source, non_strict_checker_options())
}

fn assert_no_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        !found.contains(&2538),
        "expected no TS2538, got {found:?} for source:\n{source}"
    );
}

fn assert_has_ts2538(source: &str) {
    let found = codes(source);
    assert!(
        found.contains(&2538),
        "expected TS2538, got {found:?} for source:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// Positive: concrete sources with no index signature reject the `any` key.
// ---------------------------------------------------------------------------

#[test]
fn any_key_over_empty_object_literal_source_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
declare const emptyHost: {};
const { [untypedKey]: pulled } = emptyHost;
"#,
    );
}

#[test]
fn any_key_over_concrete_shape_source_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
declare const shapedHost: { alpha: number };
const { [untypedKey]: pulled } = shapedHost;
"#,
    );
}

#[test]
fn any_key_over_unknown_source_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
declare const opaqueHost: unknown;
const { [untypedKey]: pulled } = opaqueHost;
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative controls: sources that permit a dynamic index stay clean.
// ---------------------------------------------------------------------------

#[test]
fn any_key_over_any_source_is_clean() {
    assert_no_ts2538(
        r#"
declare const untypedKey: any;
declare const anyHost: any;
const { [untypedKey]: pulled } = anyHost;
"#,
    );
}

#[test]
fn any_key_over_string_index_signature_source_is_clean() {
    assert_no_ts2538(
        r#"
declare const untypedKey: any;
declare const dictHost: { [entry: string]: number };
const { [untypedKey]: pulled } = dictHost;
"#,
    );
}

#[test]
fn any_key_over_record_string_index_source_is_clean() {
    assert_no_ts2538(
        r#"
declare const untypedKey: any;
declare const dictHost: Record<string, number>;
const { [untypedKey]: pulled } = dictHost;
"#,
    );
}

#[test]
fn any_key_over_number_index_signature_source_is_clean() {
    assert_no_ts2538(
        r#"
declare const untypedKey: any;
declare const listHost: { [slot: number]: string };
const { [untypedKey]: pulled } = listHost;
"#,
    );
}

// ---------------------------------------------------------------------------
// Generic sources: the exemption follows the resolved constraint, not the
// bare fact of being a type parameter (oracle-verified — see module doc).
// ---------------------------------------------------------------------------

#[test]
fn any_key_over_unconstrained_type_parameter_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
function readDynamic<Guest>(host: Guest) {
    const { [untypedKey]: pulled } = host;
}
"#,
    );
}

#[test]
fn any_key_over_any_constrained_type_parameter_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
function readDynamic<Guest extends any>(host: Guest) {
    const { [untypedKey]: pulled } = host;
}
"#,
    );
}

#[test]
fn any_key_over_unknown_constrained_type_parameter_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const untypedKey: any;
function readDynamic<Guest extends unknown>(host: Guest) {
    const { [untypedKey]: pulled } = host;
}
"#,
    );
}

#[test]
fn any_key_over_index_signature_constrained_type_parameter_is_clean() {
    assert_no_ts2538(
        r#"
declare const untypedKey: any;
function readDynamic<Guest extends { [entry: string]: number }>(host: Guest) {
    const { [untypedKey]: pulled } = host;
}
"#,
    );
}

// ---------------------------------------------------------------------------
// Non-`any` invalid keys (boolean, etc.) are untouched by this change.
// ---------------------------------------------------------------------------

#[test]
fn boolean_key_over_empty_object_still_reports_ts2538() {
    assert_has_ts2538(
        r#"
declare const flagKey: boolean;
declare const emptyHost: {};
const { [flagKey]: pulled } = emptyHost;
"#,
    );
}
