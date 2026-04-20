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
    let ts2322_count = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2322_count, 2,
        "Array-valued callback children should emit one child-level TS2322 per callback, got: {diags:?}"
    );
    // Verify the diagnostics mention the right types (callback return mismatch).
    // After the fix for contextualTyping33, the error now shows full function types
    // matching tsc's behavior: "Type '(x: number) => number' is not assignable to type '(x: number) => string'"
    assert!(
        diags.iter().any(
            |(code, msg)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && msg.contains("not assignable to type '(x: number) => string'")
        ),
        "TS2322 should mention the function type with string return, got: {diags:?}"
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
        !diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'number' is not assignable to type 'string'.")
        }),
        "Union children single-child errors should not collapse into return-type elaboration, got: {diags:?}"
    );
}

