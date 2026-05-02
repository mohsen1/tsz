//! Tests for TS2322 source-display widening against `undefined` / `null` targets.
//!
//! Mirrors tsc's `getBaseTypeOfLiteralTypeForComparison`: when a TS2322
//! diagnostic is emitted with target `undefined` / `null`, the source side of
//! the message widens string / number / bigint literals to their primitive
//! base. Boolean keyword sources (`true` / `false`) are preserved because
//! TypeScript treats them as first-class types in declarations.
//!
//! Conformance test: `destructuringParameterDeclaration1ES5.ts`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts2322(diags: &[(u32, String)]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|(code, msg)| (*code == 2322).then_some(msg.as_str()))
        .collect()
}

#[test]
fn ts2322_widens_number_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
function f(z = [undefined]) {}
f([1]);
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'number'") && m.contains("'undefined'")),
        "expected widened 'number' against 'undefined', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Type '1'")),
        "literal '1' should have widened, got: {msgs:?}"
    );
}

#[test]
fn ts2322_widens_string_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
function f(z = [undefined]) {}
f(["abc"]);
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'string'") && m.contains("'undefined'")),
        "expected widened 'string' against 'undefined', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Type '\"abc\"'")),
        "literal '\"abc\"' should have widened, got: {msgs:?}"
    );
}

#[test]
fn ts2322_widens_number_literal_against_null_target() {
    let diags = compile_diagnostics(
        r#"
function f(z = [null]) {}
f([42]);
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'number'") && m.contains("'null'")),
        "expected widened 'number' against 'null', got: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("Type '42'")),
        "literal '42' should have widened, got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_true_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
function f(z = [undefined]) {}
f([true]);
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'true'") && m.contains("'undefined'")),
        "expected preserved 'true' against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_false_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
function f(z = [undefined]) {}
f([false]);
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'false'") && m.contains("'undefined'")),
        "expected preserved 'false' against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_string_literal_against_string_literal_target() {
    // Same-primitive literal targets (literal target side) keep the literal
    // surface so the diff is informative.
    let diags = compile_diagnostics(
        r#"
let x: "a" = "b";
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '\"b\"'") && m.contains("'\"a\"'")),
        "expected literal '\"b\"' kept against literal '\"a\"', got: {msgs:?}"
    );
}
