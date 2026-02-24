//! Tests for TS1501: Regular expression flag target validation.
//!
//! TSC emits TS1501 when a regex flag requires a newer ECMAScript target than specified.
//! Flag requirements: u/y → es6, s → es2018, d → es2022, v → esnext.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics_with_target(source: &str, target: ScriptTarget) -> Vec<(u32, String)> {
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
        CheckerOptions {
            target,
            ..Default::default()
        },
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_ts1501(source: &str, target: ScriptTarget) -> bool {
    get_diagnostics_with_target(source, target)
        .iter()
        .any(|d| d.0 == 1501)
}

#[test]
fn u_flag_emits_ts1501_at_es5() {
    assert!(has_ts1501("var x = /foo/u;", ScriptTarget::ES5));
}

#[test]
fn u_flag_no_ts1501_at_es2015() {
    assert!(!has_ts1501("var x = /foo/u;", ScriptTarget::ES2015));
}

#[test]
fn y_flag_emits_ts1501_at_es5() {
    assert!(has_ts1501("var x = /foo/y;", ScriptTarget::ES5));
}

#[test]
fn y_flag_no_ts1501_at_es2015() {
    assert!(!has_ts1501("var x = /foo/y;", ScriptTarget::ES2015));
}

#[test]
fn s_flag_emits_ts1501_at_es2015() {
    assert!(has_ts1501("var x = /foo/s;", ScriptTarget::ES2015));
}

#[test]
fn s_flag_no_ts1501_at_es2018() {
    assert!(!has_ts1501("var x = /foo/s;", ScriptTarget::ES2018));
}

#[test]
fn d_flag_emits_ts1501_at_es2018() {
    assert!(has_ts1501("var x = /foo/d;", ScriptTarget::ES2018));
}

#[test]
fn d_flag_no_ts1501_at_es2022() {
    assert!(!has_ts1501("var x = /foo/d;", ScriptTarget::ES2022));
}

#[test]
fn gim_flags_never_emit_ts1501() {
    assert!(!has_ts1501("var x = /foo/gim;", ScriptTarget::ES3));
}

#[test]
fn message_uses_lowercase_target_name() {
    let diags = get_diagnostics_with_target("var x = /foo/u;", ScriptTarget::ES5);
    let msg = &diags.iter().find(|d| d.0 == 1501).unwrap().1;
    assert!(
        msg.contains("'es6'"),
        "Expected lowercase 'es6' in message, got: {msg}"
    );
}

#[test]
fn multiple_flags_emit_multiple_ts1501() {
    let diags = get_diagnostics_with_target("var x = /foo/us;", ScriptTarget::ES5);
    let count = diags.iter().filter(|d| d.0 == 1501).count();
    assert_eq!(
        count, 2,
        "Expected 2 TS1501 errors for 'u' and 's' flags at ES5"
    );
}
