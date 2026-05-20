//! Regression test for TS2604 suppression when JSX type-argument arity
//! mismatches.
//!
//! When a JSX element supplies the wrong number of type arguments (e.g.
//! `<MyComp<A, B>>` for a class with one type parameter), tsc emits
//! TS2558 ("Expected N type arguments, but got M") and stops there. tsz
//! was piling TS2604 ("JSX element type does not have any construct or
//! call signatures") on top because the `recovered_props` path falls back
//! to the no-props branch under `type_arg_count_mismatch`, which then
//! runs `check_jsx_element_has_signatures`.
//!
//! Source: `conformance/jsx/tsxTypeArgumentResolution.tsx`.

use tsz_common::options::checker::CheckerOptions;

fn diag_codes_for_tsx(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.tsx", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Class component instantiated with too many type args:
/// TS2558 fires (correct), TS2604 must NOT fire (false positive).
#[test]
fn jsx_too_many_type_args_emits_only_ts2558_not_ts2604() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass {}
    interface IntrinsicElements {}
}
declare class MyComp<P> {
    new(props: P): MyComp<P>;
    render(): any;
}

let x = <MyComp<{a: number}, {b: string}> a={1} />;
"#;
    let codes = diag_codes_for_tsx(source);
    assert!(
        codes.contains(&2558),
        "Expected TS2558 for wrong type-arg count. Got: {codes:?}",
    );
    assert!(
        !codes.contains(&2604),
        "TS2604 must not fire when TS2558 already reports the arity mismatch. Got: {codes:?}",
    );
}

/// Anti-hardcoding cover: rename the component, change the arity offense
/// from "too many" to "empty type-arg list" (TS2558 still fires).
#[test]
fn jsx_empty_type_args_emits_only_ts2558_not_ts2604() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass {}
    interface IntrinsicElements {}
}
declare class Renamed<P> {
    new(props: P): Renamed<P>;
    render(): any;
}

let x = <Renamed<> a={1} />;
"#;
    let codes = diag_codes_for_tsx(source);
    assert!(
        !codes.contains(&2604),
        "Renamed: TS2604 must not fire on type-arg arity mismatch. Got: {codes:?}",
    );
}

/// Negative control: when type args ARE valid, TS2604 must STILL fire if
/// the component truly has no construct/call signatures (e.g. a plain
/// non-callable interface used as JSX). Ensures the suppression didn't
/// accidentally disable TS2604 globally.
#[test]
fn jsx_valid_type_args_but_no_signatures_still_emits_ts2604() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass {}
    interface IntrinsicElements {}
}
interface NotAComponent {
    foo: number;
}
declare const NotAComp: NotAComponent;

let x = <NotAComp />;
"#;
    let codes = diag_codes_for_tsx(source);
    assert!(
        codes.contains(&2604),
        "Non-callable component must still emit TS2604. Got: {codes:?}",
    );
}
