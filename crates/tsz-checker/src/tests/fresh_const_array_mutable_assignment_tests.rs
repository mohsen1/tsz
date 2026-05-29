//! Tests for the rule: a *fresh* array/tuple literal written with `as const`
//! drops its `readonly` modifier when its contextual/target type is a mutable
//! array/tuple (or a type parameter constrained to one). This matches tsc — a
//! freshly-constructed literal is not aliased, so its const-ness is not binding
//! at a mutable consumption site.
//!
//! Structural rule: when the assignability source is a fresh `as const` array
//! literal expression and the target is a mutable array/tuple (directly, or via
//! a type parameter whose constraint is one), the source's outer `readonly` is
//! peeled before the element-wise check. An aliased `readonly` value (a
//! variable), a `readonly` target, a readonly *array* source, or a readonly
//! *variadic* tuple source keep the modifier and are rejected as before.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.message_text)
        .collect()
}

// ---------------------------------------------------------------------------
// Accepted: fresh `as const` literal into a mutable target.
// ---------------------------------------------------------------------------

#[test]
fn fresh_const_array_to_mutable_array_var_init() {
    // `const b: number[] = [1, 2] as const;` — tsc accepts (fresh literal).
    assert!(
        codes("const b: number[] = [1, 2] as const;").is_empty(),
        "fresh `as const` array should drop readonly for a mutable array target"
    );
}

#[test]
fn fresh_const_array_to_mutable_tuple_var_init() {
    assert!(
        codes("const b: [number, number] = [1, 2] as const;").is_empty(),
        "fresh `as const` array should drop readonly for a mutable tuple target"
    );
}

#[test]
fn fresh_const_array_in_return_position() {
    assert!(
        codes("function g(): number[] { return [1, 2] as const; }").is_empty(),
        "fresh `as const` array in return position should drop readonly"
    );
}

#[test]
fn fresh_const_array_to_concrete_tuple_argument() {
    let src = r#"
declare function f(t: [number, number]): void;
f([1, 2] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "fresh `as const` array argument should drop readonly for a mutable tuple param"
    );
}

#[test]
fn fresh_const_array_to_mutable_array_argument() {
    let src = r#"
declare function f(t: number[]): void;
f([1, 2] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "fresh `as const` array argument should drop readonly for a mutable array param"
    );
}

// ---------------------------------------------------------------------------
// Accepted: generic variadic-tuple inference (the reported family).
// Type-parameter names are varied to prove the rule is structural, not keyed
// on a particular spelling.
// ---------------------------------------------------------------------------

#[test]
fn fresh_const_array_into_variadic_generic_tuple_param_t() {
    let src = r#"
declare function f<T extends readonly unknown[]>(t: [...T]): T;
const r = f([1, 2] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "fresh `as const` array should infer through `[...T]` without a readonly error"
    );
}

#[test]
fn fresh_const_array_into_variadic_generic_tuple_param_renamed() {
    // Same rule, different type-parameter name (`Elems`) — must still hold.
    let src = r#"
declare function f<Elems extends readonly unknown[]>(t: [...Elems]): Elems;
const r = f([1, 2] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "rule must not depend on the type-parameter name"
    );
}

#[test]
fn fresh_const_array_into_mutable_constrained_bare_type_param() {
    // Constraint is mutable `unknown[]`, so readonly is dropped.
    let src = r#"
declare function f<K extends unknown[]>(t: K): K;
const r = f([1, 2] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "mutable-constrained bare type param should drop the source readonly"
    );
}

#[test]
fn fresh_const_arrays_into_two_variadic_type_params() {
    // `concat([1, 2] as const, ['a', 'b'] as const)` — both arguments are fresh.
    let src = r#"
declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(
  t: [...T],
  u: [...U],
): [...T, ...U];
const r = concat([1, 2] as const, ['a', 'b'] as const);
"#;
    assert!(
        codes(src).is_empty(),
        "both fresh `as const` arguments should flow into their `[...T]`/`[...U]` sites"
    );
}

// ---------------------------------------------------------------------------
// Still rejected: the readonly modifier is binding.
// ---------------------------------------------------------------------------

#[test]
fn aliased_readonly_tuple_variable_still_rejected() {
    // A variable is not fresh, so readonly stays binding (TS2345).
    let src = r#"
declare function f(t: [number, number]): void;
const a = [1, 2] as const;
f(a);
"#;
    assert!(
        codes(src).contains(&2345),
        "aliased readonly tuple must still be rejected against a mutable param"
    );
}

#[test]
fn readonly_array_source_still_rejected_for_variadic_generic() {
    // A readonly *array* (unbounded) is not a fresh fixed literal: rejected.
    let src = r#"
declare function f<T extends readonly unknown[]>(t: [...T]): T;
declare const arr: readonly number[];
const r = f(arr);
"#;
    assert!(
        codes(src).contains(&2345),
        "readonly array source must still be rejected against `[...T]`"
    );
}

#[test]
fn readonly_contextual_target_keeps_modifier() {
    // A readonly target does not request mutability, so no peeling occurs and
    // the (already readonly) source is accepted as-is — no diagnostic.
    assert!(
        codes("const b: readonly number[] = [1, 2] as const;").is_empty(),
        "readonly target accepts the readonly source without peeling"
    );
}

#[test]
fn element_mismatch_still_reported_after_dropping_readonly() {
    // After dropping readonly, the element-wise check still runs: `number` is not
    // assignable to `string`. tsc reports TS2322 on the elements, never TS4104.
    let src = "const b: string[] = [1, 2] as const;";
    let cs = codes(src);
    assert!(
        cs.contains(&2322),
        "element mismatch must still be reported (TS2322), got: {cs:?}"
    );
    assert!(
        !cs.contains(&4104),
        "must not report the readonly-to-mutable error (TS4104) for a fresh literal, got: {cs:?}"
    );
}

#[test]
fn readonly_constrained_type_param_keeps_modifier_in_inferred_type() {
    // `T extends readonly unknown[]` does not demand mutability, so the fresh
    // literal stays `readonly [1, 2]` and the inferred `T` is revealed as such
    // (guards the type-parameter-constraint path against over-stripping).
    let src = r#"
declare function f<T extends readonly unknown[]>(t: T): T;
const r = f([1, 2] as const);
const bad: null = r;
"#;
    assert!(
        messages(src).iter().any(|m| m.contains("readonly [1, 2]")),
        "readonly-constrained type param must keep the readonly modifier, got: {:?}",
        messages(src)
    );
}

#[test]
fn plain_const_array_without_mutable_context_stays_readonly() {
    // No mutable contextual type: the `as const` value remains `readonly [1, 2]`
    // and is rejected when later assigned to a mutable tuple via a variable.
    let src = r#"
const a = [1, 2] as const;
const b: [number, number] = a;
"#;
    assert!(
        !codes(src).is_empty(),
        "without a mutable contextual type the literal stays readonly"
    );
}
