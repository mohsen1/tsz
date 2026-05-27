#[test]
fn test_sfc_excess_property_emits_ts2322() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" unknownProp="oops" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for excess property 'unknownProp', got: {diags:?}"
    );
}

#[test]
fn jsx_generic_class_component_uses_constraint_for_props_checking() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: P);
    props: P;
    render(): JSX.Element;
}}

interface Prop {{
    a: number;
    b: string;
}}

declare class MyComp<P extends Prop> extends Component<P> {{
    internalProp: P;
}}

let x1 = <MyComp />;
let x2 = <MyComp a="hi" />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for generic class prop mismatch, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
        ),
        "Expected TS2739 for missing constrained props on generic class JSX element, got: {diags:?}"
    );
}

#[test]
fn jsx_overloaded_class_optional_props_accept_possibly_undefined_attr_values() {
    // JSX overload applicability checks the target prop's write surface. With
    // default optional-property semantics, assigning a `T | undefined` value to
    // an optional JSX prop is valid, so the overloaded class constructor should
    // not fall through to TS2769. The second component varies both names and
    // value shape so this locks the optional-write rule, not one spelling.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class TextBox {{
    constructor(props: {{ value?: string }});
    constructor(props: {{ value?: string }}, context?: any);
    props: {{ value?: string }};
}}

declare class CounterBox {{
    constructor(props: {{ amount?: number | false }});
    constructor(props: {{ amount?: number | false }}, context?: any);
    props: {{ amount?: number | false }};
}}

declare const maybeText: string | undefined;
declare const maybeAmount: number | undefined;

let a = <TextBox value={{maybeText}} />;
let b = <CounterBox amount={{maybeAmount}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Optional JSX props should accept possibly-undefined values under default optional-property semantics, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Optional JSX prop write surface should include undefined by default, got: {diags:?}"
    );
}

#[test]
fn jsx_overloaded_class_exact_optional_props_reject_implicit_undefined_attr_values() {
    // Negative counterpart: with exact optional property types, an optional prop
    // whose annotation omits `undefined` has a narrower write surface, so an
    // explicit possibly-undefined JSX attribute should still fail overload
    // applicability.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class ExactBox {{
    constructor(props: {{ label?: string }});
    constructor(props: {{ label?: string }}, context?: any);
    props: {{ label?: string }};
}}

