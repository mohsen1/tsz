//! Regression tests for a false-positive TS7006 (implicit-`any`) on the
//! parameters of an object-literal member returned by the inner arrow of a
//! curried `(a) => (): T => ({ m: (x, y) => ... })`.
//!
//! Structural rule: when an expression-bodied arrow with an explicit return
//! annotation returns an object literal, that annotation contextually types the
//! object literal's members, so a member function/method's parameters are
//! contextually typed and must NOT raise TS7006. `tsc` reports nothing for the
//! members below.
//!
//! tsz previously emitted TS7006 only for the *curried/nested* form: the outer
//! (unannotated) arrow runs speculative return-type inference over its body,
//! which checks the inner member WITH its contextual type but then rolls back
//! the `implicit_any_checked_closures` mark; the authoritative re-check re-enters
//! the member WITHOUT re-deriving the contextual signature and re-emits TS7006.
//! The fix preserves the contextually-validated mark across the speculative
//! rollback (and tracks object-literal method shorthand the same way arrows and
//! function-expression initializers were already tracked).
//!
//! Every case varies its binder names (interface, alias, variable, and parameter
//! identifiers) so the behavior cannot be keyed to a particular spelling, and
//! each genuine-implicit-any counterpart is checked to confirm the fix does not
//! over-suppress.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..Default::default()
    }
}

fn count_7006(source: &str) -> usize {
    check_source(source, "test.ts", strict_options())
        .iter()
        .filter(|d| d.code == 7006)
        .count()
}

#[test]
fn curried_annotated_arrow_returns_object_with_arrow_member() {
    // tsc: clean. The inner arrow's `: Algebra` annotation contextually types
    // the returned object literal, so `meet`'s `(x, y)` are `number`.
    let source = r#"
        interface Algebra { readonly meet: (x: number, y: number) => number }
        export const f =
          (base: number) =>
          (): Algebra => ({
            meet: (x, y) => base + x + y
          });
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn curried_annotated_arrow_returns_object_with_method_shorthand() {
    // Same root cause as the arrow form, but the member is a method shorthand
    // (`meet(x, y) { ... }`), which is not an `is_closure` node yet is still
    // contextually typed by the surrounding object literal.
    let source = r#"
        interface Algebra { readonly meet: (x: number, y: number) => number }
        export const f =
          (base: number) =>
          (): Algebra => ({
            meet(x, y) { return base + x + y; }
          });
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn curried_annotated_arrow_renamed_binders_no_false_positive() {
    // Renamed interface / alias / variable / parameter spellings: the fix is
    // structural, not keyed to identifier names.
    let source = r#"
        interface Ring { readonly combine: (alpha: string, beta: string) => string }
        type RingFactory = (seed: string) => () => Ring;
        export const build: RingFactory =
          (seed) =>
          () => ({
            combine: (alpha, beta) => seed + alpha + beta
          });
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn deeply_curried_annotated_arrow_no_false_positive() {
    // Three levels of currying still resolve the inner member's contextual type.
    let source = r#"
        interface Algebra { readonly meet: (x: number, y: number) => number }
        export const f =
          (a: number) =>
          (b: number) =>
          (): Algebra => ({
            meet: (x, y) => a + b + x + y
          });
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn conditional_body_returning_annotated_object_no_false_positive() {
    // The inner arrow returns `Algebra | null`; the `Algebra` arm still
    // contextually types the object literal's member parameters.
    let source = r#"
        interface Algebra { readonly meet: (p: number, q: number) => number }
        export const make =
          (flag: boolean) =>
          (n: number): Algebra | null =>
            flag ? { meet: (p, q) => p + q + n } : null;
    "#;
    assert_eq!(count_7006(source), 0);
}

#[test]
fn curried_unannotated_inner_arrow_still_reports_implicit_any() {
    // Negative case: with no return annotation on the inner arrow there is no
    // contextual type for the object literal, so tsc DOES report TS7006 for the
    // member's parameters. The fix must not suppress this.
    let source = r#"
        export const f =
          (base: number) =>
          () => ({
            meet: (x, y) => base + x + y
          });
    "#;
    assert_eq!(count_7006(source), 2);
}

#[test]
fn curried_unannotated_inner_arrow_method_shorthand_still_reports_implicit_any() {
    // Method-shorthand counterpart of the negative case.
    let source = r#"
        export const f =
          (base: number) =>
          () => ({
            meet(x, y) { return base + x + y; }
          });
    "#;
    assert_eq!(count_7006(source), 2);
}

#[test]
fn annotated_member_body_keeps_real_property_access_error() {
    // The contextual typing makes `y` a `number`, so the body raises TS2339
    // (`toUpperCase` does not exist on `number`) — and only that, with no
    // spurious TS7006 masking it.
    let source = r#"
        interface Algebra { readonly meet: (x: number, y: number) => number }
        export const f =
          (base: number) =>
          (): Algebra => ({
            meet: (x, y) => y.toUpperCase()
          });
    "#;
    let diagnostics = check_source(source, "test.ts", strict_options());
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 7006).count(),
        0,
        "no spurious TS7006: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2339),
        "expected TS2339 from `number.toUpperCase()` to survive: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
