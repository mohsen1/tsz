//! TS1210 regression tests for `arguments` used as a parameter name inside a
//! class method body. tsc reports this with the class-strict-mode message
//! ("Code contained in a class is evaluated in JavaScript's strict mode…"),
//! not with the generic TS1100 strict-mode message — so the parameter-name
//! checker has to route `arguments` to TS1210 when the enclosing scope is a
//! class. `eval` stays on TS1100 in both class and non-class contexts.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        Default::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn arguments_as_class_method_parameter_emits_ts1210() {
    // The picker target: `parseClassDeclarationInStrictModeByDefaultInES6.ts`
    // expects TS1210 at the `arguments` parameter name.
    let source = r#"
class C {
    public foo(arguments: any) { }
}
"#;
    let diags = get_diagnostics(source);
    let ts1210: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 1210 && msg.contains("arguments") && msg.contains("class"))
        .collect();
    assert!(
        !ts1210.is_empty(),
        "expected TS1210 for `arguments` parameter in class; got: {diags:#?}"
    );
}

#[test]
fn eval_as_class_method_parameter_also_emits_ts1210() {
    // tsc routes both `eval` and `arguments` params to the class-strict
    // TS1210 message when the enclosing scope is a class — matching the
    // `parseClassDeclarationInStrictModeByDefaultInES6.ts` baseline where
    // line 5 (`eval` param) reports TS1210 just like line 4 (`arguments`).
    let source = r#"
class C {
    public bar(eval: any) { }
}
"#;
    let diags = get_diagnostics(source);
    let has_1210_eval = diags
        .iter()
        .any(|(code, msg)| *code == 1210 && msg.contains("'eval'"));
    assert!(
        has_1210_eval,
        "expected TS1210 for `eval` parameter inside class; got: {diags:#?}"
    );
}

#[test]
fn arguments_as_non_class_function_parameter_uses_ts1100() {
    // Outside a class body, `arguments` parameter continues to use TS1100 —
    // the TS1210 routing is class-specific.
    let source = r#"
"use strict";
function foo(arguments: any) { }
"#;
    let diags = get_diagnostics(source);
    let has_1100_arguments = diags
        .iter()
        .any(|(code, msg)| *code == 1100 && msg.contains("arguments"));
    assert!(
        has_1100_arguments,
        "expected TS1100 for `arguments` parameter outside class; got: {diags:#?}"
    );
}