declare const maybeLabel: string | undefined;
let x = <ExactBox label={{maybeLabel}} />;
"#
    );
    let diags = jsx_diagnostics_with_options(
        &source,
        CheckerOptions {
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        has_code(&diags, diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
            || has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Exact optional JSX prop write surface should reject implicit undefined, got: {diags:?}"
    );
}

#[test]
fn jsx_overloaded_class_uses_source_constraints_for_conditional_attr_values() {
    // `ExtractValue<Wrapped>` is an unresolved conditional result, but
    // `Wrapped extends SelectProps<any>` proves the extracted value is valid
    // for `SelectProps<ExtractValue<Wrapped>>["value"]`. JSX overload
    // applicability should use that source constraint instead of rejecting the
    // class constructor overload set.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type OptionValues = string | number | boolean;
interface Option<TValue = OptionValues> {{
    value?: TValue;
    [property: string]: any;
}}
type Options<TValue = OptionValues> = Array<Option<TValue>>;
interface SelectProps<TValue = OptionValues> {{
    value?: Option<TValue> | Options<TValue> | string | string[] | number | number[] | boolean;
}}
interface WrapperProps<T extends OptionValues> {{
    value?: Option<T> | T;
}}
type ExtractValue<Wrapped> = Wrapped extends SelectProps<infer Value> ? Value : never;
declare class Select<TValue = OptionValues> {{
    constructor(props: SelectProps<TValue>);
    constructor(props: SelectProps<TValue>, context?: any);
    props: SelectProps<TValue>;
}}
function wrap<Wrapped extends SelectProps<any>>(props: WrapperProps<ExtractValue<Wrapped>>) {{
    return <Select<ExtractValue<Wrapped>> value={{props.value}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL),
        "Conditional attr values should use referenced source constraints for JSX overload applicability, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Conditional attr value should be assignable to the target optional prop, got: {diags:?}"
    );
}

#[test]
fn jsx_concrete_prop_rejects_unconstrained_generic_attr_value() {
    // tsc emits TS2322 for `<Comp s={x} />` when `Comp` expects `s: string`
    // and `x: T` is an unconstrained outer type parameter. The expected prop
    // type is concrete (no type parameters), so per-attribute checking must
    // run even though the actual value type contains a type parameter.
    // Re-tests with a different type-parameter name to ensure the rule is
    // structural and not a hardcoded `T`.
    for type_param_name in ["T", "K"] {
        let source = format!(
            r#"
{JSX_PREAMBLE}
declare function Comp(props: {{ s: string }}): JSX.Element;
function f<{type_param_name}>(x: {type_param_name}) {{
    return <Comp s={{x}} />;
}}
"#
        );
        let diags = jsx_diagnostics(&source);
        assert!(
            has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
            "Expected TS2322 for unconstrained generic attribute value (param `{type_param_name}` -> string), got: {diags:?}"
        );
    }
}

#[test]
fn jsx_concrete_prop_accepts_constrained_generic_attr_value() {
    // Counterpart to the unconstrained case: when the type parameter's
    // constraint satisfies the expected prop type, no TS2322 should fire.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare function Comp(props: {{ s: string }}): JSX.Element;
function f<T extends string>(x: T) {{
    return <Comp s={{x}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Constrained `T extends string` should be assignable to a `string` prop, got: {diags:?}"
    );
}

#[test]
fn jsx_function_component_same_name_as_props_interface_does_not_recurse() {
    let source = r#"
declare namespace JSX {
  interface Element {
    type: string;
    props: Record<string, unknown>;
  }
  interface IntrinsicElements {
    div: { children?: unknown };
    span: { children?: unknown };
  }
}

interface Fragment {
  children?: unknown[];
}

function Fragment(props: Fragment): JSX.Element {
  return <div>{props.children}</div>;
}

const frag = (
  <Fragment>
    <span>A</span>
    <span>B</span>
  </Fragment>
);

export {};
"#;

    let _ = jsx_diagnostics(source);
}

#[test]
fn test_sfc_type_mismatch_emits_ts2322() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for number not assignable to string, got: {diags:?}"
    );
}

#[test]
fn test_sfc_missing_required_prop_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'name', got: {diags:?}"
    );
}

#[test]
fn test_sfc_correct_props_no_errors() {
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
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for correct props, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for correct props, got: {diags:?}"
    );
}

#[test]
fn test_sfc_optional_props_no_errors() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name?: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for optional props, got: {diags:?}"
    );
}

// =============================================================================
// Guards: generic, overloaded, union, parse errors
// =============================================================================

#[test]
fn test_generic_sfc_skips_checking() {
    // G3 equivalent for SFCs: generic functions are skipped
    let source = format!(
        r#"
{JSX_PREAMBLE}
function GenericComp<T>(props: T) {{
    return <div>Hello</div>;
}}
let x = <GenericComp unknownProp="anything" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2322 because we skip generic SFCs
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip checking for generic SFCs, got: {diags:?}"
    );
}

#[test]
fn test_union_props_skips_checking() {
    // G5: union-typed props are skipped
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PA {{ kind: "a"; x: number }}
interface PB {{ kind: "b"; y: string }}
function UnionComp(props: PA | PB) {{
    return <div>Hello</div>;
}}
let x = <UnionComp kind="a" x={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2322 because we skip union props
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip checking for union props, got: {diags:?}"
    );
}

#[test]
fn test_spread_does_not_produce_false_positives() {
    // Spread attributes should not produce false TS2741
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{ a: string; b: number }}
function Comp(props: Props) {{ return <div />; }}
declare var partial: {{ a: string }};
let x = <Comp {{...partial}} b={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2741 — spread + explicit attrs may cover all required props
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when spread is present, got: {diags:?}"
    );
}

