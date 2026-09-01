//! TS2693 for a destructured parameter rename that shadows a primitive-keyword
//! name (`string`, `number`, ...) referenced by `typeof` inside the same
//! signature.
//!
//! Structural rule: a parameter binding pattern declares a real value binding
//! that stays in scope for every `typeof` type query of its own signature,
//! same as any other name (#16241/#16249). `error_cannot_find_name_at`
//! (`error_reporter/name_resolution.rs`) special-cases primitive-keyword names
//! with an early `TS2693` return *before* the general
//! `signature_parameter_declares_binding` guard runs later in the same
//! function, so a rename that happens to reuse a primitive keyword
//! (`({ a: string }) => typeof string`) fell through to that early branch and
//! reported `TS2693` even though `tsc` resolves the binding and reports
//! nothing.
//!
//! A second, adjacent gap: `signature_parameter_declares_binding` only checked
//! the *nearest* enclosing signature. A `typeof` query nested inside a
//! bodyless `FunctionType` that is itself part of an outer signature's return
//! type (`(a: T) => (b: U) => typeof a`) still names the outer binding under
//! `tsc`, since the inner `FunctionType` has no body and so no scope of its
//! own — fixed by climbing through pure-type signatures to the next enclosing
//! one.
//!
//! Every expectation here was checked against the pinned `typescript@7.0.2`
//! oracle (`--noEmit --strict false --target es2015 --pretty false`), not
//! against tsz's own prior output.

use crate::test_utils::{check_source, non_strict_checker_options};

/// All fixtures in this file mirror the pinned `typescript@7.0.2` oracle run
/// under `// @strict: false`, matching how the issue's own repro was checked
/// (`--strict false --target es2015`).
fn codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", non_strict_checker_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn assert_clean(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        got.is_empty(),
        "{label}: expected no diagnostics, got codes {got:?}"
    );
}

fn assert_codes(source: &str, expected: &[u32], label: &str) {
    let got = codes(source);
    assert_eq!(got, expected, "{label}: expected {expected:?}, got {got:?}");
}

// ---------------------------------------------------------------------------
// Positive: the rename resolves and the primitive-keyword TS2693 must not fire.
// ---------------------------------------------------------------------------

/// The issue's own witness: a single renamed property, referenced via `typeof`
/// in the signature's return type.
#[test]
fn renamed_to_string_referenced_by_typeof_stays_clean() {
    assert_clean(
        "type F = ({ a: string }) => typeof string;",
        "single rename to `string`, referenced",
    );
}

/// Two renames in the same parameter pattern: the unused one (`string`) still
/// reports `TS2842`, the referenced one (`number`) reports nothing — `tsc`
/// never emits `TS2693` on either.
#[test]
fn two_renames_one_used_one_unused_reports_only_ts2842() {
    assert_codes(
        "type F = ({ a: string, b: number }) => typeof number;",
        &[2842],
        "two renames, one referenced by typeof",
    );
}

/// Same rule on an interface method signature, not just a `FunctionType`
/// alias body.
#[test]
fn interface_method_signature_renamed_to_string_stays_clean() {
    assert_clean(
        "interface I { m({ a: string }): typeof string; }",
        "interface method signature rename",
    );
}

/// `number` is not special-cased by name; the same shadowing must hold for
/// every primitive keyword the early-return branch covers.
#[test]
fn renamed_to_number_referenced_by_typeof_stays_clean() {
    assert_clean(
        "type F = ({ a: number }) => typeof number;",
        "single rename to `number`, referenced",
    );
}

// ---------------------------------------------------------------------------
// Positive: nested pure-type signatures climb to the outer binding.
// ---------------------------------------------------------------------------

/// A `typeof` reference inside a nested, bodyless `FunctionType` (the return
/// type's own return type) still names the outer signature's renamed
/// binding — the inner `FunctionType` has no parameters and no body, so it is
/// not a separate scope.
#[test]
fn typeof_in_nested_function_type_return_resolves_outer_binding() {
    assert_clean(
        "type F = ({ a: string }) => (x: number) => typeof string;",
        "typeof nested inside the outer signature's return type",
    );
}

/// Two levels of nesting: the climb must not stop after a single hop.
#[test]
fn typeof_in_doubly_nested_function_type_return_resolves_outer_binding() {
    assert_clean(
        "type F = ({ a: string }) => (x: number) => (y: boolean) => typeof string;",
        "typeof nested two FunctionTypes deep",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: what must stay exactly as before.
// ---------------------------------------------------------------------------

/// No renaming at all: `typeof string` names the global primitive and `tsc`
/// reports `TS2693`. This is the control that fails if the fix over-widens to
/// "never report TS2693 near a signature".
#[test]
fn plain_parameter_no_rename_typeof_primitive_still_reports_ts2693() {
    assert_codes(
        "type F = (x: string) => typeof string;",
        &[2693],
        "no destructuring, bare typeof on a primitive keyword",
    );
}

/// The rename exists but is never referenced by a `typeof` query anywhere:
/// `TS2842` (unused renaming) fires alone, same as before this fix — the
/// guard must not suppress the unused-rename diagnostic.
#[test]
fn renamed_to_string_never_referenced_reports_only_ts2842() {
    assert_codes(
        "type F = ({ a: string }) => number;",
        &[2842],
        "rename with no typeof reference anywhere",
    );
}

/// The rename shadows one primitive keyword but the `typeof` query names a
/// *different* one: the queried name is not declared by any parameter, so the
/// global primitive is what actually gets referenced and `TS2693` still
/// fires — alongside `TS2842`, since the `number` rename itself is unused.
#[test]
fn typeof_names_a_different_primitive_than_the_rename_still_reports_ts2693() {
    assert_codes(
        "type F = ({ a: number }) => typeof string;",
        &[2842, 2693],
        "rename to `number`, typeof queries `string`",
    );
}
