//! Tests for TS2540 readonly property assignment errors
//!
//! Verifies that assigning to readonly class properties emits TS2540.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_error_with_code(source: &str, code: u32) -> bool {
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

    checker.ctx.diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn test_readonly_class_property_assignment() {
    let source = r#"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x = 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly class property"
    );
}

#[test]
fn test_non_readonly_class_property_assignment_ok() {
    let source = r#"
class C {
    y: number = 2;
}
const c = new C();
c.y = 20;
"#;
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for non-readonly property"
    );
}
