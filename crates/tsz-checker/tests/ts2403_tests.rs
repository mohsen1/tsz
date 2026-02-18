//! Tests for TS2403: Subsequent variable declarations must have the same type

use crate::CheckerState;
use tsz_binder::BinderState;
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn get_error_codes(source: &str) -> Vec<u32> {
    get_diagnostics(source)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

#[test]
fn test_var_any_then_number_ts2403() {
    // var x: any; var x = 2; should emit TS2403
    let diags = get_diagnostics("var x: any;\nvar x = 2;");
    eprintln!("TS2403 test diags: {:?}", diags);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for var x: any; var x = 2; but got: {:?}",
        diags
    );
}

#[test]
fn test_var_string_then_bare_ts2403() {
    // var y = ""; var y; should emit TS2403
    let diags = get_diagnostics("var y = \"\";\nvar y;");
    eprintln!("TS2403 bare test diags: {:?}", diags);
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for var y = ''; var y; but got: {:?}",
        diags
    );
}

#[test]
fn test_var_any_then_any_no_error() {
    // var z: any; var z; should NOT emit TS2403
    let codes = get_error_codes("var z: any;\nvar z;");
    assert!(
        !codes.contains(&2403),
        "Should NOT emit TS2403 for var z: any; var z; but got: {:?}",
        codes
    );
}
