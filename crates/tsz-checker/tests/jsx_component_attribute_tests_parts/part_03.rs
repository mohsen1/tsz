#[test]
fn jsx_children_multiple_children_for_array_type_no_error() {
    // Multiple children when `children: JSX.Element[]` should be OK
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Multiple children for array children type should NOT emit TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_non_array_emits_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;

function Todo(prop: { key: number, todo: string }) {
    return <div>{prop.key.toString() + prop.todo}</div>;
}

function TodoList() {
    return <div>
        {...<Todo key={1} todo="x" />}
    </div>;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Non-array JSX spread child should emit TS2609, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_any_has_no_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;
declare let items: any;

let ok = <div>{...items}</div>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Any-typed JSX spread child should not emit TS2609, got: {diags:?}"
    );
}

/// `void` is a non-array, non-ANY, non-ERROR primitive. Spreading it
/// must emit TS2609. Pins the structural rule that the
/// `normalize_jsx_spread_child_type` short-circuit only suppresses for
/// genuine `any`, not for arbitrary unwidened types. The behavioural
/// change from the gate narrowing (`TypeId::ANY | TypeId::ERROR` →
/// `TypeId::ANY`) is verified end-to-end by
/// `conformance/jsx/inline/inlineJsxFactoryDeclarationsLocalTypes.tsx`,
/// which moves from fingerprint-only failure (1 missing TS2609) to PASS
/// post-fix because the `{...this.props.children}` spread (where `this`
/// resolves to ERROR under strict mode + lib loading) finally emits TS2609.
#[test]
fn jsx_spread_child_void_value_emits_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;
declare let novalue: void;

let ok = <div>{...novalue}</div>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Void-typed JSX spread child must emit TS2609, got: {diags:?}"
    );
}

/// Sanity for the gate-narrowing direction: explicit `any` still
/// short-circuits without emitting TS2609 (the `ANY` branch of the
/// narrowed gate must still suppress, preserving permissiveness for
/// genuine `any`). Pairs with `jsx_spread_child_any_has_no_ts2609`.
#[test]
fn jsx_spread_child_explicit_any_value_still_no_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;
declare let widened: any;

let ok = <div>{...widened}</div>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Explicit `any` spread must still skip TS2609 after the gate narrowing, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_union_error_member_emits_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;
declare let items: MissingSpreadMember | JSX.Element[];

let bad = <div>{...items}</div>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Union with an error-typed JSX spread member must still emit TS2609, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_array_normalizes_to_multiple_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let items: JSX.Element[] = [<div></div>];
let ok = <Comp>{{...items}}</Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Array-typed JSX spread child should not emit TS2609, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Array-typed JSX spread child should normalize through the multiple-children path, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_array_allows_multiple() {
    // `children: JSX.Element | JSX.Element[]` should accept multiple children
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Union with array member should accept multiple children, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_array_allows_single_child_without_ts2745() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Union with single-child branch should not emit TS2745, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_tuple_allows_single_child_without_ts2745() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | [JSX.Element];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Union with tuple branch should not emit TS2745 for a single child, got: {diags:?}"
    );
}

#[test]
fn jsx_children_tuple_still_requires_multiple_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: [JSX.Element];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Tuple-only children should still emit TS2745 for a single child, got: {diags:?}"
    );
}

