//! Contextual-return typing must not clamp a generic rest-tuple parameter's
//! inferred arity.
//!
//! Structural rule: when a generic call `f<T extends any[]>(...rest: T): T` (the
//! return type is the bare rest-tuple type parameter) is checked against a
//! contextual tuple target, `tsc` infers `T`'s arity from the *arguments* and
//! only takes element hints — never arity — from the contextual type. `tsz` used
//! to substitute the contextual tuple into the returned bare rest parameter
//! whenever the arguments "fit" its element type, which discards tuple positions:
//! two `number` arguments spuriously fit a contextual `...rest: [number]`, so the
//! argument-inferred `[number, number]` was clamped down to `[number]` and the
//! real `TS2322` arity mismatch was hidden. The fix keeps the argument-inferred
//! tuple authoritative for direct value arguments (previously only spread
//! arguments were handled), while still letting the contextual type preserve
//! element literals (`tup(1, 2)` against `[1, 2]` stays `[1, 2]`).

use crate::test_utils::check_source_codes as codes;

#[test]
fn shorter_contextual_tuple_target_reports_arity_mismatch() {
    // Two arguments infer `[number, number]`; the shorter contextual `[number]`
    // must not clamp the arity away.
    assert_eq!(
        codes("declare function f<T extends any[]>(...a: T): T; const x: [number] = f(1, 2);"),
        vec![2322],
    );
}

#[test]
fn longer_contextual_tuple_target_reports_arity_mismatch() {
    assert_eq!(
        codes(
            "declare function f<T extends any[]>(...a: T): T; \
             const x: [number, number, number] = f(1, 2);"
        ),
        vec![2322],
    );
}

#[test]
fn single_argument_against_two_element_target_reports_arity_mismatch() {
    assert_eq!(
        codes("declare function f<T extends any[]>(...a: T): T; const x: [number, number] = f(1);"),
        vec![2322],
    );
}

#[test]
fn fixed_prefix_then_rest_tuple_reports_arity_mismatch() {
    // A leading fixed parameter before the rest tuple must not disturb the
    // arity-from-arguments rule. Renamed binders (`Head`/`Elems`).
    assert_eq!(
        codes(
            "declare function f<Elems extends any[]>(head: string, ...rest: Elems): Elems; \
             const x: [number] = f(\"h\", 1, 2);"
        ),
        vec![2322],
    );
}

#[test]
fn readonly_rest_tuple_reports_arity_mismatch() {
    assert_eq!(
        codes(
            "declare function f<T extends readonly any[]>(...a: T): T; \
             const x: readonly [number] = f(1, 2);"
        ),
        vec![2322],
    );
}

#[test]
fn rest_tuple_return_position_reports_arity_mismatch() {
    assert_eq!(
        codes(
            "declare function f<T extends any[]>(...a: T): T; \
             function g(): [number] { return f(1, 2); }"
        ),
        vec![2322],
    );
}

#[test]
fn rest_tuple_property_target_reports_arity_mismatch() {
    // Renamed binder (`Args`) and a distinct property name (`slot`) so the rule
    // is not keyed on any identifier.
    assert_eq!(
        codes(
            "declare function collect<Args extends any[]>(...a: Args): Args; \
             interface Box { slot: [number]; } const b: Box = { slot: collect(1, 2) };"
        ),
        vec![2322],
    );
}

// ---- Element-literal preservation must survive (matches tsc: clean) ----

#[test]
fn matching_literal_tuple_target_stays_clean() {
    // The contextual `[1, 2]` preserves the element literals through the
    // argument inference; the arity already matches, so no error.
    assert!(
        codes("declare function tup<T extends any[]>(...a: T): T; const x: [1, 2] = tup(1, 2);")
            .is_empty()
    );
}

#[test]
fn const_rest_tuple_matching_literal_target_stays_clean() {
    assert!(
        codes(
            "declare function tup<const T extends any[]>(...a: T): T; \
             const x: [1, 2] = tup(1, 2);"
        )
        .is_empty()
    );
}

#[test]
fn exact_arity_tuple_target_stays_clean() {
    assert!(
        codes(
            "declare function f<T extends any[]>(...a: T): T; const x: [number, number] = f(1, 2);"
        )
        .is_empty()
    );
}

#[test]
fn array_target_stays_clean() {
    assert!(
        codes("declare function f<T extends any[]>(...a: T): T; const x: number[] = f(1, 2);")
            .is_empty()
    );
}

// ---- Negative controls (unrelated shapes stay clean/unchanged) ----

#[test]
fn element_type_mismatch_still_reported() {
    // A genuine element-type mismatch surfaces regardless of the arity rule.
    assert_eq!(
        codes(
            "declare function f<T extends any[]>(...a: T): T; const x: [string, string] = f(1, 2);"
        ),
        vec![2322],
    );
}

#[test]
fn non_rest_wrapped_generic_return_preserves_literal() {
    // The contextual-return override still applies to non-rest wrapped returns:
    // `wrap("x")` is returned as `Wrap<"x">`, not widened.
    assert!(
        codes(
            "interface Wrap<T> { value: T; } declare function wrap<T>(x: T): Wrap<T>; \
             const w: Wrap<\"x\"> = wrap(\"x\");"
        )
        .is_empty()
    );
}

#[test]
fn bare_type_param_return_still_preserves_literal_upper_bound() {
    // `identity<T>(x: T): T` with a literal-union contextual target keeps the
    // contextual upper bound so the literal is not widened.
    assert!(
        codes("declare function identity<T>(x: T): T; const v: \"a\" | \"b\" = identity(\"a\");")
            .is_empty()
    );
}
