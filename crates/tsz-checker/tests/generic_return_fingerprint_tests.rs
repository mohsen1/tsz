use tsz_checker::context::CheckerOptions;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code != 2318)
    .map(|d| (d.code, d.message_text))
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
