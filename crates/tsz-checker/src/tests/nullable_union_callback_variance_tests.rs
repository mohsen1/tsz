//! Regression tests for method parameters typed as a nullable union of a
//! callback (the exact shape of `Promise.then`'s `onfulfilled`).
//!
//! tsc detects callback parameters with `getSingleCallSignature(getNonNullableType(t))`,
//! i.e. it strips `null`/`undefined` before deciding whether a method parameter is a
//! callback. A method parameter typed `((value: T) => R) | undefined` must therefore be
//! compared with strict callback variance, not relaxed method bivariance — otherwise a
//! covariant container like `Promise<T>` is wrongly accepted in both directions.
//!
//! Before the fix, `((value: T) => R) | undefined` method parameters fell out of callback
//! detection (the union is not itself callable), so the comparison silently relaxed to
//! method bivariance and dropped the expected TS2322. Plain (non-union) callback
//! parameters were already handled, so these tests vary both the union wrapper and the
//! binder names to keep the fix structural rather than name- or shape-specific.
use crate::test_utils::check_source_diagnostics;

fn ts2322_count(src: &str) -> usize {
    check_source_diagnostics(src)
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// A `Promise.then`-shaped interface (`then(cb: ((v: T) => U) | undefined): Self<U>`)
/// is covariant in `T`. Assigning the wider instantiation to the narrower one must
/// report TS2322; the reverse assignment is sound and must stay clean.
#[test]
fn nullable_union_callback_method_is_covariant() {
    let diags = check_source_diagnostics(
        r#"
interface Wide { a: string }
interface Narrow extends Wide { b: string }
interface Thenable<T> {
    chain<U>(cb: ((value: T) => Thenable<U>) | undefined | null): Thenable<U>;
}
declare let wide: Thenable<Wide>;
declare let narrow: Thenable<Narrow>;
narrow = wide;  // error: Thenable<Wide> not assignable to Thenable<Narrow>
wide = narrow;  // ok
"#,
    );
    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one TS2322 on the widening assignment, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The same structural rule with different binder names and `| undefined` only,
/// to prove the fix keys off type structure, not specific identifiers.
#[test]
fn nullable_union_callback_method_is_covariant_renamed() {
    assert_eq!(
        ts2322_count(
            r#"
interface Base { id: number }
interface Derived extends Base { extra: number }
interface Box<Elem> {
    map<Out>(transform: ((item: Elem) => Box<Out>) | undefined): Box<Out>;
}
declare let loose: Box<Base>;
declare let tight: Box<Derived>;
tight = loose;  // error
loose = tight;  // ok
"#,
        ),
        1,
        "renamed binders must still report the covariant mismatch exactly once",
    );
}

/// A plain (non-union) callback method parameter was already strict; this guards
/// that the fix did not change that established behavior.
#[test]
fn plain_callback_method_remains_covariant() {
    assert_eq!(
        ts2322_count(
            r#"
interface Wide { a: string }
interface Narrow extends Wide { b: string }
interface Thenable<T> {
    chain<U>(cb: (value: T) => Thenable<U>): Thenable<U>;
}
declare let wide: Thenable<Wide>;
declare let narrow: Thenable<Narrow>;
narrow = wide;  // error
wide = narrow;  // ok
"#,
        ),
        1,
        "plain callback method parameters must keep strict callback variance",
    );
}

/// When the nullability facts differ between the two sides (one side cannot be
/// `undefined`), tsc does not treat the parameters as matching callbacks. The
/// optional/required parameter mismatch is the dominant signal here; this test
/// only pins that the comparison stays sound (no spurious extra TS2322 beyond the
/// genuine widening one).
#[test]
fn nullable_union_callback_covariant_when_both_optional() {
    assert_eq!(
        ts2322_count(
            r#"
interface Wide { a: string }
interface Narrow extends Wide { b: string }
interface Thenable<T> {
    chain<U>(cb?: (value: T) => Thenable<U>): Thenable<U>;
}
declare let wide: Thenable<Wide>;
declare let narrow: Thenable<Narrow>;
narrow = wide;  // error
wide = narrow;  // ok
"#,
        ),
        1,
        "optional callback method parameters must keep strict callback variance",
    );
}
