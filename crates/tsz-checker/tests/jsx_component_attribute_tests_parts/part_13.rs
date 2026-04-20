#[test]
fn jsx_union_children_multiple_children_prefer_array_branch() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
type Cb = (x: number) => string;
interface Prop {{
    children: Cb | Cb[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let err =
    <Comp>
        {{ x => x }}
        {{ x => x }}
    </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_count = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2322_count, 2,
        "Union children in the multi-child form should use the array branch and report child-level TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, 7006),
        "Union children in the multi-child form should preserve contextual typing, got: {diags:?}"
    );
}

#[test]
fn jsx_children_whitespace_only_text_ignored() {
    // Whitespace-only text children should not count as children
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}>   </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should not get any type errors about extra children properties
    let children_errors: Vec<_> = diags
        .iter()
        .filter(|(c, _)| {
            *c == diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN
                || *c == diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        })
        .collect();
    assert!(
        children_errors.is_empty(),
        "Whitespace-only text should not produce children errors, got: {children_errors:?}"
    );
}

#[test]
fn jsx_children_optional_children_no_error_when_absent() {
    // Optional `children?` should not require children body
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children?: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Optional children should not emit TS2741 when absent, got: {diags:?}"
    );
}

// =============================================================================
// JSX empty expression not counted as child (TS2746 fix)
// =============================================================================

/// Empty JSX expressions like {/* comment */} should not count as children.
/// This prevents false TS2746 ("expects a single child but multiple provided").
#[test]
fn jsx_empty_expression_not_counted_as_child() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    children: JSX.Element;
}}
function Wrapper(p: Props) {{ return <div>{{p.children}}</div>; }}
const element = (
    <Wrapper>
        {{/* comment */}}
        <div>Hello</div>
    </Wrapper>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    // TS2746 should NOT fire — the empty expression {/* comment */} doesn't count
    assert!(
        !diags.iter().any(|(c, _)| *c == 2746),
        "Empty JSX expression should not count as child; got TS2746: {diags:?}"
    );
}

/// Verify that a single non-comment child does NOT trigger TS2746.
/// This complements the empty expression test above.
#[test]
fn jsx_single_real_child_no_ts2746() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    children: JSX.Element;
}}
function Wrapper(p: Props) {{ return <div>{{p.children}}</div>; }}
const element = (
    <Wrapper>
        <div>Hello</div>
    </Wrapper>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    // Single child — TS2746 should NOT fire
    assert!(
        !diags.iter().any(|(c, _)| *c == 2746),
        "Single child should not trigger TS2746, got: {diags:?}"
    );
}

// =============================================================================
// JSX factory namespace resolution (TS7026 fix)
// =============================================================================

/// When @jsxFactory is a dotted name like "X.jsx", the JSX namespace
/// should be resolved from X.JSX, not just the global JSX namespace.
#[test]
fn jsx_factory_dotted_resolves_namespace_from_root_exports() {
    let source = r#"
namespace X {
    export namespace JSX {
        export interface IntrinsicElements {
            [name: string]: any;
        }
        export interface Element {}
    }
    export function jsx() {}
}
let a = <div/>;
"#;
    let options = CheckerOptions {
        jsx_mode: JsxMode::React,
        jsx_factory: "X.jsx".to_string(),
        jsx_factory_from_config: true,
        ..CheckerOptions::default()
    };

    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.check_source_file(root);
    let diags: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // TS7026 should NOT fire — X.JSX.IntrinsicElements exists
    assert!(
        !diags.iter().any(|(c, _)| *c == 7026),
        "Factory namespace X.JSX should be found; got TS7026: {diags:?}"
    );
}

#[test]
fn jsx_fragment_with_jsx_pragma_requires_jsxfrag_even_when_react_in_scope() {
    let source = r#"
/** @jsx React.createElement */
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [name: string]: any;
    }
}
declare const React: any;
<></>;
"#;
    let diags = jsx_diagnostics_with_mode(source, JsxMode::React);
    assert!(
        has_code(&diags, 17017),
        "Expected TS17017 for missing @jsxFrag pragma, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, 2874),
        "React is in scope, TS2874 should not fire: {diags:?}"
    );
    assert!(
        !has_code(&diags, 2879),
        "React fragment factory is in scope, TS2879 should not fire: {diags:?}"
    );
}

#[test]
fn jsx_fragment_with_custom_jsx_pragma_uses_default_react_fragment_scope() {
    let source = r#"
/** @jsx dom */
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [name: string]: any;
    }
}
declare function dom(...args: any[]): any;
<></>;
"#;
    let diags = jsx_diagnostics_with_mode(source, JsxMode::React);
    assert!(
        has_code(&diags, 17017),
        "Expected TS17017 for missing @jsxFrag pragma, got: {diags:?}"
    );
    assert!(
        has_code(&diags, 2874),
        "Expected TS2874 when default React fragment factory root is missing, got: {diags:?}"
    );
    assert!(
        has_code(&diags, 2879),
        "Expected TS2879 when fragment factory root is missing, got: {diags:?}"
    );
}

#[test]
fn jsx_factory_namespace_type_alias_does_not_break_dom_create_element_literal_inference() {
    let source = r#"
export class X {
    static jsx() {
        return document.createElement('p');
    }
}

export namespace X {
    export namespace JSX {
        export type IntrinsicElements = {
            [name: string]: any;
        };
    }
}

function A() {
    return (<p>Hello</p>);
}
"#;
    let options = CheckerOptions {
        jsx_mode: JsxMode::React,
        jsx_factory: "X.jsx".to_string(),
        jsx_factory_from_config: true,
        ..CheckerOptions::default()
    };

    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.check_source_file(root);
    let diags: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        !diags.iter().any(|(code, _)| *code == 2741),
        "String-literal DOM generic inference should stay narrow enough to avoid TS2741, got: {diags:?}"
    );
}

/// Class components with multiple construct signatures (like React.Component in
/// react16.d.ts) should go through JSX overload resolution. When all overloads
/// fail (e.g. wrong attribute type), TS2769 should be emitted.
#[test]
fn jsx_class_with_multi_construct_overloads_emits_ts2769() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: P);
    constructor(props: P, context: any);
    props: P;
    render(): JSX.Element;
}}

interface PanelProps {{
    name: string;
}}

declare class Panel extends Component<PanelProps> {{}}

// Correct usage: name is a string
let ok = <Panel name="hello" />;

// Wrong type: name should be string, not number.
// Both constructor overloads fail → TS2769
let err = <Panel name={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, 2769),
        "Expected TS2769 (No overload matches this call) for prop type mismatch \
         on class component with overloaded constructors, got: {diags:?}"
    );
}

// =============================================================================
// TS2604: <this/> in class method should report non-callable JSX element
// =============================================================================

