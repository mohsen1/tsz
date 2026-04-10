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
fn assignability_intersection_preserves_fresh_object_literal_display() {
    let diags = get_diagnostics(
        r#"
interface Foo {
    fooProp: "hello" | "world";
}

interface Bar {
    barProp: string;
}

interface FooBar extends Foo, Bar {}

declare function mixBar<T>(obj: T): T & Bar;

let fooBar: FooBar = mixBar({
    fooProp: "frizzlebizzle"
});
"#,
    );

    let ts2322 = diags
        .iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .expect("expected TS2322 diagnostic");

    assert!(
        ts2322.contains("{ fooProp: \"frizzlebizzle\"; } & Bar"),
        "Expected fresh literal in intersection display, got: {ts2322}"
    );
    assert!(
        !ts2322.contains("{ fooProp: string; } & Bar"),
        "Did not expect widened fresh object member in intersection display, got: {ts2322}"
    );
}
