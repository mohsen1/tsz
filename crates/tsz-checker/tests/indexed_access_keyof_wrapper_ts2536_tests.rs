//! Regression coverage for issue #14528: indexing a type parameter `T` by a key
//! parameter whose constraint is `keyof Wrapper<T>` must not draw a spurious
//! `TS2536` when the wrapper is *key-preserving* (`type Alias<T> = T`,
//! `NonNullable<T>`, `Readonly<T>`, `Partial<T>`).
//!
//! Structural rule: `T[K]` where `K extends keyof F<T>` is a valid index when
//! the transformed key space `keyof F<T>` is assignable to `keyof T`. The
//! structural "the index mentions a transformed `T`" heuristic cannot tell a
//! key-preserving transform from a key-changing one, so the `TS2536` it raises
//! is gated on the relation-backed `transformed_index_key_space_indexes_object`
//! query. Only a key-*changing* transform (a key remap, or a *foreign* type
//! parameter's keys such as `keyof U` with `U extends T`) yields keys outside
//! `keyof T` and must keep erroring.
//!
//! Verified against `tsc` 6.x: the positive cases are clean, the negative
//! controls report `TS2536`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_codes};

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

fn count(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|&c| c == code).count()
}

#[test]
fn transparent_alias_wrapper_keyof_constraint_no_ts2536() {
    let src = "type Alias<T> = T;\n\
               declare function get<T, K extends keyof Alias<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "keyof of a transparent alias preserves keyof T: {:?}",
        codes(src)
    );
}

#[test]
fn nonnullable_wrapper_keyof_constraint_no_ts2536() {
    let src = "declare function get<T, K extends keyof NonNullable<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "keyof NonNullable<T> indexes T like keyof T: {:?}",
        codes(src)
    );
}

#[test]
fn readonly_and_partial_wrapper_keyof_constraint_no_ts2536() {
    let src = "declare function r<T, K extends keyof Readonly<T>>(o: T, k: K): T[K];\n\
               declare function p<T, K extends keyof Partial<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "homomorphic mapped wrappers preserve keyof T: {:?}",
        codes(src)
    );
}

#[test]
fn renamed_binders_keyof_wrapper_constraint_no_ts2536() {
    // No dependence on the chosen identifier names.
    let src = "type Wrap<Q> = Q;\n\
               declare function getW<Obj, Key extends keyof Wrap<Obj>>(o: Obj, k: Key): Obj[Key];";
    assert_eq!(
        count(src, 2536),
        0,
        "renamed alias/binders behave identically: {:?}",
        codes(src)
    );
}

#[test]
fn keyof_wrapper_constraint_indexes_in_function_body_no_ts2536() {
    // Expression / computation path (`o[k]` in a body) must also accept it.
    let src = "type Alias<T> = T;\n\
               function read<T, K extends keyof Alias<T>>(o: T, k: K) { return o[k]; }";
    assert_eq!(
        count(src, 2536),
        0,
        "expression-path indexing of a key-preserving wrapper is valid: {:?}",
        codes(src)
    );
}

#[test]
fn foreign_type_parameter_keys_still_report_ts2536() {
    // `keyof U` with `U extends T`: keyof U may be wider than keyof T, so it is
    // not assignable to keyof T and must keep erroring.
    let src = "declare function bad<T, U extends T, K extends keyof U>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        1,
        "foreign type-parameter keys are not a valid index for T: {:?}",
        codes(src)
    );
}

#[test]
fn key_remapping_wrapper_still_reports_ts2536() {
    // A key remap produces keys outside `keyof T`, so the index is invalid.
    let src = "type Remap<T> = { [P in keyof T as `get_${string & P}`]: T[P] };\n\
               declare function bad<T, K extends keyof Remap<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        1,
        "a key-changing transform must keep erroring: {:?}",
        codes(src)
    );
}

#[test]
fn foreign_type_parameter_keys_body_indexing_still_reports_ts2536() {
    // Expression-path negative control: the gate must not over-suppress. `keyof U`
    // (U extends T) is not assignable to keyof T, so `o[k]` keeps erroring.
    let src = "function bad<T, U extends T, K extends keyof U>(o: T, k: K) { return o[k]; }";
    assert_eq!(
        count(src, 2536),
        1,
        "expression-path foreign type-parameter keys must keep erroring: {:?}",
        codes(src)
    );
}