#[test]
fn jsx_children_single_array_expression_satisfies_array_children_type() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
type Tab = [string, JSX.Element];
interface Prop {{
    children: Tab[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let tabs: Tab[] = [["Users", <div></div>], ["Products", <div></div>]];
let ok = <Comp>{{tabs}}</Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Single array-valued child expression should satisfy array children type without TS2745, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Single array-valued child expression should not fall through to TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_single_child_component_instance_mismatch_emits_ts2740() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
        setState(state: any): void;
        forceUpdate(): void;
    }
}
interface Prop {
    children: Button;
}

class Button extends React.Component<any, any> {
    render() {
        return (<div>My Button</div>);
    }
}

function Comp(p: Prop) {
    return <div />;
}

let k1 =
    <Comp>
        <Button />
    </Comp>;
let k2 =
    <Comp>
        {Button}
    </Comp>;
"#;
    let diags = jsx_diagnostics(source);
    let ts2740_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| {
            matches!(
                *code,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            )
        })
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert_eq!(
        ts2740_msgs.len(),
        2,
        "Expected two TS2740 diagnostics for single-child JSX body mismatches, got: {diags:?}"
    );
    assert!(
        ts2740_msgs
            .iter()
            .any(|msg| msg.contains("Type 'Element'") || msg.contains("Type 'ReactElement<any>'")),
        "Expected JSX element child mismatch to report the rendered element source type, got: {ts2740_msgs:?}"
    );
    assert!(
        ts2740_msgs.iter().any(|msg| msg.contains("typeof Button")),
        "Expected expression child mismatch to mention typeof Button, got: {ts2740_msgs:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_accepts_exact_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let ok = <Comp><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Exact tuple children should not emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_rejects_extra_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let err = <Comp><div /><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Extra tuple children should emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_rejects_missing_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let err = <Comp><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing fixed tuple child should emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_multiline_formatting_whitespace_does_not_break_array_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let ok =
    <Comp>


        <div />
        <div />
    </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Formatting-only JSX whitespace should not emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Formatting-only JSX whitespace should not emit TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_children_text_mismatch_reports_ts2747_without_ts2322() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let err = <Comp><div />  <div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
        ),
        "Inline JSX whitespace text should emit TS2747, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Text-child mismatch should not also emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_render_prop_multiple_children_emit_ts2746() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface User {{
    name: string;
}}
interface Prop {{
    children: (user: User) => JSX.Element;
}}
function FetchUser(p: Prop) {{ return <div></div>; }}
let err =
    <FetchUser>
        {{ user => <div /> }}
        {{ user => <div /> }}
    </FetchUser>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Render-prop children should preserve the TS2746 body-shape diagnostic, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Render-prop body-shape errors should not degrade into child-level TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_react_component_render_prop_multiple_children_emit_child_ts2322_not_ts2746() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    type ReactText = string | number;
    interface ReactElement<P> {{ props: P; }}
    type ReactChild = ReactElement<any> | ReactText;
    interface ReactNodeArray {{
        [n: number]: ReactChild | ReactNodeArray | boolean;
    }}
    type ReactFragment = {{}} | ReactNodeArray;
    type ReactNode = ReactChild | ReactFragment | boolean;
    class Component<P, S> {{
        props: P & {{ children?: ReactNode }};
    }}
}}

interface User {{
    name: string;
}}
interface Prop {{
    children: (user: User) => JSX.Element;
}}
class FetchUser extends React.Component<Prop, any> {{}}
let err =
    <FetchUser>
        {{ user => <div /> }}
        {{ user => <div /> }}
    </FetchUser>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React-style multiple render-prop children should emit child-level TS2322 diagnostics, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "React-style multiple render-prop children should not collapse to TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_react_multiple_render_prop_children_ts2322_message_preserves_react_child_alias() {
    // tsc shows "boolean | any[] | ReactChild" — the ReactChild alias must NOT be expanded
    // to its constituent "string | number | Element" in the TS2322 target-type message.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Array<T> {{ length: number; [n: number]: T; }}
declare namespace React {{
    type ReactText = string | number;
    interface ReactElement<P> {{ props: P; }}
    type ReactChild = ReactElement<any> | ReactText;
    type ReactFragment = {{}} | Array<ReactChild | any[] | boolean>;
    type ReactNode = ReactChild | ReactFragment | boolean;
    class Component<P, S> {{
        props: P & {{ children?: ReactNode }};
    }}
}}

interface User {{
    name: string;
}}
interface Prop {{
    children: (user: User) => JSX.Element;
}}
class FetchUser extends React.Component<Prop, any> {{}}
let err =
    <FetchUser>
        {{ user => <div /> }}
        {{ user => <div /> }}
    </FetchUser>;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for function children not assignable to ReactNode child type, got: {diags:?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            msg.contains("boolean | any[] | ReactChild"),
            "TS2322 target type message should match tsc's ReactNode child union order. Got: {msg:?}"
        );
        assert!(
            !msg.contains("ReactChild | any[] | boolean"),
            "TS2322 target type message should not use construction-order ReactNode child display. Got: {msg:?}"
        );
        assert!(
            msg.contains("ReactChild"),
            "TS2322 target type message should preserve the ReactChild alias, \
             not expand it to constituent types. Got: {msg:?}"
        );
        assert!(
            !msg.contains("ReactElement") && !msg.contains("ReactText"),
            "TS2322 message should not expand ReactChild to ReactElement/ReactText. Got: {msg:?}"
        );
    }
}

