//! Regression coverage: a homomorphic mapped type whose template is a
//! conditional carrying an `infer` must not leave a non-matching, ground-
//! primitive key in a non-canonical (deferred) form.
//!
//! For `type Unwrap<T> = { [K in keyof T]: T[K] extends Promise<infer U> ? U : T[K] }`
//! applied to a source with a *mix* of keys — some resolving the conditional's
//! true branch (`Promise<number>` → binds `infer U`) and some resolving its
//! false branch to a ground primitive (`string`) — the `string`-keyed property
//! was stored as the deferred conditional `string extends Promise<infer U> ? U
//! : string` (because `Promise`'s lib base was transiently unresolved when the
//! mapped type was evaluated) rather than as canonical `string`. The
//! assignability relation then bailed to a spurious `False` on that deferred
//! target — the reflexivity-breaking `string` not assignable to `string`, i.e.
//! `M` not assignable to `M` — producing a false-positive TS2322 (#17537).
//!
//! The fix reduces such a determinable (concrete-check) deferred conditional at
//! the relation boundary, where the lib base is resolvable, instead of bailing.
//! It is deliberately *not* a blanket "ground primitive → false branch": a
//! ground primitive genuinely *is* assignable to some generic structural
//! interfaces (`string extends Iterable<infer U>` / `ArrayLike<infer U>` bind
//! `U = string` and take the true branch — oracle-verified), so the relation
//! must reduce, not assume.
//!
//! Oracle-verified against `typescript@6.0.2` (`--strict`): every positive row
//! below is clean and every negative row reports TS2322. Binder names are
//! varied so the rule stays structural rather than keyed on `T`/`U`/`Unwrap`.

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

fn assert_clean(source: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics, got {found:?} for source:\n{source}"
    );
}

fn assert_has_ts2322(source: &str) {
    let found = codes(source);
    assert!(
        found.contains(&2322),
        "expected TS2322, got {found:?} for source:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// Positive: mixed-key unwrappers assign cleanly (no reflexivity-breaking FP).
// ---------------------------------------------------------------------------

#[test]
fn promise_unwrap_mixed_keys_assigns_cleanly() {
    // The minimal witness from #17537: the `b: string` key resolves the false
    // branch to the ground primitive `string`.
    assert_clean(
        r#"
type UnwrapAll<T> = { [K in keyof T]: T[K] extends Promise<infer U> ? U : T[K] };
const r: UnwrapAll<{ a: Promise<number>; b: string }> = { a: 1, b: "s" };
"#,
    );
}

#[test]
fn promise_unwrap_renamed_binders_assigns_cleanly() {
    // Same shape, every binder renamed — the fix is structural, not name-keyed.
    assert_clean(
        r#"
type Zz<Qq> = { [Ww in keyof Qq]: Qq[Ww] extends Promise<infer Rr> ? Rr : Qq[Ww] };
const p: Zz<{ x: Promise<boolean>; y: number }> = { x: true, y: 3 };
"#,
    );
}

#[test]
fn promise_unwrap_fixed_literal_false_branch_assigns_cleanly() {
    // False branch is a fixed literal rather than `T[K]`; the non-matching key
    // was stored as the deferred `... ? U : "X"` (`"X"` not assignable to `"X"`).
    assert_clean(
        r#"
type UnwrapLit<T> = { [K in keyof T]: T[K] extends Promise<infer U> ? U : "X" };
const p: UnwrapLit<{ a: Promise<number>; b: "X" }> = { a: 1, b: "X" };
"#,
    );
}

#[test]
fn array_unwrap_mixed_keys_assigns_cleanly() {
    assert_clean(
        r#"
type UnwrapArr<T> = { [K in keyof T]: T[K] extends Array<infer U> ? U : T[K] };
const p: UnwrapArr<{ a: number[]; b: string }> = { a: 1, b: "s" };
"#,
    );
}

#[test]
fn user_generic_interface_unwrap_mixed_keys_assigns_cleanly() {
    // Base is a *user* generic interface, not a lib global — the fix is not
    // specific to lib-ness of the base.
    assert_clean(
        r#"
interface Box<Zt> { value: Zt }
type UnwrapBox<T> = { [K in keyof T]: T[K] extends Box<infer U> ? U : T[K] };
const p: UnwrapBox<{ a: Box<number>; b: string }> = { a: 1, b: "s" };
"#,
    );
}

#[test]
fn promise_unwrap_union_primitive_false_branch_assigns_cleanly() {
    assert_clean(
        r#"
type UnwrapU<T> = { [K in keyof T]: T[K] extends Promise<infer U> ? U : T[K] };
const p: UnwrapU<{ a: Promise<number>; b: string | number }> = { a: 1, b: "s" };
"#,
    );
}

// ---------------------------------------------------------------------------
// Negative: genuine mismatches still report TS2322. These rows double as the
// tripwire that the libs harness *can* observe TS2322 for this shape — a clean
// positive above is therefore a real result, not a silently dropped diagnostic.
// ---------------------------------------------------------------------------

#[test]
fn promise_unwrap_wrong_matching_key_value_reports_ts2322() {
    // `a` unwraps to `number`; assigning `"x"` must still fail.
    assert_has_ts2322(
        r#"
type UnwrapAll<T> = { [K in keyof T]: T[K] extends Promise<infer U> ? U : T[K] };
const bad: UnwrapAll<{ a: Promise<number>; b: string }> = { a: "x", b: "s" };
"#,
    );
}

#[test]
fn promise_unwrap_wrong_non_matching_primitive_key_reports_ts2322() {
    // `b` is the ground-primitive `string` key; assigning a number must fail —
    // the reduction is not silently accepting everything.
    assert_has_ts2322(
        r#"
type UnwrapArr<T> = { [K in keyof T]: T[K] extends Array<infer U> ? U : T[K] };
const bad: UnwrapArr<{ a: number[]; b: string }> = { a: 1, b: 5 };
"#,
    );
}
