//! Regression coverage for TS2536/TS2538 false positives when a naked type
//! parameter used as the *index* of an indexed access is the narrowed check
//! type of an enclosing conditional (`T extends U ? Obj[T] : Y`), and that
//! access is wrapped in another generic alias (`Wrap<Obj[T]>`).
//!
//! Within the true branch, tsc narrows `T` to (a subtype of) `U`, so the
//! access is valid whenever `U` is itself a valid key of `Obj` — regardless of
//! `T`'s own wider declared constraint (e.g. a union with an unrelated,
//! non-key member such as `boolean`). tsc validates this eagerly (accepted
//! even with zero instantiation), so the check does not depend on the access
//! ever being called/instantiated concretely.
//!
//! A bare, unwrapped access (`T extends U ? Obj[T] : Y`) already worked before
//! this fix; wrapping the same access in another generic alias forces eager
//! type-argument elaboration of `Obj[T]` ahead of any use-site narrowing,
//! which is what exposed the false positive (kysely's `FunctionModule`,
//! #16025).

use crate::test_utils::check_source_codes;

/// Minimal repro: index-narrowed conditional wrapped in a further generic
/// alias. `TB` is a valid key of `DB`; `boolean` (the other union member of
/// `T`'s own declared constraint) is not, but is unreachable in this branch.
#[test]
fn index_narrowed_conditional_through_generic_alias_no_ts2536_or_ts2538() {
    let codes = check_source_codes(
        r#"
type Id<T> = T
interface Foo<DB, TB extends keyof DB> {
  toJson<T extends TB | boolean>(table: T): T extends TB ? Id<DB[T]> : never
}
"#,
    );
    assert!(
        !codes.contains(&2536) && !codes.contains(&2538),
        "expected no TS2536/TS2538 for an index-narrowed conditional wrapped in a generic alias: {codes:?}"
    );
}

/// Renamed binders: the fix keys off name-equality between the index and the
/// conditional's check type, so a rename must not disable it.
#[test]
fn index_narrowed_conditional_through_generic_alias_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Wrap<Value> = Value
interface Container<Data, Key extends keyof Data> {
  read<Arg extends Key | boolean>(name: Arg): Arg extends Key ? Wrap<Data[Arg]> : never
}
"#,
    );
    assert!(
        !codes.contains(&2536) && !codes.contains(&2538),
        "renaming every binder must not disable the index-narrowing fix: {codes:?}"
    );
}

/// Control: the same access with no wrapping alias must remain clean (this
/// already worked before the fix; guards against a regression on the direct
/// path).
#[test]
fn index_narrowed_conditional_bare_access_still_no_ts2536_or_ts2538() {
    let codes = check_source_codes(
        r#"
interface Foo<DB, TB extends keyof DB> {
  toJson<T extends TB | boolean>(table: T): T extends TB ? DB[T] : never
}
"#,
    );
    assert!(
        !codes.contains(&2536) && !codes.contains(&2538),
        "the bare (unwrapped) access must stay clean: {codes:?}"
    );
}

/// Negative control: the conditional's `extends` type is *not* itself a valid
/// key of the object (`X` is an unconstrained type parameter, not proven to be
/// a subtype of `keyof DB`), so narrowing `T` to `X` does not make `DB[T]`
/// valid and TS2536 must still fire — this is the case that separates
/// "index is the narrowed check type" from "always suppress".
#[test]
fn index_narrowed_conditional_with_non_key_extends_still_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Id<T> = T
interface Foo<DB, X> {
  toJson<T extends X | boolean>(table: T): T extends X ? Id<DB[T]> : never
}
"#,
    );
    assert!(
        codes.contains(&2536),
        "narrowing to an extends-type that is not itself a valid key must still emit TS2536: {codes:?}"
    );
}

/// Negative control: an index that is *not* the conditional's own check type
/// (a sibling type parameter of the same name space, but structurally
/// unrelated to the enclosing conditional) must not be spuriously suppressed.
#[test]
fn unrelated_index_type_param_still_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Id<T> = T
interface Foo<DB, TB extends keyof DB, Other extends TB | boolean> {
  toJson<T extends TB | boolean>(table: T): T extends TB ? Id<DB[Other]> : never
}
"#,
    );
    assert!(
        codes.contains(&2536) || codes.contains(&2538),
        "an index unrelated to the conditional's own check type must not be suppressed: {codes:?}"
    );
}

/// Object-narrowed mirror case (`T extends X ? T[K] : Y`, the object itself is
/// the narrowed check type) must remain unaffected by the index-narrowed
/// addition living in the same helper.
#[test]
fn object_narrowed_conditional_still_no_ts2536() {
    let codes = check_source_codes(
        r#"
type Id<T> = T
type Get<T, K extends keyof { a: string; b: number }> =
  T extends { a: string; b: number } ? Id<T[K]> : never
"#,
    );
    assert!(
        !codes.contains(&2536),
        "the pre-existing object-narrowed case must still be accepted: {codes:?}"
    );
}
