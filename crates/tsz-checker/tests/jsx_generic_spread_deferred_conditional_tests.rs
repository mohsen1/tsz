//! Parity tests for JSX generic-spread whole-object assignability when the
//! spread source (or an explicit attribute value) carries a conditional type
//! that is still deferred over a type parameter.
//!
//! Structural rule: when a JSX element spreads a generic value whose type is a
//! deferred conditional over a type parameter (`Omit`/`Pick`/`Exclude`/
//! `Overwrite`/a user conditional applied to an unresolved `T`), `tsc` treats
//! the source as an instantiable, *comparable* type and does not emit `TS2322`
//! from the whole-object check. The same holds for an explicit attribute whose
//! value type is itself a deferred conditional. `tsz`'s structural relation
//! cannot soundly decide such operands and used to conservatively report
//! "not assignable", producing a false positive.
//!
//! A concrete (non-deferred) attribute mismatch must still be reported, so the
//! suppression is scoped to deferred-conditional operands only.
//!
//! Regression target: `jsxComplexSignatureHasApplicabilityError.tsx`
//! (the react-select HOC corpus shape).

use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;

const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

const COMMON: &str = r#"
interface SelectProps<T> {
    multi?: boolean;
    value?: T;
    onChange?: (v: T | undefined) => void;
}
declare function Select<T>(props: SelectProps<T>): JSX.Element;

type Pull<T> = T extends SelectProps<infer U> ? U : never;
type Drop<T, K extends keyof any> = T extends any ? Pick<T, Exclude<keyof T, K>> : never;
type Merge<T, U> = Drop<T, keyof T & keyof U> & U;
type SingleProps<W extends SelectProps<any>> = Merge<Drop<W, "multi">, { value?: Pull<W> }>;
"#;

fn jsx_opts() -> CheckerOptions {
    CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..Default::default()
    }
}

fn jsx_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.tsx", jsx_opts())
        .into_iter()
        .map(|d| (d.code, d.start, d.message_text))
        .collect()
}

fn count_ts2322(source: &str) -> usize {
    jsx_diagnostics(source)
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count()
}

/// Primary regression: a generic component spread with a deferred-conditional
/// source (`SingleProps<W>` reducing to `Drop<W, "multi"> & ...`) plus an
/// explicit attribute whose value type is itself deferred (`value={props.value}`
/// of type `Pull<W>`). `tsc` accepts this; `tsz` must not emit `TS2322`.
#[test]
fn generic_spread_deferred_conditional_no_ts2322() {
    let source = format!(
        r#"{JSX_PREAMBLE}{COMMON}
function wrap<W extends SelectProps<any>>(props: SingleProps<W>) {{
    return <Select<Pull<W>> {{...props}} multi={{false}} value={{props.value}} />;
}}
"#
    );
    assert_eq!(
        count_ts2322(&source),
        0,
        "valid generic-spread with deferred-conditional source must not emit TS2322; got {:#?}",
        jsx_diagnostics(&source)
    );
}

/// The rule is structural, not name-based: renaming every type parameter and
/// helper alias must not change the outcome.
#[test]
fn generic_spread_deferred_conditional_renamed_no_ts2322() {
    // Same shape, every user-chosen identifier renamed.
    let source = format!(
        r#"{JSX_PREAMBLE}
interface PropsX<Q> {{
    multi?: boolean;
    value?: Q;
    onChange?: (v: Q | undefined) => void;
}}
declare function Widget<Q>(props: PropsX<Q>): JSX.Element;

type Grab<Z> = Z extends PropsX<infer U> ? U : never;
type Strip<Z, K extends keyof any> = Z extends any ? Pick<Z, Exclude<keyof Z, K>> : never;
type Combine<Z, U> = Strip<Z, keyof Z & keyof U> & U;
type OneProps<Z extends PropsX<any>> = Combine<Strip<Z, "multi">, {{ value?: Grab<Z> }}>;

function build<Z extends PropsX<any>>(props: OneProps<Z>) {{
    return <Widget<Grab<Z>> {{...props}} multi={{false}} value={{props.value}} />;
}}
"#
    );
    assert_eq!(
        count_ts2322(&source),
        0,
        "renamed generic-spread case must also be clean (rule is structural); got {:#?}",
        jsx_diagnostics(&source)
    );
}

/// Multi-construct class component variant of the same shape (the exact
/// conformance-witness routing): overload resolution is the sole check and a
/// valid deferred-conditional spread must produce no TS2322.
#[test]
fn multi_construct_generic_spread_deferred_conditional_no_ts2322() {
    let source = format!(
        r#"{JSX_PREAMBLE}{COMMON}
declare class Component<P> {{
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    props: Readonly<P>;
}}
declare class SelectClass<T = string> extends Component<SelectProps<T>> {{}}

function wrapClass<W extends SelectProps<any>>(props: SingleProps<W>) {{
    return <SelectClass<Pull<W>> {{...props}} multi={{false}} value={{props.value}} />;
}}
"#
    );
    assert_eq!(
        count_ts2322(&source),
        0,
        "multi-construct class component with deferred-conditional spread must not emit TS2322; got {:#?}",
        jsx_diagnostics(&source)
    );
}

/// Negative guard: a *concrete* (non-deferred) attribute mismatch must still be
/// reported. The deferred-conditional suppression must not hide real errors.
#[test]
fn generic_spread_concrete_attr_mismatch_still_reported() {
    let source = format!(
        r#"{JSX_PREAMBLE}{COMMON}
function wrap<W extends SelectProps<any>>(props: SingleProps<W>) {{
    // `multi` is concretely `boolean`; a string literal is a real mismatch.
    return <Select<Pull<W>> {{...props}} multi={{"definitely-not-boolean"}} value={{props.value}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let real_error = diags.iter().any(|(c, _, _)| {
        *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            || *c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
    });
    assert!(
        real_error,
        "a concrete `multi` mismatch must still be reported; got {diags:#?}"
    );
}
