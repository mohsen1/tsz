//! Tests for TS2374: Duplicate index signature for type

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn test_duplicate_string_indexer() {
    let codes = get_error_codes(
        r#"
interface I {
    [x: string]: string;
    [x: string]: number;
}
"#,
    );
    let count = codes.iter().filter(|&&c| c == 2374).count();
    assert_eq!(
        count, 2,
        "Should emit 2 TS2374 for duplicate string indexers, got: {:?}",
        codes
    );
}

#[test]
fn test_duplicate_number_indexer() {
    let codes = get_error_codes(
        r#"
interface I {
    [x: number]: string;
    [x: number]: number;
}
"#,
    );
    let count = codes.iter().filter(|&&c| c == 2374).count();
    assert_eq!(
        count, 2,
        "Should emit 2 TS2374 for duplicate number indexers, got: {:?}",
        codes
    );
}

#[test]
fn test_no_error_single_indexer() {
    let codes = get_error_codes(
        r#"
interface I {
    [x: string]: string;
}
"#,
    );
    assert!(
        !codes.contains(&2374),
        "Should not emit TS2374 for single indexer, got: {:?}",
        codes
    );
}

#[test]
fn test_mixed_indexers_ok() {
    // One string and one number indexer is OK (not duplicates)
    let codes = get_error_codes(
        r#"
interface I {
    [x: string]: string;
    [x: number]: number;
}
"#,
    );
    assert!(
        !codes.contains(&2374),
        "Should not emit TS2374 for mixed string/number indexers, got: {:?}",
        codes
    );
}
