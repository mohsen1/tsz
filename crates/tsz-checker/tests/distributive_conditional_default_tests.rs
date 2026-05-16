//! Tests for distributive conditional types with type parameter defaults.
//!
//! Issue #6277: `IsUnion<T, U = T>` pattern — when a distributive conditional
//! type has a second parameter that defaults to the first, distribution must
//! substitute ONLY the distributive check parameter in each branch, leaving
//! the default-bound parameter holding the original (full-union) value.
//!
//! The structural rule:
//!   When `T extends U ? [U] extends [T] ? false : true : never` is instantiated
//!   with a union `T`, each distributed branch must preserve `U` as the full
//!   union so `[full_union] extends [member]` is false and the branch returns
//!   `true`, making the whole expression `true`.

use tsz_checker::test_utils::{check_source_strict, diagnostic_count, diagnostics_without_codes};

fn ts2322_count(source: &str) -> usize {
    diagnostic_count(&check_source_strict(source), 2322)
}

/// Returns true when the source has no diagnostics other than TS2318
/// (missing global types, expected in the no-stdlib unit test harness).
fn has_no_errors(source: &str) -> bool {
    diagnostics_without_codes(&check_source_strict(source), &[2318]).is_empty()
}

// ── IsUnion pattern (the canonical issue #6277 repro) ─────────────────────

#[test]
fn is_union_two_member_union_evaluates_to_true() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<"a" | "b">;
const x: R = true;
"#
        ),
        "distribution should produce `true` for a two-member union; got errors"
    );
}

#[test]
fn is_union_non_union_evaluates_to_false() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<string>;
const x: R = false;
"#
        ),
        "non-union type should evaluate to `false`; got errors"
    );
}

/// Renamed type parameters must produce identical results — no hardcoded name assumption.
#[test]
fn is_union_renamed_params_two_member_union() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<X, Y = X> = X extends Y ? [Y] extends [X] ? false : true : never;
type R = IsUnion<"a" | "b">;
const x: R = true;
"#
        ),
        "renamed params X/Y should evaluate identically to T/U"
    );
}

#[test]
fn is_union_three_member_union_evaluates_to_true() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<"a" | "b" | "c">;
const x: R = true;
"#
        ),
        "three-member union should evaluate to `true`; got errors"
    );
}

/// A single string literal is not a union — it must not be treated as one even
/// though literals and primitive non-union types (e.g. `string`) may follow
/// different evaluation paths.
#[test]
fn is_union_single_literal_evaluates_to_false() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<"a">;
const x: R = false;
"#
        ),
        "single literal `\"a\"` is not a union — should evaluate to `false`"
    );
}

/// Distributive short-circuit: `never extends U` is skipped, returning `never`.
#[test]
fn is_union_never_evaluates_to_never() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<never>;
type Test = [R] extends [never] ? true : false;
const x: Test = true;
"#
        ),
        "IsUnion<never> should be `never`, not `true` or `false`"
    );
}

/// `boolean` is `true | false` internally, so `IsUnion` must treat it as a union.
#[test]
fn is_union_boolean_is_union() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R = IsUnion<boolean>;
const x: R = true;
"#
        ),
        "boolean is true | false internally — IsUnion<boolean> should be `true`"
    );
}

/// Real TS2322 is still emitted when assigning the wrong branch result.
#[test]
fn is_union_wrong_assignment_emits_ts2322() {
    assert_eq!(
        ts2322_count(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
const bad: IsUnion<"a" | "b"> = false;
"#
        ),
        1,
        "IsUnion<union> evaluates to `true`; assigning `false` must emit TS2322"
    );
}

/// Caller can explicitly supply the second parameter; the override must be respected.
/// `IsUnion<"a"|"b", "a"|"b">` = true (U is the same union, distribution detects it).
/// `IsUnion<string, string>` = false (U = T = string, non-union single type).
#[test]
fn is_union_explicit_second_param_respected() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type R1 = IsUnion<"a" | "b", "a" | "b">;
const r1: R1 = true;
type R2 = IsUnion<string, string>;
const r2: R2 = false;
"#
        ),
        "explicit U should be respected: union+U=same-union → true, string+U=string → false"
    );
}

