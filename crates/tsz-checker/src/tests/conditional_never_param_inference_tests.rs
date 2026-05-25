//! Tests for issue #9652 — when a generic call's inferred type argument is
//! preserved as a literal (because the type parameter is at the top level of
//! the return type) and a parameter type that is a conditional / `Exclude` over
//! that type argument reduces to `never`, the forbidden argument must be
//! rejected with `TS2345`, matching `tsc`.
//!
//! The structural rule: once inference fixes the type argument, the instantiated
//! parameter type is evaluated; if it reduces to `never` (e.g.
//! `'a' extends 'a' ? never : 'a'`), the argument is checked against `never` and
//! rejected. Literal preservation is required for the reduction to fire, so it
//! follows `tsc`'s `widenLiteralTypes` gate: the literal is kept only when the
//! type parameter is inferred from top-level positions and appears at the top
//! level of the return type. With a `void` return the literal is widened and the
//! conditional takes its false branch — `tsc` does not error there either.
use crate::test_utils::{check_source_diagnostics, diagnostics_with_code};

fn ts2345_count(source: &str) -> usize {
    let diags = check_source_diagnostics(source);
    diagnostics_with_code(&diags, 2345).len()
}

/// Reported repro: distributive conditional `T extends 'a' ? never : T` with the
/// type parameter at the top level of the return type. `T = 'a'` is preserved,
/// the parameter reduces to `never`, and `'a'` is rejected.
#[test]
fn distributive_conditional_never_param_rejects_forbidden_arg() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<T>(key: T extends 'a' ? never : T): T;
f('a');
"#,
        ),
        1,
    );
}

/// The same rule with a renamed type parameter — the fix must follow the
/// structural shape, not the spelling `T`.
#[test]
fn renamed_type_parameter_still_rejects() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<Key>(key: Key extends 'a' ? never : Key): Key;
f('a');
"#,
        ),
        1,
    );
}

/// Non-distributive conditional `[T] extends ['a'] ? never : T`.
#[test]
fn non_distributive_conditional_never_param_rejects() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<T>(key: [T] extends ['a'] ? never : T): T;
f('a');
"#,
        ),
        1,
    );
}

/// `keyof`-based conditional — the `extends` operand references a named type, so
/// reduction must resolve `keyof R` through the type environment.
#[test]
fn keyof_conditional_never_param_rejects() {
    assert_eq!(
        ts2345_count(
            r#"
type R = { a: 1 };
declare function f<K>(key: K extends keyof R ? never : K): K;
f('a');
"#,
        ),
        1,
    );
}

/// Alias application of a conditional (the `Exclude` idiom) that reduces to
/// `never`. Defined inline so the test does not depend on the standard library.
#[test]
fn exclude_alias_never_param_rejects() {
    assert_eq!(
        ts2345_count(
            r#"
type MyExclude<T, U> = T extends U ? never : T;
declare function f<K>(key: MyExclude<K, 'a'>): K;
f('a');
"#,
        ),
        1,
    );
}

/// Positive control: an allowed argument takes the conditional's false branch
/// and must NOT error.
#[test]
fn allowed_argument_is_accepted() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<T>(key: T extends 'a' ? never : T): T;
f('b');
"#,
        ),
        0,
    );
}

/// Negative / fallback control: with a `void` return the type parameter is not
/// at the top level of the return type, so `tsc` widens the inferred literal to
/// its primitive and the conditional takes its false branch — no `TS2345`.
#[test]
fn void_return_widens_and_does_not_reject() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<T>(key: T extends 'a' ? never : T): void;
f('a');
"#,
        ),
        0,
    );
}

/// Multi-argument: only the conditional-typed parameter drives the rejection;
/// the leading concrete argument is fine.
#[test]
fn multi_arg_only_conditional_param_rejects() {
    assert_eq!(
        ts2345_count(
            r#"
declare function f<T>(a: number, key: T extends 'a' ? never : T): T;
f(1, 'a');
f(1, 'b');
"#,
        ),
        1,
    );
}

/// Method-chaining shape (type-challenges `Chainable`, #12): a duplicate key is
/// rejected while a fresh key is accepted.
#[test]
fn chainable_rejects_duplicate_key() {
    assert_eq!(
        ts2345_count(
            r#"
type Chainable<R = {}> = {
  option<K extends string, V>(key: K extends keyof R ? never : K, value: V): Chainable<R & { [P in K]: V }>;
  get(): R;
};
declare const c: Chainable;
c.option('a', 1).option('a', 2);
"#,
        ),
        1,
    );
}

#[test]
fn chainable_accepts_fresh_keys() {
    assert_eq!(
        ts2345_count(
            r#"
type Chainable<R = {}> = {
  option<K extends string, V>(key: K extends keyof R ? never : K, value: V): Chainable<R & { [P in K]: V }>;
  get(): R;
};
declare const c: Chainable;
c.option('x', 1).option('y', 2);
"#,
        ),
        0,
    );
}

/// Literal preservation must not over-fire: a union of fresh literals from
/// sibling direct arguments is still widened (`tsc` keeps `1 | 2`, which is a
/// non-error here), and a callback-return inference site widens to `number`.
#[test]
fn callback_return_site_widens_type_argument() {
    // U is inferred from a callback return position, so the literal `5` widens
    // to `number`; assigning the result to `5` is a TS2322 (not TS2345).
    let diags = check_source_diagnostics(
        r#"
declare function h<U>(fn: () => U, init: U): U;
const r: 5 = h(() => 5, 0);
"#,
    );
    assert_eq!(diagnostics_with_code(&diags, 2322).len(), 1);
}
