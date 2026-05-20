//! Tests for TS6133 unused detection of ES private names (#-prefixed members).
//!
//! ES private names (`#foo`) are private by their `#` prefix, not by the
//! TypeScript `private` keyword. The unused detection must recognize both
//! kinds of privacy for class members under `noUnusedLocals`.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_with_no_unused_locals(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_unused_locals: true,
        ..Default::default()
    };

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

fn ts6133_names(diags: &[crate::diagnostics::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 6133)
        .filter_map(|d| {
            d.message_text
                .strip_prefix("'")
                .and_then(|s| s.split("'").next())
                .map(|s| s.to_string())
        })
        .collect()
}

#[test]
fn test_es_private_field_unused_detected() {
    // ES private field that is never read should trigger TS6133
    let diags = check_with_no_unused_locals(
        r#"
        export class A {
            #unused = "unused";
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"#unused".to_string()),
        "Expected TS6133 for #unused, got names: {names:?}"
    );
}

#[test]
fn test_es_private_field_used_not_flagged() {
    // ES private field that IS read in constructor should NOT trigger TS6133
    let diags = check_with_no_unused_locals(
        r#"
        export class A {
            #used = "used";
            constructor() {
                console.log(this.#used);
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"#used".to_string()),
        "Did not expect TS6133 for #used, got names: {names:?}"
    );
}

#[test]
fn test_es_private_method_unused_detected() {
    // ES private method that is never called should trigger TS6133
    let diags = check_with_no_unused_locals(
        r#"
        export class A {
            #unused() { }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"#unused".to_string()),
        "Expected TS6133 for #unused method, got names: {names:?}"
    );
}

#[test]
fn test_es_private_used_and_unused_together() {
    // Only the unused member should be flagged, not the used one
    let diags = check_with_no_unused_locals(
        r#"
        export class A {
            #used = "used";
            #unused = "unused";
            constructor() {
                console.log(this.#used);
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"#unused".to_string()),
        "Expected TS6133 for #unused, got names: {names:?}"
    );
    assert!(
        !names.contains(&"#used".to_string()),
        "Did not expect TS6133 for #used, got names: {names:?}"
    );
}

#[test]
fn test_ts_private_keyword_still_works() {
    // Ensure regular TS `private` keyword unused detection still works
    let diags = check_with_no_unused_locals(
        r#"
        export class A {
            private unused = "unused";
            private used = "used";
            constructor() {
                console.log(this.used);
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"unused".to_string()),
        "Expected TS6133 for private unused, got names: {names:?}"
    );
    assert!(
        !names.contains(&"used".to_string()),
        "Did not expect TS6133 for private used, got names: {names:?}"
    );
}

#[test]
fn test_private_static_used_via_bracket_not_flagged() {
    // A private static member accessed via bracket notation from within the
    // class (e.g. `Foo["m1"]()` in a public method) must NOT be reported as
    // unused.  The bracket access counts as a genuine read.
    let diags = check_with_no_unused_locals(
        r#"
        export class Foo {
            private static m1() {}
            public static test() {
                Foo["m1"]();
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"m1".to_string()),
        "Did not expect TS6133 for m1 accessed via bracket notation, got names: {names:?}"
    );
}

#[test]
fn test_private_static_property_used_via_bracket_not_flagged() {
    // A private static property accessed via bracket notation from a public
    // method must NOT be reported as unused.
    let diags = check_with_no_unused_locals(
        r#"
        export class Bar {
            private static p1 = 0;
            public static test() {
                Bar["p1"];
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"p1".to_string()),
        "Did not expect TS6133 for p1 accessed via bracket notation, got names: {names:?}"
    );
}

#[test]
fn test_private_static_self_recursive_via_bracket_flagged() {
    // A private static method that calls itself ONLY via bracket notation
    // is still considered unused by tsc — the self-recursive call does not
    // count as an external read.
    let diags = check_with_no_unused_locals(
        r#"
        export class Test4 {
            private static m2(n: number): number {
                return (n === 0) ? 1 : (n * Test4["m2"](n - 1));
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"m2".to_string()),
        "Expected TS6133 for self-recursive m2 (bracket only), got names: {names:?}"
    );
}

#[test]
fn test_private_static_self_recursive_via_dot_flagged() {
    // A private static method that calls itself ONLY via dot notation is also
    // considered unused — tsc matches this behaviour.
    let diags = check_with_no_unused_locals(
        r#"
        export class Test4 {
            private static m1(n: number): number {
                return (n === 0) ? 1 : (n * Test4.m1(n - 1));
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"m1".to_string()),
        "Expected TS6133 for self-recursive m1 (dot only), got names: {names:?}"
    );
}

#[test]
fn test_private_static_used_via_bracket_no_export_not_flagged() {
    // Non-exported (script-global) class: bracket access from a public static
    // method must NOT be reported as unused (same as the exported-class case).
    let diags = check_with_no_unused_locals(
        r#"
        class Test5 {
            private static m1() {}
            public static test() {
                Test5["m1"]();
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"m1".to_string()),
        "Did not expect TS6133 for m1 in non-exported class via bracket notation, got names: {names:?}"
    );
}

#[test]
fn test_private_static_property_used_via_bracket_no_export_not_flagged() {
    // Non-exported (script-global) class: bracket property access must NOT be
    // reported as unused.
    let diags = check_with_no_unused_locals(
        r#"
        class Test6 {
            private static p1 = 0;
            public static test() {
                Test6["p1"];
            }
        }
        "#,
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"p1".to_string()),
        "Did not expect TS6133 for p1 in non-exported class via bracket notation, got names: {names:?}"
    );
}
