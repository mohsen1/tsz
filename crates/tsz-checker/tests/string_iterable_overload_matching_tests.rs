//! Tests for the call-time "string is iterable" assignability shortcut.
//!
//! See issues #6624 and #6500.
//!
//! Structural rule: at a call site, when an argument is `string`-like
//! (the `string` primitive, a string literal, or a template-literal type)
//! and the corresponding parameter is iterable yielding a type compatible
//! with `string`, the call is accepted as if `String` structurally
//! satisfied the parameter. This shortcut MUST be tightened to the same
//! "purely iterable" guard the boxed-primitive subtype rule uses
//! (`target_has_non_iterable_property_shape`):
//! - reject when the target carries Array or Tuple shape (including
//!   `ReadonlyArray`, readonly tuple wrappers, and `Application` forms
//!   that evaluate to either),
//! - reject when the target object carries any named property other than
//!   the iterable protocol (`[Symbol.iterator]` / `__@iterator`) and
//!   `length`.
//!
//! Without this guard the call-site shortcut silently accepts
//! `string` for a parameter shaped like `T[]`, which then makes the
//! generic overload pick T = unknown (the unconstrained default). The
//! later non-generic `string` overload is never tried, and the call
//! returns `unknown` when it should have returned `string`.

use tsz_checker::test_utils::{
    DEFAULT_LIB_NAMES, check_source_strict_messages_without_missing_libs, check_source_with_libs,
    load_lib_files, strict_checker_options,
};

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_messages_without_missing_libs(source)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

fn codes_with_libs(source: &str) -> Vec<u32> {
    // TS2318 ("Cannot find global type ...") fires when an es2015 lib slot
    // referenced indirectly by the loaded set isn't on disk in the lib root
    // probe order; it has no bearing on the structural rule under test.
    let libs = load_lib_files(DEFAULT_LIB_NAMES);
    check_source_with_libs(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|d| d.code)
        .filter(|c| *c != 2318)
        .collect()
}

// ---------------------------------------------------------------------------
// Bug #6624 / #6500: generic `T[]` overload first, non-generic `string` second.
// ---------------------------------------------------------------------------

/// The minimal repro from #6624. `first("hello")` MUST select the
/// non-generic `string` overload and return `string`, not let the
/// generic `T[]` overload match with T = unknown.
#[test]
fn first_string_overload_selected_when_generic_array_is_first() {
    let source = r#"
function first<T>(arr: T[]): T;
function first(arr: string): string;
function first(arr: any): any {
    return typeof arr === "string" ? arr[0] : arr[0];
}

const f1: number = first([1, 2, 3]);
const f2: string = first("hello");
"#;
    let observed = codes(source);
    assert!(
        observed.is_empty(),
        "string-arg call must select the non-generic string overload, got: {observed:?}"
    );
}

/// Bug #6500 — the same root cause but with a single combined `T[] | string`
/// implementation signature. Same expectation: pick the string overload.
#[test]
fn first_string_overload_with_unioned_implementation_signature() {
    let source = r#"
function first<T>(arr: T[]): T;
function first(arr: string): string;
function first<T>(arr: T[] | string): T | string {
    if (typeof arr === "string") return arr[0];
    return arr[0];
}

const item: number = first([1, 2, 3]);
const ch: string = first("hello");
"#;
    let observed = codes(source);
    assert!(
        observed.is_empty(),
        "first(\"hello\") must return string under the second overload, got: {observed:?}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent shapes that must keep failing — the structural shortcut should
// NOT promote a string into any of these targets just because they happen
// to be iterable.
// ---------------------------------------------------------------------------

/// `T[]` directly: passing a string must be rejected (TS2345 / TS2322 /
/// TS2769 — all three are valid surface diagnostics depending on whether
/// the call resolves an overload or fails on a single signature, but the
/// call MUST NOT silently succeed).
#[test]
fn plain_generic_array_rejects_string_argument() {
    let source = r#"
declare function takeArr<T>(arr: T[]): T;
const x = takeArr("hello");
"#;
    let observed = codes(source);
    assert!(
        observed
            .iter()
            .any(|c| *c == 2345 || *c == 2322 || *c == 2769),
        "Generic T[] parameter must reject a string argument, got: {observed:?}"
    );
}

/// `Tuple` (variadic) target — also rejects strings.
#[test]
fn tuple_target_rejects_string_argument() {
    let source = r#"
declare function takeTuple<T>(t: [T, T]): T;
const x = takeTuple("hi");
"#;
    let observed = codes(source);
    assert!(
        observed
            .iter()
            .any(|c| *c == 2345 || *c == 2322 || *c == 2769),
        "Tuple [T, T] parameter must reject a string argument, got: {observed:?}"
    );
}

/// `readonly T[]` (`ReadonlyArray<T>`) — rejects strings too.
#[test]
fn readonly_array_target_rejects_string_argument() {
    let source = r#"
declare function takeReadonly<T>(arr: readonly T[]): T;
const x = takeReadonly("hi");
"#;
    let observed = codes(source);
    assert!(
        observed
            .iter()
            .any(|c| *c == 2345 || *c == 2322 || *c == 2769),
        "readonly T[] parameter must reject a string argument, got: {observed:?}"
    );
}

/// User-defined interface with `[Symbol.iterator]` plus an unrelated
/// member (here `kind: "x"`) must NOT accept a string — the string
/// satisfies the iterable half but lacks `kind`.
#[test]
fn iterable_interface_with_extra_member_rejects_string() {
    let source = r#"
interface KindedIter<T> {
    [Symbol.iterator](): Iterator<T>;
    kind: "x";
}
declare function takeKindedIter<T>(it: KindedIter<T>): T;
const x = takeKindedIter("hi");
"#;
    let observed = codes_with_libs(source);
    assert!(
        observed
            .iter()
            .any(|c| *c == 2345 || *c == 2322 || *c == 2769),
        "iterable-with-extra-property parameter must reject a string argument, got: {observed:?}"
    );
}

// ---------------------------------------------------------------------------
// Adjacent shapes that should still SUCCEED — the tightened shortcut must
// not regress the legitimate `string -> Iterable<X>` path.
// ---------------------------------------------------------------------------

/// `string` IS structurally `Iterable<string>`, so a generic
/// `Iterable<T>` parameter must accept a string and bind T to `string`.
#[test]
fn iterable_target_accepts_string_argument_and_binds_t_to_string() {
    let source = r#"
declare function takeIter<T>(it: Iterable<T>): T;
const s: string = takeIter("hello");
"#;
    let observed = codes_with_libs(source);
    assert!(
        observed.is_empty(),
        "string -> Iterable<T> must succeed and bind T = string, got: {observed:?}"
    );
}

/// Same case via a renamed type parameter — exercises the rule that the
/// fix must be structural, not name-keyed.
#[test]
fn iterable_target_accepts_string_argument_with_renamed_type_param() {
    let source = r#"
declare function takeIter<U>(it: Iterable<U>): U;
const s: string = takeIter("hello");
"#;
    let observed = codes_with_libs(source);
    assert!(
        observed.is_empty(),
        "string -> Iterable<U> must succeed regardless of the type-parameter name, got: {observed:?}"
    );
}
