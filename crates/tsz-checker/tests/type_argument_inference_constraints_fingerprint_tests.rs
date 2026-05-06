use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
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
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect()
}

#[test]
fn invalid_explicit_type_arg_constraints_suppress_call_argument_cascades() {
    let source = r#"
function someGenerics1<T, U extends T>(n: T, m: number) { }
someGenerics1<string, number>(3, 4);

function someGenerics5<U extends number, T>(n: T, f: (x: U) => void) { }
someGenerics5<string, number>(null, null);
"#;

    let diagnostics = diagnostics(source);
    let ts2344 = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2344)
        .count();
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2345)
        .collect();

    assert_eq!(ts2344, 2, "expected one TS2344 for each bad type argument");
    assert!(
        ts2345.is_empty(),
        "invalid explicit type arguments should suppress same-call TS2345 cascades, got: {ts2345:#?}"
    );
}
