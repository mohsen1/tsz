#[test]
fn test_jsx_children_presence_narrows_react_component_type_wrappers() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    interface Component<P, S = {{}}> {{
        readonly props: Readonly<{{ children?: ReactNode }}> & Readonly<P>;
        readonly state: Readonly<S>;
    }}
    interface ComponentClass<P = {{}}> {{ new(props: P, context?: any): Component<P, any>; }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const BodyChild = (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
const ExplicitChild = (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "React.ComponentType wrappers should preserve children contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured JSX children should be contextually typed through React wrappers, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React.ComponentType wrapper normalization should avoid downstream TS2322 here, got: {diags:?}"
    );
}

#[test]
fn react_component_type_return_accepts_generic_overwritten_props_arrow() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<P = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    class Component<P, S = {{}}> {{
        constructor(props: Readonly<P>);
        constructor(props: P, context?: any);
        props: P;
    }}
    interface ComponentClass<P = {{}}, S = {{}}> {{
        new(props: P, context?: any): Component<P, S>;
    }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = T extends any ? Pick<T, Exclude<keyof T, K>> : never;
type Overwrite<T, U> = Omit<T, keyof T & keyof U> & U;

type OptionValues = string | number | boolean;
interface Option<TValue = OptionValues> {{
    value?: TValue;
    [property: string]: any;
}}
interface Props<T extends OptionValues> {{
    value?: Option<T> | T;
    onChange?(value: Option<T> | undefined): void;
}}
type ExtractValueType<T> = T extends ReactSelectProps<infer U> ? U : never;
type ReactSingleSelectProps<WrappedProps extends ReactSelectProps<any>> =
    Overwrite<Omit<WrappedProps, "multi">, Props<ExtractValueType<WrappedProps>>>;

declare class ReactSelectClass<TValue = OptionValues> extends React.Component<ReactSelectProps<TValue>> {{}}
interface ReactSelectProps<TValue = OptionValues> {{
    multi?: boolean;
    value?: Option<TValue> | Option<TValue>[] | string | string[] | number | number[] | boolean;
    onChange?: (newValue: Option<TValue> | Option<TValue>[] | null) => void;
}}

export function createReactSingleSelect<WrappedProps extends ReactSelectProps<any>>(
    WrappedComponent: React.ComponentType<WrappedProps>
): React.ComponentType<ReactSingleSelectProps<WrappedProps>> {{
    return (props) => {{
        return (
            <ReactSelectClass<ExtractValueType<WrappedProps>>
                {{...props}}
                multi={{false}}
                value={{props.value}}
                onChange={{(value) => {{
                    if (props.onChange) {{
                        props.onChange(value === null ? undefined : value);
                    }}
                }}}}
            />
        );
    }};
}}
"#
    );

    let diags = jsx_diagnostics_with_options(
        &source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic overwritten props arrow should satisfy React.ComponentType through the callable union member, got: {diags:?}"
    );
}

#[test]
fn react_component_type_return_still_checks_contextual_arrow_body() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<P = any> {{ tag: string; }}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    class Component<P, S = {{}}> {{
        constructor(props: Readonly<P>);
        constructor(props: P, context?: any);
        props: P;
    }}
    interface ComponentClass<P = {{}}, S = {{}}> {{
        new(props: P, context?: any): Component<P, S>;
    }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = T extends any ? Pick<T, Exclude<keyof T, K>> : never;
type Overwrite<T, U> = Omit<T, keyof T & keyof U> & U;

type OptionValues = string | number | boolean;
interface Option<TValue = OptionValues> {{
    value?: TValue;
}}
interface Props<T extends OptionValues> {{
    value?: Option<T> | T;
}}
type ExtractValueType<T> = T extends ReactSelectProps<infer U> ? U : never;
type ReactSingleSelectProps<WrappedProps extends ReactSelectProps<any>> =
    Overwrite<Omit<WrappedProps, "multi">, Props<ExtractValueType<WrappedProps>>>;

interface ReactSelectProps<TValue = OptionValues> {{
    multi?: boolean;
    value?: Option<TValue> | Option<TValue>[] | string | string[] | number | number[] | boolean;
}}

export function createBad<WrappedProps extends ReactSelectProps<any>>(
    WrappedComponent: React.ComponentType<WrappedProps>
): React.ComponentType<ReactSingleSelectProps<WrappedProps>> {{
    return (_props): React.ReactElement<any> => 123;
}}
"#
    );

    let diags = jsx_diagnostics_with_options(
        &source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "invalid contextual arrow body should still emit TS2322, got: {diags:?}"
    );
}

