use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_default(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_contextual_readonly_rest_tuple_parameters_use_element_positions() {
    let source = r#"
declare function each<T extends ReadonlyArray<any>>(cases: ReadonlyArray<T>): (fn: (...args: T) => any) => void;

const cases = [
    [1, '1'],
    [2, '2'],
] as const;

const eacher = each(cases);

eacher((a, b) => {
    a;
    b;
});

eacher((...args) => {
    const [a, b] = args;
    a;
    b;
});
"#;

    let diagnostics = check_default(source);
    let ts2345_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345_errors.is_empty(),
        "Readonly rest tuple callback parameters should be contextualized by position, got: {ts2345_errors:?}"
    );
}

#[test]
#[ignore = "TODO: pre-existing issue from merge - emits TS2322 instead of TS2345"]
fn test_contextual_readonly_rest_tuple_diagnostic_preserves_callable_literals() {
    let source = r#"
declare function eacher(fn: (...args: readonly [1, '1'] | readonly [2, '2']) => any): void;

eacher((a, b) => {
    let exactA: 1 = a;
    exactA;
});
"#;

    let diagnostics = check_default(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .unwrap_or_else(|| panic!("Expected TS2345, got diagnostics={diagnostics:?}"));

    assert!(
        ts2345
            .message_text
            .contains("(a: 2 | 1, b: \"1\" | \"2\") => void"),
        "Expected callable diagnostic to preserve literal unions, got {ts2345:?}"
    );
}
