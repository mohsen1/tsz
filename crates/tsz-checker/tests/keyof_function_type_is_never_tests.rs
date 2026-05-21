//! End-to-end checker coverage for issue #9721: `keyof <function type>` must
//! normalize to `never` so `[K] extends [never]` reasoning matches tsc.
//!
//! Bare function and constructor types have no own properties and no index
//! signatures, so their key space is empty and `keyof` must collapse to
//! `never`.  The solver-internal asymmetry between the two lowering forms is
//! covered by the solver-side unit tests; this file pins the user-observable
//! behaviour at the checker boundary.

use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_source_diagnostics;

fn count_ts2322(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count()
}

#[test]
fn keyof_bare_function_type_extends_never_picks_true_branch() {
    // The exact repro from issue #9721.
    let source = r#"
        type Fn = () => void;
        type K = keyof Fn;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "notnever";
    "#;

    assert!(
        count_ts2322(source) >= 1,
        "keyof (() => void) should normalize to never, so T1 should be \"isnever\" \
         and the assignment of \"notnever\" should fail with TS2322"
    );
}

#[test]
fn keyof_bare_function_type_assigning_isnever_is_ok() {
    // Positive control: the matching literal must NOT report TS2322.
    let source = r#"
        type Fn = () => void;
        type K = keyof Fn;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "isnever";
    "#;

    assert_eq!(
        count_ts2322(source),
        0,
        "Assigning \"isnever\" must succeed when keyof Fn === never",
    );
}

#[test]
fn keyof_function_with_params_extends_never_picks_true_branch() {
    // Renamed alias + different parameter shape proves the rule is structural,
    // not tied to the empty-parameter spelling.
    let source = r#"
        type Handler = (x: number, y: string) => boolean;
        type K = keyof Handler;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "notnever";
    "#;

    assert!(
        count_ts2322(source) >= 1,
        "keyof (x: number, y: string) => boolean must still normalize to never",
    );
}

#[test]
fn keyof_generic_function_type_extends_never_picks_true_branch() {
    // Generic call-signature-only types stay function-shaped after lowering.
    let source = r#"
        type Identity = <U>(u: U) => U;
        type K = keyof Identity;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "notnever";
    "#;

    assert!(
        count_ts2322(source) >= 1,
        "keyof of a generic call-signature type must normalize to never",
    );
}

#[test]
fn keyof_constructor_type_extends_never_picks_true_branch() {
    // Positive control from the issue's boundary table.  Constructor-only
    // types already lower to a `Callable` whose empty key-space resolves to
    // never; pin it down so it stays symmetric with the function-type path.
    let source = r#"
        type Ctor = new () => object;
        type K = keyof Ctor;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "notnever";
    "#;

    assert!(
        count_ts2322(source) >= 1,
        "keyof (new () => object) must normalize to never",
    );
}

#[test]
fn keyof_callable_with_explicit_property_keeps_property_keys() {
    // Negative control: a callable that *does* have a declared property must
    // still keyof to that property — the fix must not over-reach into the
    // Callable path.
    let source = r#"
        type C = { (): void; prop: number };
        type K = keyof C;
        type T1 = [K] extends [never] ? "isnever" : "notnever";
        const r: T1 = "notnever";
    "#;

    assert_eq!(
        count_ts2322(source),
        0,
        "keyof {{ (): void; prop: number }} is \"prop\", which is NOT never, \
         so the false branch is taken and the assignment of \"notnever\" succeeds",
    );
}

#[test]
fn keyof_function_in_equal_harness_matches_never() {
    // Standard `Equal<…, never>` shape used by ts-essentials / ts-toolbelt.
    // Before the fix this would report TS2344 because keyof Fn stayed
    // deferred and the harness reduced to `false` instead of `true`.
    let source = r#"
        type Equal<X, Y> =
            (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2)
                ? true
                : false;
        type Expect<T extends true> = T;
        type Fn = () => void;
        type t = Expect<Equal<keyof Fn, never>>;
    "#;

    let errors = check_source_diagnostics(source);
    let blocking: Vec<_> = errors
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT
                || d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
        .collect();
    assert!(
        blocking.is_empty(),
        "Equal<keyof Fn, never> must resolve to true, got blocking diagnostics: {blocking:?}",
    );
}