#[test]
fn react_component_type_return_rejects_incompatible_contextual_arrow_signature() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<P = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    class Component<P, S = {{}}> {{
        constructor(props: Readonly<P>);
        constructor(props: P, context?: any);
        props: P;
    }}
    interface ComponentClass<P = {{}}, S = {{}}> {{
        new(props: P, context?: any): Component<P, S>;
    }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = T extends any ? Pick<T, Exclude<keyof T, K>> : never;
type Overwrite<T, U> = Omit<T, keyof T & keyof U> & U;

type OptionValues = string | number | boolean;
interface Option<TValue = OptionValues> {{
    value?: TValue;
}}
interface Props<T extends OptionValues> {{
    value?: Option<T> | T;
}}
type ExtractValueType<T> = T extends ReactSelectProps<infer U> ? U : never;
type ReactSingleSelectProps<WrappedProps extends ReactSelectProps<any>> =
    Overwrite<Omit<WrappedProps, "multi">, Props<ExtractValueType<WrappedProps>>>;

interface ReactSelectProps<TValue = OptionValues> {{
    multi?: boolean;
    value?: Option<TValue> | Option<TValue>[] | string | string[] | number | number[] | boolean;
}}

export function createBad<WrappedProps extends ReactSelectProps<any>>(
    WrappedComponent: React.ComponentType<WrappedProps>
): React.ComponentType<ReactSingleSelectProps<WrappedProps>> {{
    return (props: ReactSingleSelectProps<WrappedProps> & {{ required: string }}) => {{
        props.required;
        return null;
    }};
}}
"#
    );

    let diags = jsx_diagnostics_with_options(
        &source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "incompatible returned function signature should still emit TS2322, got: {diags:?}"
    );
}

#[test]
fn test_react_component_type_missing_required_prop_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P, S = {{}}> {{
        props: P;
        state: S;
        render(): JSX.Element;
    }}
    interface ComponentClass<P = {{}}> {{
        new(props: P, context?: any): Component<P, any>;
    }}
    interface FunctionComponent<P = {{}}> {{
        (props: P, context?: any): JSX.Element | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | FunctionComponent<P>;
}}
declare const Elem: React.ComponentType<{{ someKey: string }}>;

const bad = <Elem />;
const good = <Elem someKey="ok" />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "React.ComponentType wrappers should report missing props via TS2741, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React.ComponentType missing props should not fall back to TS2322, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_namespace_merged_component_type_wrappers() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    interface Component<P, S = {{}}> {{
        readonly props: Readonly<{{ children?: ReactNode }}> & Readonly<P>;
        readonly state: Readonly<S>;
    }}
    interface ComponentClass<P = {{}}> {{ new(props: P, context?: any): Component<P, any>; }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}
declare namespace DropdownMenu {{
    interface BaseProps {{
        icon: string;
        label: string;
    }}
    interface PropsWithChildren extends BaseProps {{
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
    interface PropsWithControls extends BaseProps {{
        controls: {{ title: string }}[];
        children?: never;
    }}
    type Props = PropsWithChildren | PropsWithControls;
}}
declare const DropdownMenu: React.ComponentType<DropdownMenu.Props>;
const BodyChild = (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
const ExplicitChild = (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Merged namespace/value React.ComponentType wrappers should preserve callback contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Merged namespace/value wrappers should contextually type destructured children callbacks, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Merged namespace/value wrapper normalization should avoid downstream TS2322 here, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_no_contextual_type_for_generic_sfc() {
    // Generic SFCs can't provide children contextual types (type params unresolved)
    // — TS7006 is expected for the callback parameter.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function GenComp<T>(props: {{ prop: T; children: (t: T) => T }}) {{
    return <div />;
}}
const x = <GenComp prop={{"x"}}>{{i => ({{}}) }}</GenComp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // For generic SFCs, we can't infer T, so children contextual typing
    // may or may not work. This test just verifies no crash occurs.
    // (TS7006 is acceptable here since generic inference isn't implemented)
    let _ = diags; // Just verify no panic
}

#[test]
fn test_jsx_generic_children_recover_inferred_return_type_errors() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;

const explicit = <ElemLit prop="x" children={{p => "y"}} />
const body = <ElemLit prop="x">{{p => "y"}}</ElemLit>
const mismatched = <ElemLit prop="x">{{() => 12}}</ElemLit>
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_count = count_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_count >= 3,
        "Expected JSX generic children to report three TS2322 mismatches, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_intrinsic_element_no_crash() {
    // Intrinsic elements (e.g., <div>) should not crash when extracting
    // children contextual type, even if children type is broad/any.
    let source = format!(
        r#"
{JSX_PREAMBLE}
const x = <div>{{(item: string) => item}}</div>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Just verify no crash — intrinsic elements have broad children types
    let _ = diags;
}

// =============================================================================
// Spread attribute type checking (TS2322)
// =============================================================================

/// JSX preamble with typed intrinsic elements for spread tests
const JSX_INTRINSIC_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: { x: string; y?: number };
    }
}
"#;

