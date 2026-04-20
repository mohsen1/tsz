#[test]
fn test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    ) else {
        return;
    };

    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, &source, JsxMode::Preserve);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "real react16 fixture should not emit TS7006, got: {diags:?}"
    );
}

#[test]
fn test_generic_props_alias_call_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

Test({{
    as: "bar",
    callback: value => value.toUpperCase(),
}});
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic props alias call should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through the call path, got: {diags:?}"
    );
}

#[test]
fn test_concrete_props_still_emit_excess_errors() {
    // When props type is fully concrete (no type parameters), excess property
    // checking should still work.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SFC<P> {{
    (props: P & {{ children?: string }}): JSX.Element;
}}
function test(Component: SFC<{{ x: number }}>) {{
    return <Component x={{1}} y={{"blah"}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should emit TS2322 for excess property 'y' on concrete props, got: {diags:?}"
    );
}

// =============================================================================
// TS2783: JSX spread overwrite detection
// =============================================================================

#[test]
fn test_spread_overwrite_skips_type_check() {
    // When a later spread will overwrite an explicit attribute, tsc only
    // emits TS2783 (overwrite warning) and does NOT emit TS2322 (type mismatch).
    // This tests the ordering: overwrite detection before type checking.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    x: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ x: 1 }};
let t = <Foo x={{"not a number"}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // TS2783 should be emitted (spread overwrites 'x')
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Expected TS2783 for overwritten attribute, got: {diags:?}"
    );
    // TS2322 should NOT be emitted (type check skipped because overwritten)
    let ts2322_for_x = diags.iter().any(|(c, msg)| {
        *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE && msg.contains("string")
    });
    assert!(
        !ts2322_for_x,
        "Should NOT emit TS2322 for overwritten attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_jsx_spread_overwrites_explicit_attribute() {
    // When a required property in a spread follows an explicit attribute with
    // the same name, TS2783 should be emitted on the explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo a={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should emit TS2783 when spread overwrites explicit attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_not_emitted_for_optional_spread_property() {
    // When the spread property is optional, the explicit attribute may NOT be
    // overwritten at runtime, so TS2783 should NOT be emitted.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo d={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should NOT emit TS2783 when spread has optional property, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_multiple_spreads_track_required_only() {
    // First spread has optional `d`, so no TS2783. Second spread has required
    // `d`, so TS2783 fires for the original explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1 }};
let x = <Foo a={{1}} d={{1}} {{...p}} {{...{{ d: 1 }}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2783_count = diags
        .iter()
        .filter(|(c, _)| {
            *c == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    // 'a' overwritten by first spread (required in Props), 'd' overwritten by second spread
    assert!(
        ts2783_count >= 2,
        "Should emit TS2783 for both 'a' (required in first spread) and 'd' (required in second spread), got {ts2783_count} TS2783 errors: {diags:?}"
    );
}

#[test]
fn test_intrinsic_later_inferred_spread_emits_ts2783_and_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string; y?: number; z?: string };
    }
}

var obj5 = { x: 32, y: 32 };
<test1 x="ok" {...obj5} />;

var obj7 = { x: 'foo' };
<test1 x={32} {...obj7} />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2783_count = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    assert!(
        ts2783_count == 2,
        "Later inferred spreads should emit TS2783 for each overwritten explicit attr, got: {diags:?}"
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Later inferred spreads should still report the spread-side TS2322 mismatch, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_generic_type_parameter_constraint_spread() {
    // When a generic component spreads a type parameter `T extends { x: number }`,
    // the required property `x` from the constraint should trigger TS2783 if `x`
    // was also specified as an explicit attribute before the spread.
    // This matches tsc behavior for tsxGenericAttributesType1.tsx.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp<T extends {{ x: number }}>(props: T) {{ return <div />; }}
function wrapper<T extends {{ x: number }}>(Component: (props: T) => JSX.Element) {{
    return (props: T) => <Component x={{2}} {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should emit TS2783 when generic spread overwrites explicit attribute via constraint, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_not_emitted_for_generic_without_constraint() {
    // When a generic component spreads a type parameter without a constraint,
    // no TS2783 should be emitted since we don't know the properties.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp<T>(props: T) {{ return <div />; }}
function wrapper<T>(Component: (props: T) => JSX.Element) {{
    return (props: T) => <Component x={{2}} {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should NOT emit TS2783 for unconstrained type parameter spread, got: {diags:?}"
    );
}

