//! Locks in deep-property elaboration through paren/comma/assignment wrappers
//! on variable initializers. tsc anchors a single TS2322 at the deepest
//! mismatching leaf even when the initializer object literal sits behind
//! `( ... )`, `(void 0, { ... })`, or `prop = { ... }` wrappers.
//!
//! Regression: `slightlyIndirectedDeepObjectLiteralElaborations.ts` —
//! `const x: Foo = (void 0, { a: q = { b: ({ c: { d: 42 } }) } })` was
//! emitting an outer-anchored TS2322 with a one-level type display instead
//! of drilling to `d: 42` and reporting `Type 'number' is not assignable to
//! type 'string'.`.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
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
        CheckerOptions::default(),
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
fn comma_wrapped_deep_object_literal_drills_to_leaf() {
    let src = r#"
interface Foo {
    a: { b: { c: { d: string } } }
}
let q: Foo["a"] | undefined;
const x: Foo = (void 0, {
    a: q = {
        b: ({
            c: { d: 42 }
        })
    }
});
"#;
    let diags = diagnostics(src);
    let leaf = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        leaf.is_some(),
        "expected TS2322 with deep `number → string` leaf message, got: {diags:?}"
    );
    for (code, msg) in &diags {
        if *code == 2322 {
            assert!(
                !msg.contains("'{ b:"),
                "TS2322 should not anchor at outer property `a`'s shape — drill to leaf, got: {msg}"
            );
        }
    }
}

#[test]
fn paren_wrapped_object_literal_drills_to_property() {
    let src = r#"
interface T { a: string; }
const x: T = ({ a: 42 });
"#;
    let diags = diagnostics(src);
    let prop_level = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        prop_level.is_some(),
        "expected per-property TS2322 `number → string` through paren wrapper, got: {diags:?}"
    );
}

#[test]
fn assignment_in_property_value_drills_into_rhs() {
    let src = r#"
interface T { a: { b: string }; }
let q: { b: string } | undefined;
const x: T = { a: q = { b: 42 } };
"#;
    let diags = diagnostics(src);
    let leaf = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        leaf.is_some(),
        "expected TS2322 deep leaf through `q = (...)` assignment in value, got: {diags:?}"
    );
}