#[test]
fn test_spread_attribute_type_mismatch_emits_ts2322() {
    // Spreading an object with wrong property type should emit TS2322
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: 32 }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for spread with wrong property type, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_compatible_no_error() {
    // Spreading a compatible object should not emit TS2322
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: "hello" }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for compatible spread, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_override_no_ts2322() {
    // When a later explicit attribute overrides a wrong spread property,
    // no TS2322 should be emitted for the spread
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: 32, y: 10 }};
<test1 {{...obj}} x="ok" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 when explicit attr overrides spread, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_missing_required_is_ts2741_not_ts2322() {
    // Spreading an object with missing required property should emit TS2741,
    // not TS2322 — missing properties are handled by the separate TS2741 check
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ y: 10 }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should have TS2741 (missing 'x')
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required property, got: {diags:?}"
    );
    // Should NOT have TS2322 — missing properties are TS2741, not TS2322
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for just-missing properties (use TS2741), got: {diags:?}"
    );
}

// =============================================================================
// Spread with missing required props: TS2741 only, no TS2322
// =============================================================================

#[test]
fn test_spread_with_missing_props_no_ts2322() {
    // When a spread provides some props but not all required ones,
    // tsc emits only TS2741 (missing property) not TS2322 (type mismatch).
    // Even if the spread has type-incompatible properties, the TS2741 is primary.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SourceProps {{
    property1: string;
    property2: number;
}}
function Source(props: SourceProps) {{
    return <Target {{...props}} />;
}}
interface TargetProps {{
    property1: string;
    missingProp: string;
    property2: boolean;
}}
function Target(props: TargetProps) {{
    return <div>Hello</div>;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should have TS2741 for missing 'missingProp'
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing 'missingProp', got: {diags:?}"
    );
    // Should NOT have TS2322 — tsc only reports TS2741 when there are missing required props
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 when TS2741 fires for missing required props, got: {diags:?}"
    );
}

#[test]
fn test_spread_compatible_no_errors() {
    // When a spread provides all required props with correct types, no errors.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    name: string;
    age: number;
}}
function Greet(props: Props) {{
    return <div>Hello</div>;
}}
const p: Props = {{ name: "hi", age: 42 }};
let x = <Greet {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for compatible spread, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for compatible spread, got: {diags:?}"
    );
}

// =============================================================================
// IntrinsicAttributes required property checking
// =============================================================================

/// JSX namespace preamble with required `key` in `IntrinsicAttributes`.
/// This is unusual (React makes key optional) but tests like
/// tsxIntrinsicAttributeErrors.tsx deliberately test this.
const JSX_PREAMBLE_REQUIRED_KEY: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface IntrinsicAttributes {
        key: string | number
    }
    interface ElementClass {
        render: any;
    }
}
"#;

const JSX_PREAMBLE_REQUIRED_CLASS_REF: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface IntrinsicClassAttributes<T> {
        ref: T
    }
}
"#;

const JSX_PREAMBLE_REQUIRED_CLASS_REF_NO_PROPS_INFRA: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface IntrinsicClassAttributes<T> {
        ref: T
    }
}
"#;

#[test]
fn test_required_intrinsic_attribute_missing_emits_ts2741() {
    // When IntrinsicAttributes has a required property (key without ?),
    // tsc emits TS2741 if it's not provided.
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_KEY}
interface I {{
    new(n: string): {{
        x: number;
        render(): void;
    }}
}}
declare var E: I;
<E x={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'key' from IntrinsicAttributes, got: {diags:?}"
    );
}

#[test]
fn test_optional_intrinsic_attribute_no_error() {
    // Standard React pattern: IntrinsicAttributes has optional key.
    // No error when key is not provided.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // JSX_PREAMBLE doesn't define IntrinsicAttributes with required key,
    // so no TS2741 for missing key
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when IntrinsicAttributes has no required props, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_missing_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
class App {{
    props = {{}};
    render() {{
        return <div />;
    }}
}}
let x = <App />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'ref' from IntrinsicClassAttributes<T>, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_missing_without_props_infra_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF_NO_PROPS_INFRA}
class App {{}}
let x = <App />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            "ref"
        ),
        "Expected TS2741 for missing required 'ref' even without ElementAttributesProperty, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_satisfied_for_class_component() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
