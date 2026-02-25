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
