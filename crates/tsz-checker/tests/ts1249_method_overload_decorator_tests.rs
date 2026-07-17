//! Tests for TS1249 "A decorator can only decorate a method implementation,
//! not an overload." — tsc's `checkGrammarDecorators` reports this when a
//! decorator is applied to a body-less method (an overload signature), the same
//! `!nodeCanBeDecorated` path as an abstract method.

use tsz_checker::test_utils::check_source_codes_experimental_decorators;

#[test]
fn decorator_on_method_overload_reports_ts1249() {
    // The decorated overload signature (no body) is TS1249; the following
    // implementation is undecorated. Vary the method/decorator names to keep the
    // check structural, not name-scoped.
    for source in [
        r#"
declare function dec(...a: any[]): any;
class Store {
    @dec
    method(): void;
    method() {}
}
"#,
        r#"
declare function guard(...a: any[]): any;
class Widget {
    @guard
    handler(x: number): void;
    handler(x: number) {}
}
"#,
    ] {
        let codes = check_source_codes_experimental_decorators(source).to_vec();
        assert!(
            codes.contains(&1249),
            "expected TS1249 for a decorated method overload: {codes:?}"
        );
    }
}

#[test]
fn decorator_on_method_implementation_is_not_ts1249() {
    // A decorated method WITH a body is a valid implementation — no TS1249.
    let source = r#"
declare function dec(...a: any[]): any;
class Store {
    @dec
    method() {}
}
"#;
    let codes = check_source_codes_experimental_decorators(source).to_vec();
    assert!(
        !codes.contains(&1249),
        "a decorated method implementation must not report TS1249: {codes:?}"
    );
}