// ── Distributive default parameter in true branch ─────────────────────────

/// U holds the full union in the true branch; if incorrectly substituted by
/// the distributed member, only `string` would be accepted (not `number`).
#[test]
fn distributive_default_true_branch_returns_full_union() {
    assert!(
        has_no_errors(
            r#"
type WrapDefault<T, U = T> = T extends string ? U : never;
type R = WrapDefault<string | number>;
const x: R = "hello";
const y: R = 42;
"#
        ),
        "U should retain string|number; if narrowed to string, assigning 42 would fail"
    );
    assert_eq!(
        ts2322_count(
            r#"
type WrapDefault<T, U = T> = T extends string ? U : never;
const bad: WrapDefault<string | number> = true;
"#
        ),
        1,
        "assigning boolean to WrapDefault<string|number> must emit TS2322"
    );
}

/// Renamed params `A`/`B` — distribution must not depend on parameter names.
#[test]
fn distributive_default_true_branch_renamed_params() {
    assert!(
        has_no_errors(
            r#"
type WrapDefault<A, B = A> = A extends string ? B : never;
type R = WrapDefault<string | number>;
const x: R = "hello";
const y: R = 42;
"#
        ),
        "renamed A/B must preserve B as full union, same as T/U"
    );
}

// ── `IsUnion` in composite types ─────────────────────────────────────────

#[test]
fn is_union_nested_in_conditional() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type Label<T> = IsUnion<T> extends true ? "union" : "not-union";
type L1 = Label<"a" | "b">;
type L2 = Label<string>;
const l1: L1 = "union";
const l2: L2 = "not-union";
"#
        ),
        "IsUnion nested inside another conditional must still resolve"
    );
}

#[test]
fn is_union_in_generic_object_type() {
    assert!(
        has_no_errors(
            r#"
type IsUnion<T, U = T> = T extends U ? [U] extends [T] ? false : true : never;
type Box<T> = { isUnion: IsUnion<T> };
type B1 = Box<"a" | "b">;
type B2 = Box<string>;
const b1: B1 = { isUnion: true };
const b2: B2 = { isUnion: false };
"#
        ),
        "IsUnion as a property type in a generic object must evaluate correctly"
    );
}

// ── Permutation pattern (recursive distributive + K = T default) ──────────
//
// Uses inline MyExclude to avoid stdlib dependency in the no-lib test harness.

#[test]
fn permutation_type_never_produces_empty_tuple() {
    assert!(
        has_no_errors(
            r#"
type MyExclude<T, U> = T extends U ? never : T;
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
  ? [K, ...Permutation<MyExclude<T, K>>]
  : never;
type P = Permutation<never>;
const p: P = [];
"#
        ),
        "Permutation<never> should terminate as []"
    );
}

#[test]
fn permutation_type_single_member() {
    assert!(
        has_no_errors(
            r#"
type MyExclude<T, U> = T extends U ? never : T;
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
  ? [K, ...Permutation<MyExclude<T, K>>]
  : never;
type P = Permutation<"a">;
const p: P = ["a"];
"#
        ),
        "Permutation<\"a\"> should produce [\"a\"]"
    );
}

#[test]
fn permutation_type_two_members_no_error() {
    assert!(
        has_no_errors(
            r#"
type MyExclude<T, U> = T extends U ? never : T;
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
  ? [K, ...Permutation<MyExclude<T, K>>]
  : never;
type P = Permutation<"a" | "b">;
const p1: P = ["a", "b"];
const p2: P = ["b", "a"];
"#
        ),
        "Permutation<\"a\"|\"b\"> should accept both orderings"
    );
}

/// Renamed params (`X`/`Y`) must work identically — no hardcoded name assumption.
#[test]
fn permutation_type_renamed_params() {
    assert!(
        has_no_errors(
            r#"
type MyExclude<A, B> = A extends B ? never : A;
type Permute<X, Y = X> = [X] extends [never]
  ? []
  : Y extends Y
  ? [Y, ...Permute<MyExclude<X, Y>>]
  : never;
type P = Permute<"A" | "B">;
const p1: P = ["A", "B"];
const p2: P = ["B", "A"];
"#
        ),
        "renamed X/Y must work identically to T/K"
    );
}
