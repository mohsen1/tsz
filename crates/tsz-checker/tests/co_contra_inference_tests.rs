use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn contravariant_callable_alias_union_does_not_produce_ts2345() {
    let source = r#"
type Func1<T> = (x: T) => void;
type Func2<T> = ((x: T) => void) | undefined;

declare let f1: Func1<string>;
declare let f2: Func1<"a">;

declare function foo<T>(f1: Func1<T>, f2: Func1<T>): void;

foo(f1, f2);

declare let g1: Func2<string>;
declare let g2: Func2<"a">;

declare function bar<T>(g1: Func2<T>, g2: Func2<T>): void;

bar(f1, f2);
bar(g1, g2);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.iter().all(|(code, _)| *code != 2345),
        "Callable alias unions should keep contravariant inference and avoid TS2345. Actual diagnostics: {relevant:#?}"
    );
}