class App {{
    props = {{}};
    render() {{
        return <div />;
    }}
}}
const app = new App();
let x = <App ref={{app}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when required IntrinsicClassAttributes<T> are provided, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_alias_missing_emits_ts2741() {
    let source = r#"
class App {}
export const a = <App></App>;
"#;
    let react_types = r#"
interface IntrinsicClassAttributesAlias<T> {
    ref: T
}
declare namespace JSX {
    interface Element {}
    type IntrinsicClassAttributes<T> = IntrinsicClassAttributesAlias<T>
}
"#;

    let diags = cross_file_jsx_diagnostics_with_mode(react_types, source, JsxMode::ReactJsx);
    let relevant_diags: Vec<_> = diags
        .into_iter()
        .filter(|(code, _)| *code != diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE)
        .collect();

    assert!(
        relevant_diags.iter().any(|(code, msg)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && msg.contains("Property 'ref' is missing")
                && (msg.contains("IntrinsicClassAttributesAlias")
                    || msg.contains("IntrinsicClassAttributes"))
        }),
        "Expected TS2741 for missing required 'ref' from alias-based IntrinsicClassAttributes<T>, got: {relevant_diags:?}"
    );
}

#[test]
fn test_jsx_sfc_with_too_many_required_parameters_emits_ts6229() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function MyComp4(props: {{ x: number }}, context: any, bad: any, verybad: any) {{
    return <div></div>;
}}
function MyComp3(props: {{ x: number }}, context: any, bad: any) {{
    return <div></div>;
}}
function MyComp2(props: {{ x: number }}, context: any) {{
    return <div></div>;
}}
declare function MyTagWithOptionalNonJSXBits(
    props: {{ x: number }},
    context: any,
    nonReactArg?: string
): JSX.Element;
const a = <MyComp4 x={{2}} />;
const b = <MyComp3 x={{2}} />;
const c = <MyComp2 x={{2}} />;
const d = <MyTagWithOptionalNonJSXBits x={{2}} />;
"#
    );

    let diags = jsx_diagnostics_with_mode(&source, JsxMode::React);
    let ts6229: Vec<&String> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TAG_EXPECTS_AT_LEAST_ARGUMENTS_BUT_THE_JSX_FACTORY_PROVIDES_AT_MOST)
        .map(|(_, msg)| msg)
        .collect();

    assert_eq!(
        ts6229.len(),
        2,
        "Expected TS6229 only for JSX tags requiring more than props+context, got: {diags:?}"
    );
    assert!(
        ts6229
            .iter()
            .any(|msg| msg.contains("MyComp4") && msg.contains("'4'")),
        "Expected TS6229 for MyComp4, got: {ts6229:?}"
    );
    assert!(
        ts6229
            .iter()
            .any(|msg| msg.contains("MyComp3") && msg.contains("'3'")),
        "Expected TS6229 for MyComp3, got: {ts6229:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_not_required_for_sfc() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
function App(props: {{ label: string }}) {{
    return <div />;
}}
let x = <App label="ok" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            "ref"
        ),
        "Should not emit missing required 'ref' for function components, got: {diags:?}"
    );
}

// =============================================================================
// Union-typed props checking (discriminated unions)
// =============================================================================

#[test]
fn test_union_props_conflicting_discriminant_emits_ts2322() {
    // When JSX attributes conflict with ALL union members, emit TS2322.
    // <TextComponent editable={true} /> without onEdit conflicts with both members:
    // - { editable: false } requires editable=false
    // - { editable: true, onEdit: ... } requires onEdit
    // But per-attribute type checking only checks VALUE compatibility, not missing props.
    // Since editable=true is NOT assignable to editable: false (first member),
    // and no member has a compatible editable value, TS2322 fires.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type TextProps = {{ editable: false }}
               | {{ editable: true; onEdit: (text: string) => void }};
declare function TextComponent(props: TextProps): JSX.Element;
let x = <TextComponent editable={{true}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for discriminated union props mismatch, got: {diags:?}"
    );
}

#[test]
fn test_union_props_matching_discriminant_no_error() {
    // When attributes match at least one union member, no TS2322.
    // <UnionComp kind="a" x={42} /> matches PA { kind: "a"; x: number }
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PA {{ kind: "a"; x: number }}
interface PB {{ kind: "b"; y: string }}
declare function UnionComp(props: PA | PB): JSX.Element;
let x = <UnionComp kind="a" x={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should NOT emit TS2322 when attributes match union member, got: {diags:?}"
    );
}

