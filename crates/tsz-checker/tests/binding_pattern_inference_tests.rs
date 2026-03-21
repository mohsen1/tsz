use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
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
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn test_unknown_binding_patterns_match_tsc_split_diagnostics() {
    let source = r#"
declare function f<T>(): T;
const {} = f();
const { p1 } = f();
const [] = f();
const [e1, e2] = f();
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let codes: Vec<u32> = relevant.iter().map(|(code, _)| *code).collect();

    assert_eq!(
        codes,
        vec![2571, 2339, 2488, 2571, 2488],
        "Expected TypeScript-style unknown destructuring diagnostics. Actual diagnostics: {relevant:#?}"
    );
}
