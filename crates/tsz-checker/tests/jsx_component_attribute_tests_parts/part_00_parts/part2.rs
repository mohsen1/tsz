#[test]
fn test_ts2698_not_emitted_for_any_spread() {
    // Spreading `any` in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
declare var a: any;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for any spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_type_param_extends_any_emits_error() {
    // Spreading a type parameter constrained to `any` should emit TS2698.
    // tsc internally rewrites `T extends any` to `T extends unknown`, and
    // spreading `unknown` is rejected (TS2698).
    //
    // Conformance regression source:
    // `TypeScript/tests/cases/compiler/jsxExcessPropsAndAssignability.tsx`.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
function f<T extends any>(x: T) {
    const e = <div { ...x } />;
    return e;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading T extends any, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_type_param_extends_unknown_emits_error() {
    // `T extends unknown` is the post-normalization form of `T extends any`,
    // and tsc rejects spreading either with TS2698.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
function f<T extends unknown>(x: T) {
    const e = <div { ...x } />;
    return e;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading T extends unknown, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_unconstrained_type_param_spread() {
    // tsc treats unconstrained type parameters as instantiable-non-primitive,
    // which is a valid spread source. No TS2698 for `function f<T>(x: T)`.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
function f<T>(x: T) {
    const e = <div { ...x } />;
    return e;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for unconstrained T spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_works_with_intrinsic_any_props() {
    // TS2698 should fire even when IntrinsicElements has [key: string]: any
    // (i.e., when skip_prop_checks would be true). The spread type validation
    // is independent of the props type.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const b = null;
const c = undefined;
const d = <div { ...b } />;
const e = <div { ...c } />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2698_count = count_code(
        &diags,
        diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
    );
    assert!(
        ts2698_count >= 2,
        "Expected at least 2 TS2698 errors (for null and undefined spreads), got {ts2698_count}: {diags:?}"
    );
}

#[test]
fn test_intrinsic_jsx_element_returns_jsx_element_type() {
    // JSX intrinsic elements (e.g., <div/>) should have type JSX.Element,
    // not IntrinsicElements["div"]. A function returning <div/> should be
    // assignable to () => JSX.Element.
    let source = r#"
declare namespace JSX {
    interface Element { _brand: "element" }
    interface IntrinsicElements {
        div: { className?: string };
        button: { onClick?: () => void };
    }
}
const f = () => <button>test</button>;
const x: () => JSX.Element = f;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Intrinsic JSX element should have type JSX.Element, got: {diags:?}"
    );
}
