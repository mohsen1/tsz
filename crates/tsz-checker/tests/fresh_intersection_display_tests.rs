use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

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

#[test]
fn test_function_expression_generic_return_type_shows_type_args() {
    let source = r#"
interface Wrapper<T> {
    value: T;
}
var a = function wrap<U>(x: U): Wrapper<U> { return null; };
"#;
    let diags = get_diagnostics(source);
    let ts2322 = diags
        .iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg.as_str())
        .expect("Expected TS2322 for null not assignable to Wrapper<U>");
    assert!(
        ts2322.contains("Wrapper<U>"),
        "Expected 'Wrapper<U>' with type arg in error message, got: {ts2322}"
    );
}

#[test]
fn test_arrow_function_generic_return_type_shows_type_args() {
    let source = r#"
interface Wrapper<T> {
    value: T;
}
var a = <U>(x: U): Wrapper<U> => { return null; };
"#;
    let diags = get_diagnostics(source);
    let ts2322 = diags
        .iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg.as_str())
        .expect("Expected TS2322 for null not assignable to Wrapper<U>");
    assert!(
        ts2322.contains("Wrapper<U>"),
        "Expected 'Wrapper<U>' with type arg in arrow function error message, got: {ts2322}"
    );
}
