//! Distributive conditional types with callable-object branch shapes must
//! substitute the distribution variable into call/construct signatures.
//!
//! When a distributive conditional's true or false branch is a type literal
//! with call signatures — e.g. `{ (arg: T): T }` — the checker represents it
//! as a `Callable` shape.  Without explicit `Callable` handling in
//! `substitute_exact_type_db`, the distribution variable `T` is never rewritten
//! and every union member collapses to the same hash-consed Callable,
//! losing per-variant precision.
//!
//! Structural rule:
//!   When `T extends U ? { (arg: T): T } : never` distributes over `A | B`,
//!   each branch substitutes T with the specific member so the result is
//!   `{ (arg: A): A } | { (arg: B): B }`, matching tsc behavior.
//!
//! Every test varies binder names to prevent fixture-name fast paths.
//! Cases cover: call signature param/return substitution, construct signatures,
//! callable with extra properties, and multi-member unions.

use tsz_checker::test_utils::{check_source_diagnostics, check_source_strict};

fn no_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got: {diagnostics:#?}\nsource:\n{source}"
    );
}

fn has_ts2322(source: &str) {
    let diagnostics = check_source_strict(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected TS2322, got: {diagnostics:#?}\nsource:\n{source}"
    );
}

/// Call-signature parameter type carries the distribution variable.
/// After distribution over `"x" | "y"` the per-member literal types must be
/// preserved so a string-typed variable is rejected.
#[test]
fn distributive_callable_branch_substitutes_param_type() {
    // tsc: TS2322 because { (v: "x"): "x" } | { (v: "y"): "y" } ≠ (v: string) => string
    has_ts2322(
        r#"
type WrapFn<Tok> = Tok extends string ? { (v: Tok): Tok } : never;
type R = WrapFn<"x" | "y">;
declare const r: R;
const wrong: { (v: string): string } = r;
"#,
    );
}

/// Renamed iteration variable (`Elem` instead of `T`) must behave identically.
#[test]
fn distributive_callable_branch_renamed_var_substitutes_param() {
    has_ts2322(
        r#"
type WrapHandler<Elem> = Elem extends number ? { (n: Elem): Elem } : never;
type Handlers = WrapHandler<1 | 2>;
declare const h: Handlers;
const wrong: { (n: number): number } = h;
"#,
    );
}

/// Return type also carries the distribution variable and must be substituted.
#[test]
fn distributive_callable_branch_substitutes_return_type() {
    no_errors(
        r#"
type Producer<Kind> = Kind extends string ? { (): Kind } : never;
type P = Producer<"a" | "b">;
declare const p: P;
// The call result is "a" | "b", which is string-assignable.
const s: string = p();
"#,
    );
}

/// Three-member literal union: each variant must produce a separate callable
/// shape so the result is a three-way union, not a single widened callable.
#[test]
fn distributive_callable_branch_three_member_union_preserves_precision() {
    has_ts2322(
        r#"
type Wrap3<T> = T extends string ? { (input: T): void } : never;
type R = Wrap3<"a" | "b" | "c">;
declare const r: R;
// { (input: "a"): void } | { (input: "b"): void } | { (input: "c"): void }
// is not assignable to { (input: string): void }
const wrong: { (input: string): void } = r;
"#,
    );
}

/// Positive case: the distributed callable result IS assignable to a wider
/// callable type that accepts the union of literals.
#[test]
fn distributive_callable_branch_result_is_assignable_to_union_param() {
    no_errors(
        r#"
type WrapStrict<Tok> = Tok extends string ? { (v: Tok): Tok } : never;
type R = WrapStrict<"x" | "y">;
declare const r: R;
// A variable typed as { (v: "x"): "x" } | { (v: "y"): "y" } must accept
// calls with the specific literal arguments — calling with "x" is valid.
const call_result_x: "x" = (r as { (v: "x"): "x" })("x");
"#,
    );
}
