//! A function-like's own type parameters and the top-level local type
//! declarations (`interface`/`type`/`class`/`enum`) in its body share one
//! declaration space, per `TypeScript/tests/cases/conformance/types/localTypes/localTypes4.ts`
//! (f3): `function f<T>() { interface T {} }` reports `TS2300` on both `T`s.
//!
//! Every expectation here is pinned against `typescript@7.0.2`.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut found: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|diag| diag.code)
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn type_parameter_collides_with_top_level_local_interface() {
    let source = "function f<T>() { interface T {} }";
    assert_eq!(codes(source), vec![2300, 2300]);
}

#[test]
fn type_parameter_collides_with_top_level_local_type_alias() {
    // Also reports TS2454 ("used before being assigned") for `v` — real tsc
    // behavior, unrelated to the duplicate-identifier collision under test.
    let source = "function f<T>() { type T = string; let v: T; v; }";
    assert_eq!(codes(source), vec![2300, 2300, 2454]);
}

#[test]
fn type_parameter_collides_with_top_level_local_class() {
    let source = "function f<T>() { class T {} }";
    assert_eq!(codes(source), vec![2300, 2300]);
}

#[test]
fn type_parameter_collides_with_top_level_local_enum() {
    let source = "function f<T>() { enum T { A } }";
    assert_eq!(codes(source), vec![2567, 2567]);
}

#[test]
fn arrow_function_type_parameter_collides_with_top_level_local_interface() {
    let source = "const f = <T,>() => { interface T {} };";
    assert_eq!(codes(source), vec![2300, 2300]);
}

#[test]
fn method_type_parameter_collides_with_top_level_local_interface() {
    let source = "class C { method<T>() { interface T {} } }";
    assert_eq!(codes(source), vec![2300, 2300]);
}

#[test]
fn value_declaration_does_not_collide_with_type_parameter() {
    // Type parameters occupy only the type declaration space: a same-named
    // `let`/parameter/function is a legal shadow, not a duplicate.
    let source = "function f<T>() { let T = 5; T; }";
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn parameter_does_not_collide_with_own_type_parameter() {
    let source = "function f<T>(T: number) { T; }";
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn function_declaration_does_not_collide_with_type_parameter() {
    let source = "function f<T>() { function T() {} }";
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn nested_block_local_interface_does_not_collide_with_outer_type_parameter() {
    // A genuinely nested block (`if`/`for`/a further-nested `{ }`) is its own
    // declaration space, distinct from the function's own top-level body.
    let source = "function f<T>() { if (true) { interface T {} } }";
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn inner_function_own_type_parameter_does_not_collide_with_outer() {
    let source = "function f<T>() { function g<T>() {} }";
    assert_eq!(codes(source), Vec::<u32>::new());
}

#[test]
fn nested_inside_outer_function_still_collides() {
    let source = "function f3() { function f<T>() { interface T {} return undefined; } }";
    assert_eq!(codes(source), vec![2300, 2300]);
}
