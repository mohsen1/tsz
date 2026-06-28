//! Regression coverage for type-level indexed access over a union with a
//! nullish member (`null` / `undefined` / `void`).
//!
//! `tsc` computes `(T)[K]` over a union by requiring *every* constituent to
//! resolve the key — `getPropertyTypeForIndexType` consults `getPropertyOfType`
//! (present only when all members have it) and `getApplicableIndexInfo`
//! (`getUnionIndexInfos`, present only when all members supply it). A nullish
//! constituent has no members and no index signatures, so it can never resolve a
//! key, and the whole access reports TS2339.
//!
//! tsz previously exposed a union's index signature whenever *any* member had
//! one (`get_index_signatures` collected from "any member"), so a union like
//! `null | { [k: string]: number }` wrongly looked string-indexed: the
//! type-level indexed-access checker accepted the key and dropped TS2339, and
//! the access silently evaluated to the member value type (#14804). The fix
//! requires every constituent to supply the index signature, voiding it when a
//! nullish (or otherwise index-less) member is present.
//!
//! Binder-name invariance: witnesses vary the alias / key names so the rule is
//! structural, not keyed off any spelling.

use crate::test_utils::{
    check_source_with_libs, diagnostic_codes, load_default_lib_files, strict_checker_options,
};

fn codes(source: &str) -> Vec<u32> {
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        strict_checker_options(),
        &load_default_lib_files(),
    ))
}

fn assert_has_2339(source: &str) {
    let found = codes(source);
    assert!(
        found.contains(&2339),
        "expected TS2339 (property does not exist) for source:\n{source}\ngot: {found:?}"
    );
    // The missing-key family is TS2339, never the generic TS2536
    // ("cannot be used to index type"), which `tsc` reserves for
    // generic/type-parameter object receivers.
    assert!(
        !found.contains(&2536),
        "must not emit TS2536 for a concrete nullish-union receiver:\n{source}\ngot: {found:?}"
    );
}

fn assert_clean(source: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics, got {found:?} for source:\n{source}"
    );
}

/// The reported witness: a string-index member plus a `null` member. The union
/// has no applicable string index (null supplies none), so every key is missing.
#[test]
fn null_with_string_index_member_reports_missing() {
    assert_has_2339(
        r#"
type T = null | { [k: string]: number };
type Bad = T["nope"];
"#,
    );
}

/// `undefined` constituent behaves identically to `null`.
#[test]
fn undefined_with_string_index_member_reports_missing() {
    assert_has_2339(
        r#"
type T = undefined | { [k: string]: number };
type Bad = T["nope"];
"#,
    );
}

/// `void` is also a nullish-like member with no apparent members.
#[test]
fn void_with_string_index_member_reports_missing() {
    assert_has_2339(
        r#"
type T = void | { [k: string]: number };
type Bad = T["nope"];
"#,
    );
}

/// Named-property member plus a nullish member: the named member resolves the
/// key, but `null` does not, so the union access is still TS2339.
#[test]
fn null_with_named_property_member_reports_missing() {
    assert_has_2339(
        r#"
type T = null | { a: number };
type Bad = T["a"];
"#,
    );
}

/// Number-index member plus a nullish member, indexed numerically.
#[test]
fn null_with_number_index_member_reports_missing() {
    assert_has_2339(
        r#"
type T = null | { [k: number]: string };
type Bad = T[0];
"#,
    );
}

/// Multiple index-signature members plus a nullish member: still missing,
/// because the nullish member breaks the all-members requirement.
#[test]
fn null_with_multiple_index_members_reports_missing() {
    assert_has_2339(
        r#"
type T = null | { [k: string]: number } | { [k: string]: boolean };
type Bad = T["x"];
"#,
    );
}

/// Binder-name invariance: the rule must not key off the alias or key spelling.
#[test]
fn nullish_union_missing_property_is_binder_name_invariant() {
    assert_has_2339(
        r#"
type Receiver = undefined | { [property: string]: boolean };
type Result = Receiver["anything"];
"#,
    );
}

/// Control — `NonNullable` strips the nullish member, leaving a genuinely
/// string-indexed object whose key space accepts every string key.
#[test]
fn non_nullable_strips_nullish_member_and_is_clean() {
    assert_clean(
        r#"
type T = NonNullable<null | { [k: string]: number }>;
type Ok = T["nope"];
"#,
    );
}

/// Control — when every constituent supplies the string index signature, the
/// union exposes it and the access is valid (the value type is the union of the
/// per-member value types).
#[test]
fn union_where_all_members_have_string_index_is_clean() {
    assert_clean(
        r#"
type T = { [k: string]: number } | { [k: string]: boolean };
type Ok = T["x"];
"#,
    );
}

/// Control — a key resolvable on every member (named on one, via the string
/// index on the other) is valid even though the members differ in shape.
#[test]
fn union_key_resolvable_on_all_members_is_clean() {
    assert_clean(
        r#"
type T = { a: number } | { [k: string]: number };
type Ok = T["a"];
"#,
    );
}

/// Control — a single string-indexed object (no nullish member) accepts any
/// string key, unchanged by the fix.
#[test]
fn plain_string_indexed_object_is_clean() {
    assert_clean(
        r#"
type T = { [k: string]: number };
type Ok = T["nope"];
"#,
    );
}
