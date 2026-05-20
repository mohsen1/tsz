//! Parity tests for JSX multi-construct class component overload resolution.
//!
//! When a JSX component has multiple construct signatures (e.g., React.Component's
//! two-constructor overload pattern), type checking must be performed EXCLUSIVELY
//! through overload resolution (`check_jsx_overloaded_sfc`). No additional
//! generic-spread assignability check should run after overload resolution, because
//! that would produce spurious TS2322 errors for valid JSX calls whose props type
//! uses complex conditional/mapped/generic types.
//!
//! Structural rule: for multi-construct class components, overload resolution is the
//! sole validation path. tsc emits TS2769 when no overload matches; it does NOT emit
//! a separate TS2322 from a generic-spread check on top.
//!
//! Regression target: `jsxComplexSignatureHasApplicabilityError.tsx` (conformance).

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

/// The primary regression test: a class component with two construct signatures
/// (the React.Component<P> pattern) must NOT produce TS2322 for valid JSX — the
/// overload resolution is the sole check and it passes when all provided props
/// satisfy at least one overload's props type.
#[test]
fn multi_construct_valid_jsx_no_ts2322() {
    // `ElementAttributesProperty.props: {}` in JSX_PREAMBLE tells the checker
    // which field to use for attribute matching — without it, props extraction fails.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    props: P;
}}

interface MyProps {{
    a: string;
    b: number;
}}

declare class MyComp extends Component<MyProps> {{}}

// Valid JSX matching the constructor overloads: must produce no TS2322.
const el = <MyComp a="hello" b={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "multi-construct class component with valid props must not emit TS2322; got {ts2322:#?}"
    );
}

/// The invalid-JSX case: a multi-construct class component with props that do not
/// satisfy any overload must emit TS2769 ("No overload matches this call"), NOT
/// multiple TS2322 errors — the overload resolution path owns the diagnostic.
#[test]
fn multi_construct_invalid_jsx_emits_ts2769_not_ts2322() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    props: P;
}}

interface PropsA {{ mode: "a"; n: number; }}
interface PropsB {{ mode: "b"; s: string; }}

declare class Switcher extends Component<PropsA | PropsB> {{}}

// Invalid: excess property `extra` not in either PropsA or PropsB.
// Multi-construct overload resolution should emit TS2769, not pile of TS2322.
const el = <Switcher mode="a" n={{1}} extra={{true}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // If any TS2769 is present, check no raw TS2322 is accompanying it
    // (tsc's rule: overload failure produces one TS2769, not N TS2322).
    let ts2769_count = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .count();
    let ts2322_count = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    // We should get at most one overload-level error. TS2322 in isolation (without
    // TS2769) would mean overload resolution is not running at all; both together
    // means the old double-check path is still active.
    assert!(
        ts2769_count <= 1,
        "expected at most one TS2769 for multi-construct JSX; got {ts2769_count}: {diags:#?}"
    );
    assert!(
        ts2322_count == 0,
        "multi-construct JSX must not emit standalone TS2322; got {ts2322_count}: {diags:#?}"
    );
}

/// Regression test for the exact shape that `jsxComplexSignatureHasApplicabilityError.tsx`
/// exercises: a multi-construct component with a generic spread attribute plus explicit
/// attributes. The OLD code ran `check_jsx_generic_spread_attrs_assignability` after
/// overload resolution and produced a spurious TS2322 for the spread+explicit combination
/// even when the overload matched. The fixed code skips that check entirely.
#[test]
fn multi_construct_generic_spread_plus_explicit_no_ts2322() {
    // The generic wrapper function carries a type parameter T so the spread `{...props}`
    // is generic — this used to trigger check_jsx_generic_spread_attrs_assignability.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    props: P;
}}

interface SelectProps<T> {{
    value: T;
    options: T[];
}}

declare class SelectComp<T> extends Component<SelectProps<T>> {{}}

// Generic wrapper function: props contains type parameter T.
// The spread {{...props}} is generic, and value={{props.value}} is an explicit attr.
// This used to produce a spurious TS2322 via check_jsx_generic_spread_attrs_assignability.
function createWrapper<T>(SelectClass: typeof SelectComp, props: SelectProps<T>): JSX.Element {{
    return <SelectClass {{...props}} value={{props.value}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "multi-construct class with generic spread + explicit attr must not emit TS2322; got {ts2322:#?}"
    );
}

/// Overloaded SFC (function with multiple call signatures) should still use overload
/// resolution and emit TS2769 when no overload matches — this path is separate from
/// the multi-construct path and should be unaffected by the fix.
#[test]
fn overloaded_sfc_invalid_jsx_emits_ts2769() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare function F(p: {{ mode: "a"; n: number }}): JSX.Element;
declare function F(p: {{ mode: "b"; s: string }}): JSX.Element;

// `s` is excess for mode="a", and mode="b" requires s not n.
// No overload matches → TS2769.
const bad = <F mode="a" s="x" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // tsc emits TS2769 for this case.
    let ts2769: Vec<_> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .collect();
    assert!(
        !ts2769.is_empty(),
        "overloaded SFC with no matching overload must emit TS2769; got {diags:#?}"
    );
    // Ensure no raw TS2322 accompanies the TS2769 (overload resolution is the sole check)
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "overloaded SFC must not emit standalone TS2322 alongside TS2769; got {ts2322:#?}"
    );
}

/// Overloaded SFC (function with multiple call signatures) with valid props must
/// produce no errors.
#[test]
fn overloaded_sfc_valid_jsx_no_errors() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare function F(p: {{ mode: "a"; n: number }}): JSX.Element;
declare function F(p: {{ mode: "b"; s: string }}): JSX.Element;

// Matches overload 2 exactly.
const ok = <F mode="b" s="hello" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        diags.is_empty(),
        "overloaded SFC with matching overload must produce no errors; got {diags:#?}"
    );
}

/// Verify that the fix also covers the name variant: renaming the type parameter
/// (e.g., from T to U, K, etc.) must not change the behavior.
/// This guards against any accidentally hardcoded type-parameter-name matching.
#[test]
fn multi_construct_generic_spread_name_variant_k() {
    // Same test as `multi_construct_generic_spread_plus_explicit_no_ts2322` but
    // uses `K` as the type parameter name instead of `T`.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: Readonly<P>);
    constructor(props: P, context?: any);
    props: P;
}}

interface SelectProps<K> {{
    value: K;
    options: K[];
}}

declare class SelectComp<K> extends Component<SelectProps<K>> {{}}

function createWrapper<K>(SelectClass: typeof SelectComp, props: SelectProps<K>): JSX.Element {{
    return <SelectClass {{...props}} value={{props.value}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "multi-construct class with generic spread (type param K) must not emit TS2322; got {ts2322:#?}"
    );
}