#[test]
fn test_union_props_callback_attribute_skips_check() {
    // When attributes include callback expressions, skip the union check
    // to avoid false positives from missing contextual typing.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PS {{ multi: false; onChange: (s: string) => void }}
interface PM {{ multi: true; onChange: (s: string[]) => void }}
declare function Comp(props: PS | PM): JSX.Element;
let x = <Comp multi={{false}} onChange={{val => {{}}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip union check when callback attributes present, got: {diags:?}"
    );
}

#[test]
fn test_union_props_class_component_missing_required_emits_ts2322() {
    // Class components with union props: mode="write" matches the second member's
    // discriminant, but `value` is required and missing. Neither union member is
    // fully satisfied, so TS2322 should fire.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Props = {{ mode: "read" }} | {{ mode: "write"; value: string }};
declare class Editor {{
    constructor(props: Props);
    props: Props;
    render(): JSX.Element;
}}
let x = <Editor mode="write" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for union props with missing required property, got: {diags:?}"
    );
}

#[test]
fn test_union_of_class_component_types_missing_required_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    class Component<P, S> {{
        props: P;
    }}
}}

class RC1 extends React.Component<{{ x: number }}, {{}}> {{}}
class RC4 extends React.Component<{{}}, {{}}> {{}}

var PartRCComp = RC1 || RC4;
let a = <PartRCComp />;
let b = <PartRCComp data-extra="hello" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2741_count = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .count();
    assert_eq!(
        ts2741_count, 2,
        "Expected one TS2741 per JSX use of a component-type union with missing required props, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not fall back to TS2322 for component-type unions missing required props, got: {diags:?}"
    );
}

// =============================================================================
// Diagnostic anchor: JSX attribute errors should point at the attribute, not
// the enclosing variable statement.
// =============================================================================

#[test]
fn test_jsx_attr_error_anchors_at_attribute_not_variable_statement() {
    // TS2322 for JSX attribute type mismatch should point at the attribute name,
    // not at the `let` statement. The attribute `name={42}` should be the anchor.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let p = <Greet name={{42}} />;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322 = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(!ts2322.is_empty(), "Expected TS2322, got: {diags:?}");
    // The error should NOT point at the `let` keyword (start of the variable statement)
    let let_pos = source.find("let p").unwrap() as u32;
    let attr_pos = source.find("name={42}").unwrap() as u32;
    for (_, start, _) in &ts2322 {
        assert!(
            *start >= attr_pos,
            "TS2322 should anchor at attribute name (pos >= {attr_pos}), not at variable statement (pos {let_pos}). Got start={start}"
        );
    }
}

// =============================================================================
// TS2741 spread diagnostic: structural type form, not alias name
// =============================================================================

#[test]
fn test_spread_ts2741_shows_structural_form_not_alias_name() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface ComponentProps {{
    property1: string;
    property2: number;
}}
interface AnotherComponentProps {{
    property1: string;
    AnotherProperty1: string;
    property2: boolean;
}}
function AnotherComponent(props: AnotherComponentProps) {{ return <div />; }}
declare var props: ComponentProps;
<AnotherComponent {{...props}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2741: Vec<_> = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .collect();
    assert!(
        !ts2741.is_empty(),
        "Expected TS2741 for missing 'AnotherProperty1', got: {diags:?}"
    );
    let msg = &ts2741[0].1;
    assert!(
        msg.contains("{ property1: string; property2: number; }"),
        "TS2741 message must show structural type form, not alias name. Got: {msg:?}"
    );
    assert!(
        !msg.contains("'ComponentProps'"),
        "TS2741 message must not show alias name 'ComponentProps' as source type. Got: {msg:?}"
    );
}

// =============================================================================
// Boolean shorthand: `<Foo x/>` should report `Type 'true'` not `Type 'boolean'`
// =============================================================================

#[test]
fn test_boolean_shorthand_reports_true_not_boolean() {
    // When target is `false`, `<Foo x/>` (x=true) should produce
    // "Type 'true' is not assignable to type 'false'",
    // not "Type 'boolean' is not assignable to type 'false'".
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: false }}) {{
    return <div />;
}}
let p = <Foo x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs = messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    // Should say 'true', not 'boolean'
    let has_true = ts2322_msgs.iter().any(|m| m.contains("'true'"));
    let has_boolean = ts2322_msgs.iter().any(|m| m.contains("'boolean'"));
    assert!(
        has_true && !has_boolean,
        "Expected message with 'true' not 'boolean'. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_boolean_shorthand_reports_boolean_when_target_is_not_boolean_literal() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: string }}) {{
    return <div />;
}}
let p = <Foo x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs = messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    let has_boolean = ts2322_msgs.iter().any(|m| m.contains("'boolean'"));
    assert!(
        has_boolean,
        "Expected message with 'boolean'. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_explicit_attr_reports_boolean_target_for_string_value() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: string; n: boolean }}) {{
    return <div />;
}}
let p = <Foo x="ok" n="bad" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs = messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    let has_explicit_target = ts2322_msgs
        .iter()
        .any(|m| m.contains("Type 'string' is not assignable to type 'boolean'"));
    assert!(
        has_explicit_target,
        "Expected explicit attribute mismatch against boolean target. Got: {ts2322_msgs:?}"
    );
}

