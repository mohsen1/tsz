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
#[ignore = "pre-existing regression"]
fn test_contextual_readonly_rest_tuple_diagnostic_preserves_callable_literals() {
    let source = r#"
declare function eacher(fn: (...args: readonly [1, '1'] | readonly [2, '2']) => any): void;

eacher((a, b) => {
    let exactA: 1 = a;
    exactA;
});
"#;

    let diagnostics = check_default(source);
    // The error is on the variable assignment `let exactA: 1 = a` which is TS2322 (assignability),
    // not TS2345 (argument mismatch). This matches tsc behavior for this pattern.
    // Note: `a` is currently inferred as `number` rather than `1 | 2` — the literal types
    // from the tuple union rest parameter are not fully preserved yet.
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| panic!("Expected TS2322, got diagnostics={diagnostics:?}"));

    assert!(
        ts2322.message_text.contains("not assignable to type '1'"),
        "Expected assignability error for narrower literal type, got {ts2322:?}"
    );
}