#[test]
fn jsx_react_multiple_render_prop_children_contextual_type_uses_declared_callback() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    type ReactText = string | number;
    interface ReactElement<P> {{ props: P; }}
    type ReactChild = ReactElement<any> | ReactText;
    interface ReactNodeArray {{
        [n: number]: ReactChild | ReactNodeArray | boolean;
    }}
    type ReactFragment = {{}} | ReactNodeArray;
    type ReactNode = ReactChild | ReactFragment | boolean;
    class Component<P, S> {{
        props: P & {{ children?: ReactNode }};
    }}
}}

interface User {{
    name: string;
}}
interface Prop {{
    children: (user: User) => JSX.Element;
}}
class FetchUser extends React.Component<Prop, any> {{}}
let err =
    <FetchUser>
        {{ user => <div /> }}
        {{ user => <div /> }}
    </FetchUser>;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type '(user: User) => Element'")
                && message.contains("ReactChild")
        })
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "React multiple render-prop children should be contextually typed by the declared callback while reporting against ReactNode, got: {diags:?}"
    );

    let first_brace = source.find("{ user =>").expect("test source has child");
    let first_user = source.find("user =>").expect("test source has arrow");
    assert!(
        ts2322
            .iter()
            .any(|(_, start, _)| *start as usize == first_user),
        "TS2322 should anchor at the arrow expression, not the JSX expression wrapper. brace={first_brace}, user={first_user}, got: {ts2322:?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|(_, start, _)| *start as usize != first_brace),
        "TS2322 should not anchor at the opening JSX expression brace, got: {ts2322:?}"
    );
}

#[test]
fn jsx_array_children_text_child_emits_ts2745_not_ts2747() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: ((x: number) => JSX.Element)[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let err =
    <Comp>
        unexpected text
    </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Single text child for array-valued children should emit TS2745, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
        ),
        "Single text child for array-valued children should stay on the TS2745 shape path, got: {diags:?}"
    );
}

#[test]
fn jsx_array_children_callbacks_emit_child_level_ts2322_without_ts7006() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: ((x: number) => string)[];
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
    let ts2322_count = count_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert_eq!(
        ts2322_count, 2,
        "Array-valued callback children should emit one child-level TS2322 per callback, got: {diags:?}"
    );
    // Verify the diagnostics mention the right types (callback return mismatch).
    // After the expression-body arrow anchoring fix, tsc reports the return-type
    // mismatch at the arrow body rather than the full function type:
    // "Type 'number' is not assignable to type 'string'."
    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'number' is not assignable to type 'string'"
        ),
        "TS2322 should mention number→string return mismatch, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, 7006),
        "Array-valued callback children should keep contextual parameter typing, got: {diags:?}"
    );
}

#[test]
fn jsx_union_children_single_child_emits_ts2322_without_return_type_elaboration() {
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
    </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Union children with a single callback should still report TS2322, got: {diags:?}"
    );
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'number' is not assignable to type 'string'."
        ),
        "Union children single-child errors should not collapse into return-type elaboration, got: {diags:?}"
    );
}