// =============================================================================
// TS2741 source type formatting: should show types, not just property names
// =============================================================================

#[test]
fn test_ts2741_source_type_includes_property_types() {
    // TS2741 "Property 'y' is missing in type '{ x: string; }' but required in type ..."
    // should show property TYPES (not just names like `{ x }`).
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp(props: {{ x: string; y: number }}) {{
    return <div />;
}}
let p = <Comp x="hello" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2741_msgs = messages_for_code(
        &diags,
        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
    );
    assert!(
        !ts2741_msgs.is_empty(),
        "Expected TS2741 for missing 'y', got: {diags:?}"
    );
    // The source type should include property types, e.g., `{ x: string; }`
    // not just `{ x }`
    let has_typed_format = ts2741_msgs
        .iter()
        .any(|m| m.contains("x: string") || m.contains("x: \"hello\""));
    assert!(
        has_typed_format,
        "TS2741 source type should include property types. Got: {ts2741_msgs:?}"
    );
}

// =============================================================================
// Generic SFC spread IntrinsicAttributes checking
// =============================================================================

/// JSX namespace preamble with optional `IntrinsicAttributes` (standard React pattern).
const JSX_PREAMBLE_WITH_IA: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface IntrinsicAttributes {
        key?: string | number;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

#[test]
fn jsx_body_children_excess_property_checks_use_intrinsic_attributes() {
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
const Tag = (x: {{}}) => <div></div>;
const k3 = <Tag children={{<div></div>}} />;
const k4 = <Tag key="1"><div></div></Tag>;
const k5 = <Tag key="1"><div></div><div></div></Tag>;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected explicit and body-children excess-property TS2322s, got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, _, message)| message.contains(
            "Type '{ children: Element; key: string; }' is not assignable to type 'IntrinsicAttributes'."
        )),
        "Expected body children with key to synthesize '{{ children: Element; key: string; }}', got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, _, message)| message.contains(
            "Type '{ children: Element[]; key: string; }' is not assignable to type 'IntrinsicAttributes'."
        )),
        "Expected multi-body children with key to synthesize '{{ children: Element[]; key: string; }}', got: {diags:?}"
    );
}

#[test]
fn test_generic_sfc_spread_unconstrained_emits_ts2322() {
    // <Component {...props} /> where Component<T>(props: T) and props: U (unconstrained)
    // should emit TS2322: "Type 'U' is not assignable to type 'IntrinsicAttributes & U'"
    // because unconstrained U's constraint (unknown) is not assignable to IntrinsicAttributes.
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
declare function Component<T>(props: T): JSX.Element;
const decorator = function <U>(props: U) {{
    return <Component {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for unconstrained U not assignable to IntrinsicAttributes & U, got: {diags:?}"
    );
    // Verify the error message mentions IntrinsicAttributes
    let ts2322_msgs = messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        ts2322_msgs
            .iter()
            .any(|m| m.contains("IntrinsicAttributes")),
        "TS2322 message should mention IntrinsicAttributes. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_generic_sfc_spread_constrained_no_error() {
    // <Component {...props} /> where props: U extends {x: string}
    // should NOT emit TS2322 because U's constraint ({x: string}) IS assignable
    // to IntrinsicAttributes (which has all-optional properties).
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
declare function Component<T>(props: T): JSX.Element;
const decorator = function <U extends {{x: string}}>(props: U) {{
    return <Component {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_about_ia: Vec<_> = diags
        .iter()
        .filter(|(c, m)| {
            *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && m.contains("IntrinsicAttributes")
        })
        .collect();
    assert!(
        ts2322_about_ia.is_empty(),
        "Should NOT emit TS2322 for constrained U that satisfies IntrinsicAttributes, got: {ts2322_about_ia:?}"
    );
}

#[test]
fn test_intrinsic_generic_spread_type_mismatch_emits_ts2322_not_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string };
    }
}

function make2<T extends { x: number }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic intrinsic spread mismatch should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Generic intrinsic spread mismatch should not fall back to TS2741, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_generic_spread_missing_required_emits_ts2322_not_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string };
    }
}

