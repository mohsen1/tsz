use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
        .collect()
}

#[test]
fn generic_wrapper_call_uses_contextual_return_literal() {
    let source = r#"
interface Wrap<T> {
    value: T;
}

declare function wrap<T>(value: T): Wrap<T>;
declare function box<T>(value: T): { value: T };

function wrappedFoo(): Wrap<'foo'> {
    return wrap('foo');
}

let boxedDraw: { value: 'win' | 'draw' } = box('draw');
"#;

    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "generic wrapper calls should preserve contextual literal returns: {diags:#?}"
    );
}

#[test]
fn generic_wrapper_call_uses_contextual_tuple_return() {
    let source = r#"
interface OK<T> {
    kind: "OK";
    value: T;
}

declare function ok<T>(value: T): OK<T>;

let result: OK<[string, number]> = ok(["hello", 12]);
"#;

    let diags = diagnostics(source);
    assert!(
        diags.is_empty(),
        "generic wrapper call should preserve contextual tuple return: {diags:#?}"
    );
}