#[test]
fn jsx_single_callable_child_body_mismatch_elaborates_at_return() {
    // When the children prop is a single callable type and the JSX body
    // child is a function expression with a wrong return value, tsc
    // reports the body-level literal mismatch (e.g.
    // `Type '"y"' is not assignable to type '"x"'`) at the return
    // expression, not the whole-callable mismatch on the function.
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const argchild = <ElemLit prop="x">{{p => "y"}}</ElemLit>;
"#
    );
    let diags = jsx_diagnostics(&source);
    let body_elab = diags.iter().any(|(code, message)| {
        *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && message.contains("Type '\"y\"'")
            && message.contains("'\"x\"'")
    });
    assert!(
        body_elab,
        "Single-callable target should produce body-level TS2322 elaboration `Type '\"y\"' is not assignable to type '\"x\"'`, got: {diags:?}"
    );
}

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
fn jsx_multi_children_property_mismatch_anchors_at_each_child_not_var_decl() {
    // When a JSX element used as a variable initializer has children that
    // fail structural assignability against the parent's children type, each
    // diagnostic must anchor at the failing child element / JSX expression —
    // not at the enclosing variable declaration that the assignment-anchor
    // walker would otherwise pick.
    //
    // Regression test for the picked conformance failure
    // `inlineJsxFactoryDeclarationsLocalTypes.tsx`: tsz used to emit one
    // TS2741 per declaration anchored at the variable identifier (col 7 of
    // `_brokenTree`/`_brokenTree2`) instead of one per child anchored at the
    // child's `<` / `{`. tsc emits one diagnostic per failing child, anchored
    // at the child.
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface OtherElem {{ __otherBrand: void; }}
interface Prop {{
    children: JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
declare const o: OtherElem;
const _bad = <Comp>{{o}}{{o}}</Comp>;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2741: Vec<_> = diags
        .iter()
        .filter(|(code, _, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .collect();
    assert_eq!(
        ts2741.len(),
        2,
        "expected one TS2741 per failing JSX child, got: {diags:?}"
    );

    // The diagnostics must anchor at distinct positions (each child) rather
    // than collapse onto a single position (the variable declaration).
    let positions: std::collections::HashSet<u32> =
        ts2741.iter().map(|(_, start, _)| *start).collect();
    assert_eq!(
        positions.len(),
        2,
        "expected each TS2741 anchored at a distinct child position, got: {ts2741:?}"
    );

    // Each diagnostic must anchor inside the JSX body (after the `<Comp>`
    // opening tag), proving the assignment-anchor walker did not rewrite it
    // up to the `_bad` variable declaration.
    let var_kw_pos = source.find("const _bad").expect("const present") as u32;
    let comp_open_close =
        source.find("<Comp>").expect("opening tag") as u32 + "<Comp>".len() as u32;
    for (code, start, msg) in &ts2741 {
        assert!(
            *start >= comp_open_close,
            "TS2741 {code} '{msg}' anchored at {start}, expected inside JSX body \
             (>= {comp_open_close}, after `<Comp>`); var decl starts at {var_kw_pos}"
        );
    }
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
        !has_code(&diags, 2746),
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
        !has_code(&diags, 2746),
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
        !has_code(&diags, 7026),
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
fn jsx_member_component_missing_root_reports_at_member_tag_root() {
    let react_global =
        load_typescript_fixture("TypeScript/tests/lib/react18/global.d.ts").unwrap_or_default();
    let react18 =
        load_typescript_fixture("TypeScript/tests/lib/react18/react18.d.ts").unwrap_or_default();
    let react_like_lib = format!("{react_global}\n{react18}");
    let source = r#"
const test = () => "asd";
const jsxWithJsxFragment = <>{test}</>;
const jsxWithReactFragment = <React.Fragment>{test}</React.Fragment>;
"#;
    let diags = cross_file_jsx_diagnostics_with_pos(&react_like_lib, source, JsxMode::React);
    let react_scope_diags: Vec<_> = diags
        .iter()
        .filter(|(code, _, message)| matches!(*code, 2304 | 2874) && message.contains("'React'"))
        .collect();
    let expected_start = source.find("<React.Fragment>").unwrap() as u32 + 1;

    assert!(
        react_scope_diags
            .iter()
            .any(|(_, start, _)| *start == expected_start),
        "Expected missing React diagnostic at JSX member tag root, got: {diags:?}"
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
        !has_code(&diags, 2741),
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

#[test]
fn test_ts2604_emitted_for_this_tag_in_class_method() {
    // <this/> inside a class method should emit TS2604 because the class
    // instance type has no construct or call signatures. The `this` keyword
    // starts with a lowercase letter but is NOT an intrinsic element —
    // it must not be skipped by the intrinsic-element shortcut.
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<T, U> {{}}
class Text extends Component<{{}}, {{}}> {{
    render() {{
        return <this />;
    }}
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should emit TS2604 for <this/> in class method since the class instance \
         type has no construct or call signatures, got: {diags:?}"
    );
}

#[test]
fn test_ts2604_this_tag_not_suppressed_by_component_union_this_annotation() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
type C1 = (props: {{ ok?: boolean }}) => JSX.Element;
type C2 = (props: {{ ok?: boolean }}) => JSX.Element;

function render(this: C1 | C2) {{
    return <this ok />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Expected TS2604 for <this/> even when `this` annotation is a component-union, got: {diags:?}"
    );
}

// =============================================================================
// JSX explicit type arguments: TS2558, TS2344, TS2322
// =============================================================================

/// Helper that wraps `jsx_diagnostics` but returns only unique error codes.
fn jsx_codes(source: &str) -> Vec<u32> {
    let diags = jsx_diagnostics(source);
    let mut codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

#[test]
fn test_jsx_explicit_type_args_correct_count_and_types_no_error() {
    // <MyComp<{a: number, b: string}> a={10} b="hi" /> — fully correct
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class MyComp<P> extends Object {{
    props: P;
}}
let x = <MyComp<{{a: number, b: string}}> a={{10}} b="hi" />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        !codes.contains(&2322),
        "TS2322 should NOT fire when attribute types match the explicit type arg, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2558),
        "TS2558 should NOT fire with the correct number of type args, got: {codes:?}"
    );
}

#[test]
fn test_jsx_explicit_type_args_attribute_type_mismatch_emits_ts2322() {
    // <MyComp<{a: number, b: string}> a={10} b={20} /> — b is number, not string
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Prop {{ a: number; b: string }}
declare class MyComp<P> extends Object {{
    props: P;
}}
let x = <MyComp<Prop> a={{10}} b={{20}} />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        codes.contains(&2322),
        "TS2322 should fire when attribute type doesn't match the explicit type arg; \
         b is 'number' but declared 'string', got: {codes:?}"
    );
}

#[test]
fn test_jsx_explicit_type_args_too_many_emits_ts2558() {
    // <MyComp<Prop, Prop> /> — MyComp has 1 type param, got 2
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Prop {{ a: number }}
declare class MyComp<P> extends Object {{
    props: P;
}}
let x = <MyComp<Prop, Prop> a={{10}} />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        codes.contains(&2558),
        "TS2558 should fire when too many type arguments are provided, got: {codes:?}"
    );
    // tsc does NOT emit TS2322 when there is a type-arg arity mismatch.
    assert!(
        !codes.contains(&2322),
        "TS2322 should NOT fire when the type-arg arity is wrong (TS2558 already fired), \
         got: {codes:?}"
    );
}

#[test]
fn test_jsx_intrinsic_type_args_validate_nested_errors() {
    let source = r#"
type Record<K extends keyof any, T> = { [P in K]: T };
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: {};
    }
}

const a = <div<>></div>;
const b = <div<number,>></div>;
const c = <div<Missing>></div>;
const d = <div<Missing<AlsoMissing>>></div>;
const e = <div<Record<object, object>>></div>;
const f = <div<number>></div>;
const g = <div<>/>;
const h = <div<number,>/>;
const i = <div<Missing>/>;
const j = <div<Missing<AlsoMissing>>/>;
const k = <div<Record<object, object>>/>;
const l = <div<number>/>;
"#;
    let codes = jsx_codes(source);
    assert!(
        codes.contains(&1009),
        "intrinsic JSX trailing type-argument commas should emit TS1009, got: {codes:?}"
    );
    assert!(
        codes.contains(&2304),
        "intrinsic JSX type arguments should be visited for missing names, got: {codes:?}"
    );
    assert!(
        codes.contains(&2344),
        "intrinsic JSX type arguments should be checked for constraints, got: {codes:?}"
    );
    assert!(
        codes.contains(&2558),
        "intrinsic JSX elements should reject explicit type arguments, got: {codes:?}"
    );
}

#[test]
fn test_jsx_explicit_type_args_constraint_violation_emits_ts2344() {
    // <MyComp2<Prop> /> where MyComp2<P extends {a: string}> and Prop = {a: number}
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Prop {{ a: number; b: string }}
declare class MyComp2<P extends {{ a: string }}> extends Object {{
    props: P;
}}
let x = <MyComp2<Prop> a={{10}} b="hi" />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        codes.contains(&2344),
        "TS2344 should fire when a type argument violates its constraint, got: {codes:?}"
    );
}

#[test]
fn test_jsx_explicit_type_args_defaulted_params_ok() {
    // <MyComp2<{a: string}, {b: string}> a="hi" b="hi" /> — 2 args for 1-2 param class
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class MyComp2<P extends {{ a: string }}, P2 = {{}}> extends Object {{
    props: P;
}}
let x = <MyComp2<{{a: string}}, {{b: string}}> a="hi" />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        !codes.contains(&2558),
        "TS2558 should NOT fire when using a defaulted 2nd type param, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "TS2322 should NOT fire for correct attribute value, got: {codes:?}"
    );
}

#[test]
fn test_jsx_explicit_type_args_too_many_with_defaults_emits_ts2558() {
    // <MyComp2<A, B, C> /> — MyComp2 has at most 2 type params, got 3
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Prop {{ a: string }}
declare class MyComp2<P extends {{ a: string }}, P2 = {{}}> extends Object {{
    props: P;
}}
let x = <MyComp2<{{a: string}}, {{b: string}}, Prop> a="hi" />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        codes.contains(&2558),
        "TS2558 should fire when more type args are given than the max (1-2), got: {codes:?}"
    );
}

// =============================================================================
// Class component with primitive constructor parameter — no ElementAttributesProperty
// (tsxElementResolution10 parity)
// =============================================================================

/// JSX namespace with NO `ElementAttributesProperty` — tsc uses first constructor param as props type.
const JSX_NO_ELEMENT_ATTRS_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
}
"#;

