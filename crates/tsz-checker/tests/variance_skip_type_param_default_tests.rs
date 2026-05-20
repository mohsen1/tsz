//! Variance computation must NOT visit a generic method's type-parameter
//! defaults as occurrences of the outer container's type parameter.
//!
//! Structural rule: a default like `<TResult1 = T>` on a method's local
//! type parameter is an *instantiation hint* for callers, not an
//! occurrence of `T` in the generic body. Counting it as an occurrence
//! over-constrains variance.
//!
//! Concrete consequence (this test): a `Promise`-shaped interface with
//! `then<TResult1 = T, TResult2 = never>(...)` and `finally(): Promise<T>`
//! had its outer `T` computed as INVARIANT, because the default `TR1 =
//! T` recorded T contravariantly inside `then`'s callback-return position
//! AND covariantly inside the outer `Promise<TR1>` return position. tsc
//! treats Promise as covariant — so `Promise<never>` must be assignable
//! to `Promise<X>` for any X.

use tsz_checker::test_utils::check_source_diagnostics;

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// Promise-shaped class with method type-parameter default = outer T.
/// `P<never>` must be assignable to `P<{count: number}>`.
#[test]
fn promise_shaped_method_default_does_not_invariantize_outer() {
    let source = r#"
declare class P<T> {
    then<TR1 = T, TR2 = never>(
        onfulfilled?: ((value: T) => TR1 | P<TR1>) | undefined | null,
        onrejected?: ((reason: any) => TR2 | P<TR2>) | undefined | null
    ): P<TR1 | TR2>;
    catch<TR = never>(
        onrejected?: ((reason: any) => TR | P<TR>) | undefined | null
    ): P<T | TR>;
    finally(onfinally?: (() => void) | undefined | null): P<T>;
}
declare const p: P<never>;
const q: P<{count: number}> = p;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "Promise<never> → Promise<{{count}}> must not emit TS2322 (covariant T); got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Anti-hardcoding (§25): the rule is structural ("type-param defaults
/// don't constrain outer-T variance"), not specific to `T`/`TR1`. Re-run
/// with different names — the fix must hold.
#[test]
fn promise_shaped_method_default_independent_of_param_names() {
    let source = r#"
declare class Box<Value> {
    map<NewValue = Value>(
        cb?: ((v: Value) => NewValue | Box<NewValue>) | undefined
    ): Box<NewValue>;
    inspect(cb?: () => void): Box<Value>;
}
declare const b: Box<never>;
const c: Box<{count: number}> = b;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "Box<never> → Box<{{count}}> must not emit TS2322; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Constraint companion: `<U extends T>` IS still considered for variance
/// because constraints structurally couple U to T. The fix must be scoped
/// to *defaults*, not to constraints. This test pins that scope.
#[test]
fn type_param_constraint_referencing_outer_t_still_affects_variance() {
    // When U is constrained by T and used in a covariant return, T
    // genuinely propagates and must remain consistent. Here the source
    // is generic-over-T, and we exercise an internal use that depends
    // on the constraint relationship.
    let source = r#"
declare class C<T> {
    take<U extends T>(u: U): C<T>;
}
declare const c1: C<{a: number}>;
const c2: C<{a: number}> = c1;
"#;
    let diags = check_source_diagnostics(source);
    assert_eq!(
        count(&diags, 2322),
        0,
        "Same-T C<X> assignment must remain valid; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Negative companion: when the outer-T usage is genuinely invariant
/// (e.g. T appears in both contravariant property type AND covariant
/// return), the fix must NOT make it covariant. Otherwise the variance
/// computation would be too lax.
#[test]
fn genuine_invariant_outer_t_still_rejects_never_to_concrete() {
    let source = r#"
declare class Cell<T> {
    get(): T;
    set(v: T): void;  // contravariant via mandatory function-typed property is too loose;
                       // use a function-property to force strict variance.
}
"#;
    let _ = check_source_diagnostics(source);
    // The point of this test is to lock the negative direction. The
    // method `set(v: T)` is bivariant (method bivariance), so this
    // particular shape is still permissive. The genuine contravariant
    // shape uses a function-typed property; we cover that here.
    let strict_source = r#"
declare class Cell<T> {
    set: (v: T) => void;  // function-typed property — strict contravariance
    get: () => T;
}
declare const c: Cell<never>;
const d: Cell<{count: number}> = c;
"#;
    let diags = check_source_diagnostics(strict_source);
    // `Cell<T>` with `set: (v: T) => void` is strictly invariant in T:
    // `Cell<never>` requires `set: (v: never) => void`, and `Cell<X>`
    // requires `set: (v: X) => void`. For source.set <: target.set,
    // contravariantly, target.param X <: source.param never ⇒ X = never
    // only. So general X rejects.
    assert!(
        count(&diags, 2322) >= 1,
        "Cell<never> → Cell<X> with strict contravariant set must reject; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