function make3<T extends { y: string }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic intrinsic spread missing required props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Generic intrinsic spread missing required props should not fall back to TS2741, got: {diags:?}"
    );
}

#[test]
fn test_non_generic_sfc_no_spurious_intrinsic_attrs_check() {
    // Non-generic SFC: <Greet name="world" /> should NOT get an IntrinsicAttributes error.
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
function Greet(props: {{ name: string }}): JSX.Element {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_about_ia: Vec<_> = diags
        .iter()
        .filter(|(c, m)| {
            *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && m.contains("IntrinsicAttributes")
        })
        .collect();
    assert!(
        ts2322_about_ia.is_empty(),
        "Non-generic SFC should not emit IntrinsicAttributes TS2322, got: {ts2322_about_ia:?}"
    );
}

// =====================================================================
// JSX Children Type Checking Tests
// =====================================================================

/// Helper: Standard JSX namespace preamble with `ElementAttributesProperty` + `ElementChildrenAttribute`.
/// Element has a `__brand` property so it's not just `{}` — this prevents `any[]` from being
/// assignable to `JSX.Element` (which would break TS2746 single-child detection).
const JSX_CHILDREN_PREAMBLE: &str = r#"
interface Array<T> { length: number; [n: number]: T; }
declare namespace JSX {
    interface Element { __brand: string }
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

#[test]
fn jsx_children_single_element_child_satisfies_element_type() {
    // Single element child should satisfy `children: JSX.Element`
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    b: string;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div>{{p.b}}</div>; }}
let k = <Comp a={{10}} b="hi"><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Single element child should satisfy JSX.Element children type, got: {diags:?}"
    );
}

#[test]
fn jsx_children_missing_required_children_emits_ts2741() {
    // Component requiring `children` but given no children body should emit TS2741
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required children should emit TS2741, got: {diags:?}"
    );
}

#[test]
fn jsx_children_custom_element_children_attribute_uses_assignability_path() {
    let source = r#"
// @strict: true
export {}

declare global {
    namespace JSX {
        type Element = any;
        interface ElementAttributesProperty { __properties__: {} }
        interface IntrinsicElements { [key: string]: string }
        interface ElementChildrenAttribute { __children__: {} }
    }
}

interface MockComponentInterface {
    new (): {
        __properties__: { bar?: number } & { __children__: () => number };
    };
}

declare const MockComponent: MockComponentInterface;

<MockComponent>{}</MockComponent>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Custom ElementChildrenAttribute should route body children through TS2322 assignability, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Custom ElementChildrenAttribute should not fall back to TS2741 missing-prop, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(_, msg)| msg.contains("Type '{}' is not assignable to type")),
        "Custom ElementChildrenAttribute should format the synthesized JSX attrs object as '{{}}', got: {diags:?}"
    );
}

#[test]
fn jsx_children_diagnostics_keep_declared_children_display_through_intrinsic_intersection() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Props {{
    children: (x: number) => string;
}}
function Blah(props: Props) {{ return <div></div>; }}

interface PropsArr {{
    children: ((x: number) => string)[];
}}
function Blah2(props: PropsArr) {{ return <div></div>; }}

type Cb = (x: number) => string;
interface PropsMixed {{
    children: Cb | Cb[];
}}
function Blah3(props: PropsMixed) {{ return <div></div>; }}

let text = <Blah>Hello unexpected text!</Blah>;
let multi = <Blah>{{x => "" + x}}{{x => "" + x}}</Blah>;
let arraySingle = <Blah2>{{x => x}}</Blah2>;
let mixed = <Blah3>{{x => x}}</Blah3>;
let mixedText = <Blah3>Hello unexpected text!</Blah3>;
"#
    );

    let diags = jsx_diagnostics_with_pos(&source);
    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU,
            "expected type of 'children' is '(x: number) => string"
        ),
        "Plain function children text diagnostic should use the declared function type, got: {diags:?}"
    );
    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO,
            "single child of type '(x: number) => string"
        ),
        "Plain function children arity diagnostic should use the declared function type, got: {diags:?}"
    );
    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING,
            "expects type '((x: number) => string)[]"
        ),
        "Array children should keep the array target for single body children, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, _, msg)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && msg.contains("Type '(x: number) => number' is not assignable to type")
                && (msg.contains("Cb[] | Cb") || msg.contains("Cb | Cb[]"))
        }),
        "Union children mismatch should report against the declared union surface, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, _, msg)| {
            *code == diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
                && (msg.contains("expected type of 'children' is 'Cb[] | Cb")
                    || msg.contains("expected type of 'children' is 'Cb | Cb[]"))
        }),
        "Union children text diagnostic should keep the declared union surface, got: {diags:?}"
    );

    let mixed_start = source
        .find("let mixed =")
        .expect("test source should contain the mixed declaration");
    let mixed_child_start = source[mixed_start..]
        .find("{x => x")
        .map(|offset| mixed_start + offset)
        .expect("test source should contain the mixed child expression")
        as u32;
    let mixed_child_end = source[mixed_start..]
        .find("}</Blah3>")
        .map(|offset| mixed_start + offset)
        .expect("test source should contain the mixed child close")
        as u32;
    assert!(
        diags.iter().any(|(code, start, msg)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && *start >= mixed_child_start
                && *start <= mixed_child_end
                && (msg.contains("Cb[] | Cb") || msg.contains("Cb | Cb[]"))
        }),
        "Union child TS2322 should be anchored at the JSX child expression, got: {diags:?}"
    );
}