#[test]
fn test_string_index_signature_no_excess_errors() {
    // Props with string index signature should not report excess properties
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{ name: string; [key: string]: any }}
function Comp(props: Props) {{ return <div />; }}
let x = <Comp name="hi" anyOtherProp="fine" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 with string index signature, got: {diags:?}"
    );
}

// =============================================================================
// Intrinsic element attribute checking with interface-referenced props
// =============================================================================
//
// When JSX.IntrinsicElements maps a tag to an *interface reference* (e.g.,
// `test1: Attribs1`), the props type arrives as Lazy(DefId). The checker must
// resolve it before attribute checking; otherwise, the solver's
// PropertyAccessEvaluator returns TypeId::ANY (QueryCache.resolve_lazy → None),
// silently suppressing all type errors.

#[test]
fn test_intrinsic_interface_ref_type_mismatch() {
    // Interface-referenced props (Attribs1) should be resolved from Lazy(DefId)
    // so that type mismatches are detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
    s?: string;
}
let a = <test1 x={'not a number'} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for string not assignable to number on interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_excess_property() {
    // Excess properties on interface-referenced props should be detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
}
let a = <test1 y={0} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for excess property 'y' on interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_correct_props() {
    // Correct props on interface-referenced types should not produce errors.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
    s?: string;
}
let a = <test1 x={42} />;
let b = <test1 />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for correct interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_inline_type_still_works() {
    // Inline object types (not interface references) should continue to work.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test2: { reqd: string };
    }
}
let a = <test2 reqd={42} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for number not assignable to string on inline props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_missing_required() {
    // Missing required props on interface-referenced types should be detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test2: { n: boolean };
    }
}
let a = <test2 />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'n' on inline props, got: {diags:?}"
    );
}

// =============================================================================
// Hyphenated attribute handling
// =============================================================================

#[test]
fn test_hyphenated_attrs_bypass_type_checking() {
    // TSC treats hyphenated attributes (data-*, aria-*) as untyped
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp(props: {{ name: string }}) {{ return <div />; }}
let x = <Comp name="hi" data-testid="foo" aria-label="bar" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not type-check hyphenated attributes, got: {diags:?}"
    );
}

#[test]
fn test_declared_hyphenated_attr_uses_synthesized_assignability_error() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { "data-foo"?: string };
    }
}

<test1 data-foo={32} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("data-foo")
                && message.contains("number")
                && message.contains("not assignable")
        }),
        "Declared hyphenated attrs should use synthesized JSX-attrs assignability, got: {diags:?}"
    );
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'number' is not assignable to type 'string'"
        ),
        "Declared hyphenated attrs should not use the per-attribute TS2322 path, got: {diags:?}"
    );
}

// =============================================================================
// TS2604: JSX element type without call/construct signatures
// =============================================================================

#[test]
fn test_ts2604_emitted_for_non_callable_element() {
    // A non-callable value used as JSX tag should emit TS2604
    let source = format!(
        r#"
{JSX_PREAMBLE}
var Div = 3;
<Div />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should emit TS2604 for non-callable JSX element, got: {diags:?}"
    );
}

#[test]
fn test_ts2604_not_emitted_for_callable_element() {
    // A callable value used as JSX tag should NOT get TS2604
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp() {{ return <div />; }}
<Comp />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should NOT emit TS2604 for callable JSX element, got: {diags:?}"
    );
}

#[test]
fn test_ts2604_not_emitted_for_empty_interface_with_no_intrinsics() {
    // When no JSX.IntrinsicElements exists, string-typed tags shouldn't get TS2604
    let source = r#"
declare namespace JSX {
    interface Element {}
}
var CustomTag = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should NOT emit TS2604 for string-typed JSX tag, got: {diags:?}"
    );
}

#[test]
fn test_string_literal_component_tag_uses_intrinsic_lookup() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
var CustomTag: "h1" = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected TS2339 for missing JSX.IntrinsicElements['h1'], got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Expected TS2604 after intrinsic lookup fails for literal string tag, got: {diags:?}"
    );
}