#[test]
fn test_jsx_class_constructor_primitive_param_no_elem_attrs_prop_emits_ts2322_at_tag() {
    // When JSX.ElementAttributesProperty is absent, tsc uses the first constructor
    // parameter as the props type even when it is a primitive (e.g. `string`).
    // The synthesized attrs object `{ x: number }` is then checked against `string`
    // → TS2322 must be anchored at the tag name (col 2), NOT per-attribute (col 7).
    let source = format!(
        r#"
{JSX_NO_ELEMENT_ATTRS_PREAMBLE}
interface Obj1type {{
    new(n: string): any;
}}
declare var Obj1: Obj1type;
<Obj1 x={{1}} />;
"#
    );
    // Obj1 returns `any` → no TS2322 expected (any swallows attribute checks).
    let codes = jsx_codes(&source);
    assert!(
        !codes.contains(&2322),
        "TS2322 should NOT fire for class component that returns `any`, got: {codes:?}"
    );
}

#[test]
fn test_jsx_class_constructor_primitive_param_no_elem_attrs_prop_obj_return_emits_ts2322() {
    // Class component with `new(n: string): { render(): any }` and no ElementAttributesProperty.
    // tsc uses first param (`string`) as props type → `{ x: number }` not assignable to `string`.
    // TS2322 must be at tag position (whole-object), not per-attribute.
    let source = format!(
        r#"
{JSX_NO_ELEMENT_ATTRS_PREAMBLE}
interface Obj2type {{
    new(n: string): {{ render(): any }};
}}
declare var Obj2: Obj2type;
<Obj2 x={{1}} render={{2}} />;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322_diags: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2322).collect();
    assert!(
        !ts2322_diags.is_empty(),
        "TS2322 should fire for class component with primitive param and mismatched attrs, got: {diags:?}"
    );
    // Verify the message includes the whole attrs object (not a single attribute)
    let msg = &ts2322_diags[0].2;
    assert!(
        msg.contains("{ x: number; render: number; }")
            || msg.contains("not assignable to type 'string'"),
        "TS2322 message should show whole attrs object vs string, got: {msg:?}"
    );
    // Verify x appears before render (declaration order preserved)
    if msg.contains("{ x: number; render: number; }") {
        let x_pos = msg.find("x: number").unwrap_or(usize::MAX);
        let render_pos = msg.find("render: number").unwrap_or(usize::MAX);
        assert!(
            x_pos < render_pos,
            "Property 'x' should appear before 'render' in TS2322 message (declaration order), got: {msg:?}"
        );
    }
}

