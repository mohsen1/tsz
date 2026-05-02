//! Tests for TS2322 message-target trimming in JSX spread attribute
//! diagnostics.
//!
//! When a JSX spread argument fails to satisfy a class component's prop type,
//! tsc renders the bare component prop type (e.g. `PoisonedProp`) rather than
//! the children-injected form (`PoisonedProp & { children?: ReactNode |
//! undefined; }`). Our `check_spread_property_types` adopts the same display.
//!
//! Conformance test: `tsxSpreadAttributesResolution5.tsx`.
//!
//! NOTE: this test does not depend on the React lib types — it builds a
//! minimal JSX setup with `IntrinsicAttributes`, `IntrinsicClassAttributes`,
//! a fake `Component` base class, and a `children?` injection so the checker
//! takes the same code path as the real conformance test without the slow
//! lib-loading.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

const JSX_PRELUDE: &str = r#"
declare namespace JSX {
    interface IntrinsicAttributes { key?: string }
    interface IntrinsicClassAttributes<T> {}
    interface ElementChildrenAttribute { children: {} }
    interface IntrinsicElements { div: {} }
}
type ReactNode = string | number | null | undefined;
declare class Component<P, S> {
    constructor(props: P);
    render(): any;
    props: P & { children?: ReactNode };
    state: S;
}
"#;

#[test]
fn ts2322_spread_message_strips_children_injection() {
    let source = format!(
        r#"{JSX_PRELUDE}

interface PoisonedProp {{
    x: string;
    y: 2;
}}

class Poisoned extends Component<PoisonedProp, {{}}> {{
    render() {{ return null; }}
}}

let obj = {{ x: "hello", y: 2 }};
let p = <Poisoned {{...obj}} />;
"#
    );
    let diags = compile_diagnostics(&source);
    let ts2322 = diags
        .iter()
        .find(|(code, _)| *code == 2322)
        .unwrap_or_else(|| {
            panic!("expected TS2322, got: {diags:?}");
        });
    let msg = &ts2322.1;
    assert!(
        !msg.contains("children?:"),
        "TS2322 message should NOT contain `children?:` (the synthetic injection); got: {msg}"
    );
    assert!(
        !msg.contains("ReactNode"),
        "TS2322 message should NOT mention `ReactNode`; got: {msg}"
    );
    assert!(
        msg.contains("PoisonedProp") || msg.contains("'{ x:"),
        "TS2322 message should mention `PoisonedProp` (or its expanded shape) as target; got: {msg}"
    );
}