#[test]
fn test_string_literal_component_tag_succeeds_when_intrinsic_exists() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        h1: any;
    }
}
var CustomTag: "h1" = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Should not emit TS2339 when the literal string tag exists in IntrinsicElements, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should not emit TS2604 when the literal string tag resolves as an intrinsic element, got: {diags:?}"
    );
}

#[test]
fn test_property_access_string_literal_tag_keeps_dynamic_component_behavior() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
const tags: { header: "h1" } = { header: "h1" };
<tags.header />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Property access tags should not be forced through intrinsic lookup, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Property access literal tags should keep dynamic-tag behavior, got: {diags:?}"
    );
}

#[test]
fn test_missing_intrinsic_name_reports_opening_and_closing_tag_errors() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
<customTag> Hello World </customTag>;
"#;
    let diags = jsx_diagnostics_with_pos(source);
    let ts2339_count = diags
        .iter()
        .filter(|(code, _, message)| {
            *code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && message.contains(
                    "Property 'customTag' does not exist on type 'JSX.IntrinsicElements'.",
                )
        })
        .count();
    assert_eq!(
        ts2339_count, 2,
        "Expected TS2339 on both opening and closing tags for missing intrinsic name, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_checks_attributes() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
    }
}
<foobaz prop={10} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 when intrinsic template-literal index signature requires string props, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Template-literal intrinsic match should not fall through to TS2339, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_prefers_more_specific_match() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
        [k: `foobar${string}`]: { prop: 'literal' };
    }
}
<foobarbaz prop="smth" />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 from the more specific template-literal intrinsic match, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "More specific template-literal intrinsic match should not fall through to TS2339, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_accepts_valid_values() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
        [k: `foobar${string}`]: { prop: 'literal' };
    }
}
<foobaz prop="smth" />;
<foobarbaz prop="literal" />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Valid intrinsic template-literal props should not emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Valid intrinsic template-literal props should not emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// Class component attribute checking (DEBUG)
// =============================================================================

#[test]
fn test_class_component_direct_constructor_emits_ts2322() {
    // Class component with direct constructor taking P — type params should be instantiated
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    props: P;
    constructor(props: P);
    render(): JSX.Element;
}}
interface Prop {{
    x: false;
}}
class Poisoned extends Component<Prop> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <Poisoned x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Debug: eprintln!("ALL DIAGNOSTICS: {:?}", diags);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for boolean not assignable to false, got: {diags:?}"
    );
}

#[test]
fn test_class_component_optional_constructor_emits_ts2322() {
    // React-style: constructor(props?: P, context?: any) — should still check props
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P & {{ children?: any }};
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element | null;
}}
interface Prop {{
    x: false;
}}
class Poisoned extends Component<Prop, {{}}> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <Poisoned x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Debug: eprintln!("ALL DIAGNOSTICS: {:?}", diags);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for boolean not assignable to false (React-style class), got: {diags:?}"
    );
}

#[test]
fn test_class_component_missing_required_prop_emits_ts2322_not_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P;
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element;
}}
class NeedsProp extends Component<{{ reqd: string }}, {{}}> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <NeedsProp />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing required class-component props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required class-component props should not fall back to TS2741, got: {diags:?}"
    );
}

#[test]
fn test_property_access_class_component_missing_required_prop_emits_ts2322_not_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P;
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element;
}}
interface ComponentClass<P> {{
    new (props?: P, context?: any): Component<P, any>;
}}
declare namespace TestMod {{
    interface TestClass extends ComponentClass<{{ reqd: string }}> {{}}
    var Test: TestClass;
}}
const T = TestMod.Test;
let p1 = <T />;
let p2 = <TestMod.Test />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing required property-access class-component props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required property-access class-component props should not fall back to TS2741, got: {diags:?}"
    );
}

// =============================================================================
// Cross-file: import React = require('react') with ambient module
// =============================================================================

