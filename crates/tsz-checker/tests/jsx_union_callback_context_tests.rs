//! JSX union callback prop contextual typing tests.

use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

#[test]
fn jsx_optional_callback_union_prop_no_ts7006() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface MouseEvent {{ clientX: number; clientY: number; }}
interface ButtonProps {{
    onClick?: (e: MouseEvent) => void;
}}
declare function Button(props: ButtonProps): JSX.Element;

<Button onClick={{(e) => e.clientX}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "optional callback prop union should contextually type arrow param, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "optional callback prop union should not produce TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_optional_callback_union_prop_renamed_param_no_ts7006() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface KeyboardEvent {{ key: string; }}
interface InputProps {{
    onKeyDown?: (event: KeyboardEvent) => void;
}}
declare function Input(props: InputProps): JSX.Element;

<Input onKeyDown={{(ev) => ev.key}} />;
<Input onKeyDown={{(x) => x.key}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "renamed param optional callback should contextually type arrow param, got: {diags:?}"
    );
}

#[test]
fn jsx_union_of_two_callables_no_ts7006() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface EventA {{ aField: number; }}
interface EventB {{ bField: string; }}
interface TwoCallbackProps {{
    handler: ((e: EventA) => void) | ((e: EventB) => void);
}}
declare function Widget(props: TwoCallbackProps): JSX.Element;

<Widget handler={{(e) => {{ void e; }}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "union of two callables should not emit TS7006, got: {diags:?}"
    );
}

#[test]
fn jsx_direct_callable_prop_no_ts7006() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface ClickEvent {{ x: number; }}
interface DirectProps {{
    onClick: (e: ClickEvent) => void;
}}
declare function Btn(props: DirectProps): JSX.Element;

<Btn onClick={{(e) => e.x}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "direct callable prop should contextually type arrow param, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "direct callable prop should not emit TS2322, got: {diags:?}"
    );
}
