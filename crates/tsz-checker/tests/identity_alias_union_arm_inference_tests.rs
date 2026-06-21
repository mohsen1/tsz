//! Regression tests for issue #14251.
//!
//! When a generic call infers a type parameter `A` from a union target whose
//! arm is a *transparent identity-alias application* — an alias whose body is
//! exactly its own type parameter, e.g. `type Some<X> = X`, applied as
//! `Some<A>` — tsc treats the arm as the naked variable `A` and unions the
//! unmatched source constituents into it (`A = number`). tsz previously kept
//! `Some<A>` as an opaque `Application`, so the arm partitioned as a structured
//! member instead of a naked variable; the unmatched source constituents never
//! reached `A`, which collapsed to its `{}` default and leaked the `None`
//! branch, producing a spurious `TS2362` (`O.map(O.fromNullable(...), x => x * 2)`
//! mined from ts-belt).
//!
//! The fix peels a transparent identity-alias arm to its forwarded type during
//! union-inference partitioning. These tests pin the behaviour for several
//! structurally equivalent shapes — not just the reported spelling — and guard
//! the cases that must *not* be peeled (aliases whose body adds structure).

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn diag_count(source: &str, code: u32) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(c, _)| *c == code)
        .count()
}

fn total_diags(source: &str) -> usize {
    compile_and_get_diagnostics(source).len()
}

/// The reported repro: ts-belt-style `Option`, overloaded `map`, identity-alias
/// `Some<A>` arm. tsc infers `A = number`, so `x * 2` is clean.
#[test]
fn ts_belt_option_map_identity_alias_arm_no_ts2362() {
    let source = r#"
type Some<A> = A
type None = undefined | null
type Option<A> = Some<A> | None
declare function fromNullable<A>(value: A): Option<NonNullable<A>>
declare function map<A, B>(option: Option<A>, mapFn: (value: A) => B): Option<B>
declare function map<A, B>(mapFn: (value: A) => B): (option: Option<A>) => Option<B>
const o1 = fromNullable(5 as number | null)
const b = map(o1, x => x * 2)
"#;
    assert_eq!(
        diag_count(source, 2362),
        0,
        "identity-alias `Some<A>` arm must infer A = number, leaving `x * 2` clean"
    );
    assert_eq!(
        total_diags(source),
        0,
        "the whole program should type-check clean"
    );
}

/// Without the data-last overload the single-signature form must also be clean:
/// the trigger is the identity-alias arm, not the overload count.
#[test]
fn single_signature_identity_alias_arm_no_ts2362() {
    let source = r#"
type Some<A> = A
type None = undefined | null
type Option<A> = Some<A> | None
declare function map<A, B>(o: Option<A>, f: (v: A) => B): Option<B>
declare const o: Option<number>
const b = map(o, x => x * 2)
"#;
    assert_eq!(diag_count(source, 2362), 0);
    assert_eq!(total_diags(source), 0);
}

/// The rule is structural, not tied to the identifiers `Some`/`A`/`None`.
#[test]
fn renamed_binders_identity_alias_arm_no_ts2362() {
    let source = r#"
type Forward<Inner> = Inner
type Nothing = undefined | null
type Maybe<Inner> = Forward<Inner> | Nothing
declare function lift<Inner, Out>(m: Maybe<Inner>, fn: (v: Inner) => Out): Maybe<Out>
declare const m: Maybe<number>
const r = lift(m, n => n * 2)
"#;
    assert_eq!(diag_count(source, 2362), 0);
    assert_eq!(total_diags(source), 0);
}

/// A single-member `None` (just `undefined`) exercises the same partitioning.
#[test]
fn single_member_none_identity_alias_arm_no_ts2362() {
    let source = r#"
type Some<A> = A
type Option<A> = Some<A> | undefined
declare function map<A, B>(o: Option<A>, f: (v: A) => B): Option<B>
declare const o: Option<number>
const b = map(o, x => x * 2)
"#;
    assert_eq!(diag_count(source, 2362), 0);
    assert_eq!(total_diags(source), 0);
}

/// The identity-alias arm also forwards a non-numeric element correctly: `A`
/// must infer `string`, so a numeric use of the parameter is rejected (TS2362)
/// — the peel preserves the *correct* element, it does not blanket-suppress.
#[test]
fn identity_alias_arm_forwards_string_still_reports_ts2362() {
    let source = r#"
type Some<A> = A
type None = undefined | null
type Option<A> = Some<A> | None
declare function map<A, B>(o: Option<A>, f: (v: A) => B): Option<B>
declare const o: Option<string>
const b = map(o, x => x * 2)
"#;
    assert_eq!(
        diag_count(source, 2362),
        1,
        "A must infer `string`; using it as the LHS of `* 2` is a real TS2362"
    );
}

/// A non-identity alias whose body adds structure (`type Box<X> = { v: X }`)
/// must NOT be peeled: the structured arm still drives inference and a property
/// access through it remains well typed.
#[test]
fn structured_alias_arm_is_not_peeled() {
    let source = r#"
type Box<X> = { v: X }
type None = undefined | null
type Boxed<X> = Box<X> | None
declare function unbox<X, R>(b: Boxed<X>, f: (b: { v: X }) => R): R
declare const bx: Boxed<number>
const out = unbox(bx, b => b.v * 2)
"#;
    assert_eq!(diag_count(source, 2362), 0);
    assert_eq!(total_diags(source), 0);
}