fn load_cross_file_jsx_lib_files() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

/// Helper to compile a multi-file JSX project and return diagnostics for the main file.
fn cross_file_jsx_diagnostics(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        false,
    )
}

fn cross_file_jsx_diagnostics_with_mode(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode_and_default_libs(lib_source, main_source, jsx_mode, false)
}

fn cross_file_jsx_diagnostics_with_mode_and_default_libs(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
    include_default_libs: bool,
) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_options_and_default_libs(
        lib_source,
        main_source,
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
        include_default_libs,
    )
}

fn cross_file_jsx_diagnostics_with_options_and_default_libs(
    lib_source: &str,
    main_source: &str,
    options: CheckerOptions,
    include_default_libs: bool,
) -> Vec<(u32, String)> {
    let default_lib_files = if include_default_libs {
        load_cross_file_jsx_lib_files()
    } else {
        Vec::new()
    };

    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    // Parse and bind main file
    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    let mut raw_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    raw_lib_contexts.push(tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    binder_main.merge_lib_contexts_into_binder(&raw_lib_contexts);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);

    let mut all_arenas_vec = vec![Arc::clone(&arena_main), Arc::clone(&arena_lib)];
    let mut all_binders_vec = vec![Arc::clone(&binder_main), Arc::clone(&binder_lib)];
    for lib in &default_lib_files {
        all_arenas_vec.push(Arc::clone(&lib.arena));
        all_binders_vec.push(Arc::clone(&lib.binder));
    }
    let all_arenas = Arc::new(all_arenas_vec);
    let all_binders = Arc::new(all_binders_vec);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    let mut checker_lib_contexts: Vec<_> = default_lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker_lib_contexts.push(tsz_checker::context::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    });
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker
        .ctx
        .set_actual_lib_file_count(default_lib_files.len());

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn jsx_element_type_literal_with_generic_merges_global_jsx_exports() {
    let react_like_lib = r#"
declare namespace React {
    type ComponentType<P = {}> = (props: P) => JSX.Element;

    global {
        namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                div: {};
            }
        }
    }
}

declare module "react" {
    export = React;
}
"#;
    let source = r#"
import * as React from "react";

declare global {
    namespace JSX {
        type ElementType<P = any> =
            | {
                [K in keyof JSX.IntrinsicElements]: P extends JSX.IntrinsicElements[K]
                    ? K
                    : never;
            }[keyof JSX.IntrinsicElements]
            | React.ComponentType<P>;
    }
}

let a = <div />;
let c = <ruhroh />;
"#;

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        react_like_lib,
        source,
        JsxMode::React,
        true,
    );

    assert!(
        !has_code(&diags, diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER),
        "JSX.IntrinsicElements should resolve through merged global JSX augmentations, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS
        ),
        "JSX.IntrinsicElements should be visible for intrinsic lookup, got: {diags:?}"
    );
    assert!(
        has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Unknown intrinsic tag should report TS2339, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::ITS_RETURN_TYPE_IS_NOT_A_VALID_JSX_ELEMENT
        ) || has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "Unknown JSX ElementType tag should report a JSX component validity error, got: {diags:?}"
    );
}

#[test]
fn jsx_intrinsic_elements_merges_global_augmentation_with_existing_namespace() {
    let diags = jsx_diagnostics(
        r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
}

export {};

declare global {
    namespace JSX {
        interface IntrinsicElements {
            "my-custom-element": {};
        }
    }
}

<div />;
<my-custom-element />;
<missing-element />;
"#,
    );

    let ts2339_messages =
        messages_for_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE);
    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected only the truly missing intrinsic tag to fail, got: {diags:?}"
    );
    assert!(
        ts2339_messages[0].contains("missing-element"),
        "Expected TS2339 for missing-element only, got: {diags:?}"
    );
}