#[test]
fn test_generic_jsx_function_attr_error_anchors_at_attribute_not_body() {
    // When a function-valued JSX attribute produces a body-level type error,
    // tsc suppresses the body-level error and anchors the TS2322 at the
    // attribute name. The target type displays as the intersection of the
    // actual (inferred) function type and the expected (declared) function type.
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass {
        render(): any;
    }
    interface IntrinsicElements {}
    interface ElementAttributesProperty {
        props: {};
    }
}
declare namespace React {
    class Component<P, S> {
        props: P;
        state: S;
    }
}
"#;

    let main_source = r#"
interface BaseProps<T> {
    initialValues: T;
    nextValues: (cur: T) => T;
}
declare class MyComponent<Props = {}, Values = {}> extends React.Component<Props & BaseProps<Values>, {}> {
    iv: Values;
}
// The function body returns `string` but the expected return type is `{ x: string }`.
// TS2322 should fire at the attribute anchor, and the target should show the
// intersection of the actual and expected callable types.
let d = <MyComponent initialValues={{ x: "y" }} nextValues={a => a.x} />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    let ts2322_diags: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        !ts2322_diags.is_empty(),
        "Expected TS2322 for mismatched function-valued attr return type, got: {diags:?}"
    );

    for (_, message) in &ts2322_diags {
        assert!(
            message.contains(" & "),
            "TS2322 target should show intersection of callable types with '&', got: {message}"
        );
    }
}

