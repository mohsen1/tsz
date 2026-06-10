//! Tests for literal preservation when a generic type parameter's declared
//! constraint is a tuple/array carrying a primitive-constrained type parameter,
//! e.g. `arrayToEnum<T extends string, U extends [T, ...T[]]>(items: U)`.
//!
//! tsc preserves the literal element types inferred for `U` (so
//! `{ [k in U[number]]: k }` keeps concrete literal keys and `U[number]` is the
//! literal union). tsz widened them to `string` because the literal-preservation
//! gate (`constraint_is_primitive_type` in the generic-call layer, consulted by
//! `mark_declared_constraint_preserves_literals`) did not recognise that a tuple
//! constraint `[T, ...T[]]` carries the primitive-constrained parameter `T` in
//! its element/rest types.
//!
//! Structural rule: when inferring `U` whose declared constraint is a tuple
//! (or array) whose element/rest type is a type parameter with a primitive
//! constraint, fresh literal candidates for `U` are preserved (not widened),
//! matching tsc's `getCovariantInference` literal-preservation gate. This is the
//! cross-file zod `arrayToEnum`/`ZodIssueCode` mapped-enum family.
//!
//! Binder names are varied across cases; the rule must be name-independent.

use crate::test_utils::check_source_diagnostics;

fn ts2322(src: &str) -> Vec<String> {
    check_source_diagnostics(src)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn variadic_tuple_constraint_preserves_literal_elements() {
    // `U extends [T, ...T[]]` with `T extends string`: `u[0]` must stay `"a"`.
    let errs = ts2322(
        r#"
declare function f<T extends string, U extends [T, ...T[]]>(items: U): U;
const u = f(["a", "b"]);
const a: "a" = u[0];
const b: "b" = u[1];
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn fixed_tuple_constraint_preserves_literal_elements() {
    // Renamed binders + fixed (non-variadic) tuple constraint `[X, X]`.
    let errs = ts2322(
        r#"
declare function collect<X extends string, V extends [X, X]>(values: V): V;
const v = collect(["red", "blue"]);
const first: "red" = v[0];
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn mapped_over_variadic_tuple_index_keeps_literal_keys() {
    // The zod `arrayToEnum` shape: `{ [k in U[number]]: k }` must keep concrete
    // literal keys so indexed access and `keyof` stay literal.
    let errs = ts2322(
        r#"
declare function arrayToEnum<T extends string, U extends [T, ...T[]]>(
  items: U
): { [k in U[number]]: k };
const E = arrayToEnum(["alpha", "beta", "gamma"]);
const k: "alpha" = E["alpha"];
const key: "alpha" | "beta" | "gamma" = "beta" as keyof typeof E;
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn array_constraint_with_primitive_param_preserves_literals() {
    // Array (not tuple) constraint `U extends T[]` with `T extends string`.
    let errs = ts2322(
        r#"
declare function g<T extends string, U extends T[]>(items: U): U;
const u = g(["x", "y"]);
const first: "x" | "y" = u[0];
"#,
    );
    assert!(errs.is_empty(), "expected no TS2322, got: {errs:?}");
}

#[test]
fn unconstrained_string_array_constraint_still_widens() {
    // Negative control: `U extends string[]` carries no primitive-constrained
    // type parameter, so tsc widens the elements to `string`; a literal target
    // must still be rejected (parity with tsc).
    let errs = ts2322(
        r#"
declare function h<U extends string[]>(items: U): U;
const u = h(["a", "b"]);
const first: "a" = u[0];
"#,
    );
    assert_eq!(
        errs.len(),
        1,
        "expected TS2322 (widened to string), got: {errs:?}"
    );
}

#[test]
fn preserved_literal_wrong_target_still_rejected() {
    // Negative control: literals are preserved, so the precise element type is
    // observable. `u[1]` is `"b"`; assigning it to `"a"` must fail (parity with
    // tsc), proving the preservation did not collapse the element to a single
    // value or widen it away.
    let errs = ts2322(
        r#"
declare function f<T extends string, U extends [T, ...T[]]>(items: U): U;
const u = f(["a", "b"]);
const bad: "a" = u[1];
"#,
    );
    assert_eq!(errs.len(), 1, "expected TS2322, got: {errs:?}");
}
