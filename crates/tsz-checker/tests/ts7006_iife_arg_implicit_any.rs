//! Tests for TS7006 ("Parameter X implicitly has an 'any' type") emission for
//! function expressions inside IIFE arguments.
//!
//! Regression for `contextuallyTypedIifeStrict.ts`: when an IIFE argument is
//! an object literal whose property value is a function expression with an
//! unannotated parameter, tsc emits TS7006 for that parameter. tsz used to
//! suppress it because the object-literal type-refresh path cleared all
//! implicit-any diagnostics in the function's parameter span whenever the
//! refresh saw "a concrete contextual type" — even when that contextual type
//! was the property's own self-referential function shape, which doesn't
//! actually contextually type the parameter.

use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// `function(n)` lives inside the object literal `{ a: function(n) {…} }`,
/// which is the argument to `(o => o.a(11))`. The IIFE's `o` parameter has no
/// annotation, so the object literal isn't actually contextually typed. tsc
/// emits TS7006 for `n` because the property's "contextual type" is just the
/// object literal's own inferred shape, which doesn't constrain `n`.
#[test]
fn ts7006_for_function_param_inside_iife_object_literal_arg() {
    let source = r#"
let eleven = (o => o.a(11))({ a: function(n) { return n; } });
"#;
    let diags = diagnostics_for(source);
    let ts7006_n: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 7006 && msg.contains("'n'"))
        .collect();
    assert!(
        !ts7006_n.is_empty(),
        "expected TS7006 for parameter 'n' inside the IIFE arg's function expression; got: {diags:?}"
    );
}

/// Sanity: when a property value's function expression DOES have a real
/// contextual type (annotated variable), the existing clear-stale logic
/// should still suppress TS7006. This pins down the asymmetry — the fix must
/// not regress the genuine contextual-typing path.
#[test]
fn no_ts7006_for_function_param_with_real_contextual_type() {
    let source = r#"
let f: { a: (n: number) => number } = { a: function(n) { return n; } };
"#;
    let diags = diagnostics_for(source);
    let ts7006_n: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 7006 && msg.contains("'n'"))
        .collect();
    assert!(
        ts7006_n.is_empty(),
        "TS7006 must NOT fire for 'n' when the property has a real `(n: number) => number` contextual type; got: {ts7006_n:?}"
    );
}