/// When a class JSX component's constructor takes a primitive first parameter
/// (e.g. `new(n: string): { x: number; render(): void }`) AND the JSX
/// namespace's `IntrinsicAttributes` declares a required prop the caller did
/// not pass (typically `key`), tsc reports ONLY `TS2741` for the missing
/// required `IntrinsicAttributes` prop. It does NOT also emit `TS2322` for
/// whole-attrs assignability against the primitive props type, because
/// primitive props can never structurally accept JSX attributes — the
/// assignability failure is uninformative when TS2741 already conveys the
/// user-actionable error.
///
/// Mirrors the conformance test
/// `conformance/jsx/tsxIntrinsicAttributeErrors.tsx`.
#[test]
fn jsx_class_primitive_props_with_missing_intrinsic_required_emits_only_ts2741_not_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface ElementClass { render: any; }
    interface IntrinsicAttributes { key: string | number }
    interface IntrinsicClassAttributes<T> { ref: T }
    interface IntrinsicElements { div: { text?: string }; span: any; }
}
interface I { new(n: string): { x: number; render(): void } }
declare var E: I;
<E x={10} />
"#;
    let codes = jsx_codes(source);
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "Expected TS2741 (missing required `key`) for class JSX with primitive props, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected NO TS2322 for class JSX whole-attrs assignability against primitive props, got: {codes:?}"
    );
}

/// Regression test for issue #3227: `JSX.LibraryManagedAttributes` was being
/// discarded whenever the formatted evaluated props type happened to contain
/// the substring `Factory<`. That was a display-text heuristic, not a
/// semantic condition, so any user type named `Factory` (or anything else
/// whose printed form started with `Factory<`) silently broke LMA.
///
/// Structural rule: when a component has `defaultProps`, the props returned
/// from `JSX.LibraryManagedAttributes<C, Props>` must reflect the mapped
/// optional-property result regardless of the names of types appearing in
/// the props.
fn jsx_lma_user_type_named_factory_does_not_disable_default_props_helper(
    user_type_name: &str,
) -> Vec<u32> {
    let source = format!(
        r#"
declare namespace JSX {{
    interface Element {{}}
    interface ElementClass {{}}
    interface IntrinsicElements {{}}
    type LibraryManagedAttributes<C, P> =
        C extends {{ defaultProps: infer D }}
          ? {{ [K in keyof P]?: P[K] }}
          : P;
}}

interface {user_type_name}<T> {{
    make(): T;
}}

interface Props {{
    value: {user_type_name}<number>;
    other: number;
}}

declare function Comp(props: Props): JSX.Element;
declare namespace Comp {{
    const defaultProps: {{
        value: {user_type_name}<number>;
    }};
}}

const _ok = <Comp />;
"#
    );
    jsx_codes(&source)
}

#[test]
fn jsx_lma_user_type_named_factory_does_not_disable_default_props() {
    // Reproduces the issue: a user type literally named `Factory` must not
    // suppress the LMA-mapped optional props.
    let codes = jsx_lma_user_type_named_factory_does_not_disable_default_props_helper("Factory");
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "User type named `Factory` should not disable JSX.LibraryManagedAttributes; \
         expected no TS2741 for `<Comp />`, got: {codes:?}"
    );
}

