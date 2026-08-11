//! Regression coverage for issue #14528: indexing a type parameter `T` by a key
//! parameter whose constraint is `keyof Wrapper<T>` must not draw a spurious
//! `TS2536` when the wrapper's `keyof` stays a *deferred generic mapped index*.
//!
//! Structural rule (mirrors tsc's `checkIndexedAccessIndexType` →
//! `getIndexTypeForMappedType`): `T[K]` where `K extends keyof S` is a valid
//! index when `keyof S` is a deferred mapped index — `S` is a mapped type over a
//! generic `keyof` (`{ [P in keyof <generic> as? N]: ... }`), whether the mapped
//! source is `T` itself or a foreign type parameter. tsc resolves such a `keyof`
//! to a deferred index (`getIndexTypeForGenericType`) that its relation worker
//! treats as assignable to `keyof T`, so **even a key-remapping `as` clause
//! stays valid** — the remapped keys (`` `N` ``) never leave the deferred index
//! and are never compared structurally against `keyof T`. This subsumes the
//! key-preserving wrappers (`type Alias<T> = T`, `NonNullable<T>`,
//! `Readonly<T>`, `Partial<T>`) and the key-remapping ones alike.
//!
//! Only these still report `TS2536`, because their `keyof` resolves to a
//! concrete or non-mapped key space that must relate structurally to `keyof T`:
//! a *bare foreign* type parameter's keys (`keyof U` with `U extends T`), a
//! *conditional* wrapper (`keyof (T extends … ? … : …)`), a *constant*-key
//! mapped type (`[P in "a" | "b"]`), and a *concrete-object* keyof mapped type
//! (`[P in keyof { a: 1 }]`).
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
fn key_remapping_wrapper_over_object_param_no_ts2536() {
    // A key-*remapping* mapped wrapper (`as` clause) over the object type
    // parameter is still a valid index: `keyof Remap<T>` is a deferred mapped
    // index assignable to `keyof T` (verified vs tsc 6.x: clean). The remapped
    // `` `get_${…}` `` keys never leave the deferred index.
    let src = "type Remap<T> = { [P in keyof T as `get_${string & P}`]: T[P] };\n\
               declare function ok<T, K extends keyof Remap<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "keyof of a generic key-remapping mapped type indexes T: {:?}",
        codes(src)
    );
}

#[test]
fn key_remapping_wrapper_over_constrained_foreign_param_no_ts2536() {
    // The deferred mapped index is valid even when the mapped type is over a
    // *foreign* type parameter (`U extends T`): `keyof Remap<U>` still indexes
    // `T` in tsc.
    let src = "type Remap<Q> = { [P in keyof Q as `get_${string & P}`]: Q[P] };\n\
               declare function ok<T, U extends T, K extends keyof Remap<U>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "keyof of a mapped type over a constrained foreign param indexes T: {:?}",
        codes(src)
    );
}

#[test]
fn key_remapping_wrapper_over_unrelated_param_no_ts2536() {
    // ...and even over a *wholly unrelated* type parameter: tsc keeps the
    // deferred mapped index assignable to `keyof T` regardless of the mapped
    // source, so `T[K]` with `K extends keyof Wrap<U>` is accepted.
    let src = "type Wrap<Q> = { [P in keyof Q as `get_${string & P}`]: Q[P] };\n\
               declare function ok<T, U, K extends keyof Wrap<U>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "keyof of a mapped type over an unrelated generic param indexes T: {:?}",
        codes(src)
    );
}

#[test]
fn key_remapping_wrapper_indexes_in_function_body_no_ts2536() {
    // Expression / computation path (`o[k]` in a body) accepts the remap too.
    let src = "type Remap<T> = { [P in keyof T as `get_${string & P}`]: T[P] };\n\
               function read<T, K extends keyof Remap<T>>(o: T, k: K) { return o[k]; }";
    assert_eq!(
        count(src, 2536),
        0,
        "expression-path indexing of a key-remapping wrapper is valid: {:?}",
        codes(src)
    );
}

#[test]
fn pick_and_omit_wrapper_keyof_constraint_no_ts2536() {
    // `Pick`/`Omit` mapped wrappers over a generic `keyof` keep the index valid:
    // `keyof Pick<T, keyof T>` and `keyof Omit<T, "x">` both index `T`.
    let src = "declare function pk<T, K extends keyof Pick<T, keyof T>>(o: T, k: K): T[K];\n\
               declare function om<T, K extends keyof Omit<T, \"x\">>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        0,
        "Pick/Omit over a generic keyof index T: {:?}",
        codes(src)
    );
}

#[test]
fn constant_key_mapped_wrapper_still_reports_ts2536() {
    // A mapped wrapper whose key source is a *constant* (`[P in "zzz"]`) is not a
    // deferred generic mapped index: its keys are concrete and unrelated to
    // `keyof T`, so tsc keeps reporting TS2536.
    let src = "type Const<T> = { [P in \"zzz\"]: T };\n\
               declare function bad<T, K extends keyof Const<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        1,
        "a constant-key mapped wrapper must keep erroring: {:?}",
        codes(src)
    );
}

#[test]
fn concrete_object_keyof_mapped_wrapper_still_reports_ts2536() {
    // A mapped wrapper over the `keyof` of a *concrete* object
    // (`[P in keyof { a: 1; b: 2 }]`) resolves to a concrete key union not
    // assignable to `keyof T`, so it keeps erroring — the generic-keyof
    // discriminator excludes it.
    let src = "type FromObj<T> = { [P in keyof { a: 1; b: 2 }]: T };\n\
               declare function bad<T, K extends keyof FromObj<T>>(o: T, k: K): T[K];";
    assert_eq!(
        count(src, 2536),
        1,
        "a concrete-object-keyof mapped wrapper must keep erroring: {:?}",
        codes(src)
    );
}

#[test]
fn direct_keyof_intersection_index_still_reports_ts2536() {
    // tsc distinguishes a *type-parameter* index `K extends keyof (T & {})`
    // (allowed) from a *direct* keyof value `k: keyof (T & {})` (still TS2536),
    // even though both reduce to `keyof T`. Mirrors
    // `conformance/types/unknown/unknownControlFlow.ts` `ff3`. The suppression
    // must not extend to the direct-value form.
    let src = "function ff3<T>(t: T, k: keyof (T & {})) { t[k]; }";
    assert_eq!(
        count(src, 2536),
        1,
        "a direct transformed-keyof value index must keep erroring: {:?}",
        codes(src)
    );
}

#[test]
fn direct_keyof_of_own_object_index_no_ts2536() {
    // Control: indexing by the object's *own* `keyof T` (not a transform) is
    // valid in tsc (`unknownControlFlow.ts` `ff1`), and unaffected.
    let src = "function ff1<T>(t: T, k: keyof T) { t[k]; }";
    assert_eq!(
        count(src, 2536),
        0,
        "indexing by the object's own keyof is valid: {:?}",
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