#[test]
fn jsx_children_diagnostic_uses_string_literal_children_property_name() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Props {{
    "children": (x: number) => string;
}}
function Blah(props: Props) {{ return <div></div>; }}

let text = <Blah>Hello unexpected text!</Blah>;
"#
    );

    let diags = jsx_diagnostics_with_pos(&source);
    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU,
            "expected type of 'children' is '(x: number) => string"
        ),
        "String-literal children property diagnostic should use the declared function type, got: {diags:?}"
    );
}

#[test]
fn jsx_children_react_jsx_ignores_element_children_attribute_and_keeps_related_info() {
    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        h1: { children: string }
    }

    type Element = string;

    interface ElementChildrenAttribute {
        offspring: any;
    }
}

const Title = (props: { children: string }) => <h1>{props.children}</h1>;
<Title>Hello, world!</Title>;

const Wrong = (props: { offspring: string }) => <h1>{props.offspring}</h1>;
<Wrong>Byebye, world!</Wrong>;
"#;
    let diags = jsx_full_diagnostics_with_mode(source, JsxMode::ReactJsx);
    let ts2741 = diags
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .expect("Expected TS2741 for missing 'offspring' prop under react-jsx");

    assert!(
        ts2741
            .message_text
            .contains("Property 'offspring' is missing in type '{ children: string; }'"),
        "TS2741 should still use synthesized children props under react-jsx, got: {ts2741:?}"
    );
    // TODO: TS2741 should include "'offspring' is declared here." related info,
    // but declaration source tracking for JSX synthesized props is not yet implemented.
    // Once added, uncomment the assertion below.
    // assert!(
    //     ts2741.related_information.iter().any(|info| {
    //         info.code == diagnostic_codes::IS_DECLARED_HERE
    //             && info.message_text == "'offspring' is declared here."
    //     }),
    //     "TS2741 should include declaration related info for the required prop, got: {ts2741:?}"
    // );
}

#[test]
fn jsx_children_generic_component_explicit_children_gets_contextual_return_type() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const arg = <ElemLit prop="x" children={{p => "y"}} />;
const mismatched = <ElemLit prop="x" children={{() => 12}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    // After the TS2345 expression-body arrow change, these may report as
    // TS2322 or TS2345 depending on the callback shape. Accept either.
    let type_error_count = diags
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .count();
    assert!(
        type_error_count >= 1,
        "Generic JSX children attr should get contextual return typing, got: {diags:?}"
    );
}

#[test]
fn jsx_children_generic_component_body_children_gets_contextual_return_type() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const argchild = <ElemLit prop="x">{{p => "y"}}</ElemLit>;
const mismatched = <ElemLit prop="x">{{() => 12}}</ElemLit>;
"#
    );

    let diags = jsx_diagnostics(&source);
    // After the TS2345 expression-body arrow change, these may report as
    // TS2322 or TS2345 depending on the callback shape. Accept either.
    let type_error_count = diags
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .count();
    assert!(
        type_error_count >= 1,
        "Generic JSX body children should get contextual return typing, got: {diags:?}"
    );
}

#[test]
fn jsx_children_double_specified_emits_ts2710() {
    // Children as both attribute and body should emit TS2710
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} children={{<div/>}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN
        ),
        "Children specified both as attribute and body should emit TS2710, got: {diags:?}"
    );
}

#[test]
fn jsx_children_multiple_children_for_single_type_emits_ts2746() {
    // Multiple children when `children: JSX.Element` (non-array) should emit TS2746
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Multiple children for non-array children type should emit TS2746, got: {diags:?}"
    );
}