fn cross_file_jsx_diagnostics_with_pos(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, u32, String)> {
    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    binder_main.merge_lib_contexts_into_binder(&[tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    }]);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        CheckerOptions {
            jsx_mode,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&arena_main),
        Arc::clone(&arena_lib),
    ]));
    checker.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&binder_main),
        Arc::clone(&binder_lib),
    ]));
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_lib_contexts(vec![tsz_checker::context::LibContext {
            arena: Arc::clone(&arena_lib),
            binder: Arc::clone(&binder_lib),
        }]);
    checker.ctx.set_actual_lib_file_count(1);

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    None
}

#[test]
fn test_cross_file_import_require_export_equals() {
    // Simulate: declare module "react" { export = __React; }
    // with: import React = require('react')
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface Prop {
    x: false;
}
class Poisoned extends React.Component<Prop, {}> {
    render() {
        return <div>Hello</div>;
    }
}

let p = <Poisoned x />;
"#;

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        true,
    );
    // The export= resolution should work — no TS2307 "Cannot find module"
    assert!(
        !has_code(&diags, 2307),
        "Should not emit TS2307 for resolvable ambient module, got: {diags:?}"
    );
}

#[test]
fn cross_file_react_class_empty_attrs_reports_missing_props_not_whole_target_ts2322() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
    interface IntrinsicAttributes {}
    interface IntrinsicClassAttributes<T> {}
}
declare namespace __React {
    type ReactNode = string | number | null | undefined;
    class Component<P, S = {}> {
        props: P & { children?: ReactNode };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface PoisonedProp {
    x: string;
    y: "2";
}
class Poisoned extends React.Component<PoisonedProp, {}> {
    render() {
        return <div>Hello</div>;
    }
}

let y = <Poisoned />;
"#;

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        true,
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                && message.contains("PoisonedProp")
                && message.contains("x, y")
        }),
        "Expected TS2739 against bare PoisonedProp for empty React class attrs, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("IntrinsicClassAttributes")
                && message.contains("Type '{}'")
        }),
        "Empty React class attrs should not fall through to whole-target TS2322, got: {diags:?}"
    );
}

#[test]
fn test_cross_file_react_class_generic_props_emit_errors() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    interface Attributes {
        key?: string | number;
    }
    interface ClassAttributes<T> extends Attributes {
        ref?: (instance: T) => any;
    }
    interface ReactNode {
        readonly __tsz_react_node: true;
    }
    class Component<P, S = {}> {
        props: P & { children?: ReactNode };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface Prop {
    a: number,
    b: string
}

class MyComp<P extends Prop> extends React.Component<P, {}> {
    internalProp: P;
    render() {
        return <div>Hello</div>;
    }
}

let x1 = <MyComp />;
let x2 = <MyComp a="hi" />;
"#;

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        lib_source,
        main_source,
        JsxMode::Preserve,
        true,
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for class component prop mismatch, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
        ),
        "Expected TS2739 for missing class component props, got: {diags:?}"
    );
}

#[test]
fn test_cross_file_react_component_override_emits_ts2416() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

class B1<T extends { x: string }> extends React.Component<T, {}> {
    render() {
        return <div>hi</div>;
    }
}
class B<U> extends React.Component<U, {}> {
    props: U;
    render() {
        return <B1 {...this.props} x="hi" />;
    }
}
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        !has_code(&diags, 2307),
        "Should resolve the ambient React module, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        ),
        "Expected TS2416 for incompatible inherited props override across the React module boundary, got: {diags:?}"
    );
}

// =============================================================================
// TS2698: JSX spread type validation
// =============================================================================

#[test]
fn test_ts2698_spread_null_emits_error() {
    // Spreading `null` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = null;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading null, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_undefined_emits_error() {
    // Spreading `undefined` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = undefined;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading undefined, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_never_emits_error() {
    // Spreading `never` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = {} as never;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading never, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_object_spread() {
    // Spreading a valid object in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = { x: 1 };
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for object spread, got: {diags:?}"
    );
}

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

// =============================================================================
// Intrinsic element return type: JSX.Element
// =============================================================================

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

// =============================================================================
// Generic type parameter props: excess property suppression
// =============================================================================

