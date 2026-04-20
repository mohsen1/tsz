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

