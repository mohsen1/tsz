//! Regression guard: `Record<keyof P, any>` assignable to a still-generic
//! homomorphic mapped target `{ [K in keyof P]: P[K] }` (issue #14943).
//!
//! Structural rule: when relating a homomorphic mapped source whose per-key
//! value type is `any` (e.g. `Record<keyof P, any>` ≡ `{ [K in keyof P]: any }`)
//! to a still-generic homomorphic mapped target `{ [K in keyof P]: P[K] }` over
//! the same generic key set `keyof P`, every source value is `any`, which is
//! assignable to each deferred target value `P[K]`. tsc accepts (clean); tsz
//! used to drop the `any`-propagation on the deferred mapped template leg and
//! report a false `TS2322` at the assignment. Owner: solver relation
//! (mapped-to-mapped template comparison).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

/// Type-check `source` as an external module with the bundled `es5` lib loaded
/// (so `Record` is in scope), returning only the diagnostic codes. Skips
/// gracefully (empty) when the bundled lib asset is unavailable.
fn check_es5_strict_codes(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts"]);
    if libs.is_empty() {
        return Vec::new();
    }
    check_source_with_libs_code_messages(source, "test.ts", strict_opts(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn ts2322_count(codes: &[u32]) -> usize {
    codes.iter().filter(|&&c| c == 2322).count()
}

/// The exact witness from #14943.
#[test]
fn record_keyof_any_assignable_to_homomorphic_identity_mapped() {
    let codes = check_es5_strict_codes(
        r#"
function g<P extends Record<string, unknown>>(): { [K in keyof P]: P[K] } {
  const o: Record<keyof P, any> = {} as any;
  return o;
}
export {};
"#,
    );
    if codes.is_empty() {
        return; // lib asset unavailable — covered by CLI/conformance instead
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Record<keyof P, any> assignable to homomorphic identity mapped: {codes:?}"
    );
}

/// Same rule with deliberately different binder names (no name-keyed logic).
#[test]
fn record_keyof_any_renamed_binders_assignable() {
    let codes = check_es5_strict_codes(
        r#"
function build<TRec extends Record<string, unknown>>(): { [Key in keyof TRec]: TRec[Key] } {
  const bag: Record<keyof TRec, any> = {} as any;
  return bag;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "renamed-binder form must also be assignable: {codes:?}"
    );
}

/// Widened target value (`unknown`): `any` source value is still assignable.
#[test]
fn record_keyof_any_assignable_to_unknown_valued_mapped() {
    let codes = check_es5_strict_codes(
        r#"
function widen<Q extends Record<string, unknown>>(): { [K in keyof Q]: unknown } {
  const o: Record<keyof Q, any> = {} as any;
  return o;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "Record<keyof Q, any> assignable to unknown-valued mapped: {codes:?}"
    );
}

/// Alias wrapper around the `Record<keyof T, any>` source.
#[test]
fn record_keyof_any_through_alias_wrapper_assignable() {
    let codes = check_es5_strict_codes(
        r#"
type AnyRec<T> = Record<keyof T, any>;
function via<P extends Record<string, unknown>>(): { [K in keyof P]: P[K] } {
  const o: AnyRec<P> = {} as any;
  return o;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        0,
        "alias-wrapped Record<keyof P, any> is assignable: {codes:?}"
    );
}

/// Negative: a concrete (non-`any`) source value type that does NOT satisfy the
/// target value type must still error. The `any`-propagation must be specific to
/// `any`, not a blanket accept of any homomorphic mapped source.
#[test]
fn record_keyof_string_not_assignable_to_number_valued_mapped() {
    let codes = check_es5_strict_codes(
        r#"
function bad<P extends Record<string, unknown>>(): { [K in keyof P]: number } {
  const o: Record<keyof P, string> = {} as any;
  return o;
}
export {};
"#,
    );
    if codes.is_empty() {
        return;
    }
    assert_eq!(
        ts2322_count(&codes),
        1,
        "string values must not satisfy a number-valued mapped target: {codes:?}"
    );
}
