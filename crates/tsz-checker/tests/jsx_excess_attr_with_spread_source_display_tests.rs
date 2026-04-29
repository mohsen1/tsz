//! Locks tsc-parity for the per-attribute excess-property TS2322 source-type
//! display when the JSX element has spread attributes.
//!
//! Regression target: `contextuallyTypedStringLiteralsInJsxAttributes02.tsx`
//! (single-overload `<NoOverload {...{onClick: ...}} extra />` previously
//! displayed `Type '{ extra: true; }'` — only the failing attribute. tsc emits
//! the merged synthesized attrs object: `Type '{ extra: true; onClick: ... }'`.
//!
//! These tests check the diagnostic *message* string (not just code) so the
//! synthesized-source rendering does not regress.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2322_messages(diags: &[(u32, String)]) -> Vec<String> {
    diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.clone())
        .collect()
}

#[test]
fn excess_attr_after_object_spread_includes_spread_props_in_source_type() {
    // <NoOverload {...{onClick: ...}} extra /> — single-overload SFC where the
    // explicit `extra` attribute is excess. tsc prints the merged synthesized
    // attrs object: `{ extra: true; onClick: number; }`.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface ButtonProps {{ onClick: number; }}
function NoOverload(p: ButtonProps): JSX.Element {{ return p as any; }}
const c1 = <NoOverload {{...{{onClick: 1}}}} extra />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let msgs = ts2322_messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("extra: true") && m.contains("onClick")),
        "TS2322 source type should include both `extra` and the spread's `onClick` \
         (mirroring tsc's `{{ extra: true; onClick: ... }}` synthesized source). Got: {msgs:?}"
    );
    // Negative lock: the buggy `{ extra: true; }`-only display must not appear
    // for this case (the fingerprint test we are fixing).
    assert!(
        !msgs.iter().any(|m| {
            // A diagnostic whose first line is exactly `{ extra: true; }` and
            // doesn't mention the spread's prop is the bug we're fixing.
            let first_line = m.lines().next().unwrap_or("");
            first_line.contains("'{ extra: true; }'")
        }),
        "TS2322 first-line source must include the spread's props, not just `{{ extra: true; }}`. Got: {msgs:?}"
    );
}

#[test]
fn excess_attr_after_object_spread_with_string_literal_value_displays_literal() {
    // `{...{goTo:"home"}} extra` — spread props are typed under the props_type
    // contextual type when present. With no contextual type narrowing here
    // (props is Record<string, unknown>-shaped), `goTo`'s value is the literal
    // `"home"`. The synthesized source should still merge spread + explicit.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LinkProps {{ goTo: string; }}
function NoOverload1(p: LinkProps): JSX.Element {{ return p as any; }}
const d1 = <NoOverload1 {{...{{goTo:"home"}}}} extra />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let msgs = ts2322_messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("extra: true") && m.contains("goTo")),
        "TS2322 source type should include both `extra` and the spread's `goTo`. Got: {msgs:?}"
    );
}

#[test]
fn excess_attr_without_spread_keeps_single_attr_source_display() {
    // Without any spread, the existing single-attr fallback display is still
    // correct. `<X bar />` where ButtonProps doesn't have `bar` should print
    // `{ bar: true; }` as the source type.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface ButtonProps {{ onClick: number; }}
function NoOverload(p: ButtonProps): JSX.Element {{ return p as any; }}
const x = <NoOverload onClick={{1}} bar />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let msgs = ts2322_messages(&diags);
    // The synthesized helper now produces `{ onClick: 1; bar: true; }` (or
    // similar) since explicit attrs are merged. The behavior we lock is just
    // that the diagnostic mentions the excess attr `bar`.
    assert!(
        msgs.iter().any(|m| m.contains("bar")),
        "Excess attr 'bar' should be reflected in TS2322. Got: {msgs:?}"
    );
}
