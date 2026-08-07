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

// #16759: a declared object member's literal property widens in display when
// intersected with a tuple, because `source_carries_canonical_literal_member`
// probed `array_element_type`/`tuple_elements` before `intersection_members` —
// `get_tuple_elements` reduces a tuple-containing intersection down to just the
// most-specific tuple member for contextual typing, so the sibling object's
// properties were never reached and the source fell through to the
// non-literal-target widening fallback. An object-and-object intersection
// never hit this (no tuple/array probe to short-circuit on), which is why it
// stayed correct while the mixed shape didn't.
mod mixed_intersection_literal_property_display {
    use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

    fn assert_ts2322_source(source: &str, expected_substring: &str) {
        let diags = get_diagnostics(source);
        let ts2322 = diags
            .iter()
            .find(|(code, _)| *code == 2322)
            .map(|(_, message)| message.as_str())
            .unwrap_or_else(|| panic!("expected TS2322 diagnostic, got: {diags:?}"));
        assert!(
            ts2322.contains(expected_substring),
            "expected `{expected_substring}` in diagnostic, got: {ts2322}"
        );
    }

    #[test]
    fn object_first_numeric_literal_and_tuple() {
        assert_ts2322_source(
            r#"
declare const b: { z: 1 } & [string, ...[number, boolean]];
const x: number = b;
"#,
            "{ z: 1; } & [string, number, boolean]",
        );
    }

    #[test]
    fn tuple_first_numeric_literal() {
        assert_ts2322_source(
            r#"
declare const zq: { alpha: 1 } & [boolean, string];
const w: number = zq;
"#,
            "{ alpha: 1; } & [boolean, string]",
        );
    }

    #[test]
    fn object_second_position() {
        assert_ts2322_source(
            r#"
declare const zq: [boolean, string] & { alpha: 1 };
const w: number = zq;
"#,
            "{ alpha: 1; }",
        );
    }

    #[test]
    fn string_literal_property() {
        assert_ts2322_source(
            r#"
declare const b: { s: "hi" } & [number, boolean];
const x: number = b;
"#,
            r#"{ s: "hi"; } & [number, boolean]"#,
        );
    }

    #[test]
    fn boolean_literal_property() {
        assert_ts2322_source(
            r#"
declare const b: { flag: true } & [number, string];
const x: number = b;
"#,
            "{ flag: true; } & [number, string]",
        );
    }

    #[test]
    fn readonly_property() {
        assert_ts2322_source(
            r#"
declare const b: { readonly z: 1 } & [number, string];
const x: number = b;
"#,
            "{ readonly z: 1; } & [number, string]",
        );
    }

    #[test]
    fn optional_property() {
        assert_ts2322_source(
            r#"
declare const b: { z?: 1 } & [number, string];
const x: number = b;
"#,
            "{ z?: 1 | undefined; } & [number, string]",
        );
    }

    #[test]
    fn array_sibling_still_preserves_literal() {
        assert_ts2322_source(
            r#"
declare const b: { z: 1 } & number[];
const x: number = b;
"#,
            "{ z: 1; } & number[]",
        );
    }

    #[test]
    fn object_and_object_intersection_unaffected() {
        assert_ts2322_source(
            r#"
declare const c: { z: 1 } & { y: 2 };
const v: number = c;
"#,
            "{ z: 1; } & { y: 2; }",
        );
    }

    #[test]
    fn renamed_binder_control() {
        assert_ts2322_source(
            r#"
declare const wq: { alpha: 5 } & [string, ...[boolean, number]];
const zzz: string = wq;
"#,
            "{ alpha: 5; } & [string, boolean, number]",
        );
    }
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
