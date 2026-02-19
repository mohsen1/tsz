//! Tests for enum member visibility across merged declarations

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
fn test_merged_enum_member_visibility() {
    // Members from the first enum declaration should be visible in the second
    let codes = get_error_codes(
        r#"
enum E { a, b = a }
enum E { c = a }
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for 'a' in merged enum, got: {codes:?}"
    );
}

#[test]
fn test_merged_enum_export() {
    // Exported enum merging: members from prior declarations visible
    let codes = get_error_codes(
        r#"
export enum Animals { Cat = 1 }
export enum Animals { Dog = 2 }
export enum Animals { CatDog = Cat | Dog }
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for Cat/Dog in merged exported enum, got: {codes:?}"
    );
}

#[test]
fn test_enum_iife_initializer() {
    // IIFE in enum initializer should have its scope properly bound
    let codes = get_error_codes(
        r#"
enum E {
    A = (() => {
        function localFunction() { }
        var x: { (): void; };
        x = localFunction;
        return 0;
    })()
}
"#,
    );
    assert!(
        !codes.contains(&2304),
        "Should not emit TS2304 for locals inside IIFE in enum initializer, got: {codes:?}"
    );
}
