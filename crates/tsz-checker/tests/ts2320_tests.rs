//! Tests for TS2320: Interface inherits conflicting declarations from base types.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostic_codes(source: &str) -> Vec<u32> {
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

fn has_error(source: &str, code: u32) -> bool {
    get_diagnostic_codes(source).contains(&code)
}

#[test]
fn ts2320_different_optionality() {
    // interface C has x?: number, interface C2 has x: number
    // interface A extends C, C2 should get TS2320
    let source = r#"
interface C {
    x?: number;
}
interface C2 {
    x: number;
}
interface A extends C, C2 {
    y: string;
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for different optionality"
    );
}

#[test]
fn ts2320_incompatible_types() {
    // Classic TS2320: same property name, incompatible types
    let source = r#"
interface Mover {
    move(): void;
    getStatus(): { speed: number; };
}
interface Shaker {
    shake(): void;
    getStatus(): { frequency: number; };
}
interface MoverShaker extends Mover, Shaker {
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for incompatible types"
    );
}

#[test]
fn ts2320_compatible_override_no_error() {
    // When the derived interface provides a compatible override, no TS2320
    let source = r#"
interface Mover {
    getStatus(): { speed: number; };
}
interface Shaker {
    getStatus(): { frequency: number; };
}
interface MoverShaker extends Mover, Shaker {
    getStatus(): { speed: number; frequency: number; };
}
"#;
    // TS2320 should still fire because the *inherited* members conflict,
    // even though the override resolves it. tsc reports TS2320 here.
    // Actually tsc does NOT report TS2320 when an override is provided.
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when override resolves conflict"
    );
}

#[test]
fn ts2320_same_optionality_no_error() {
    // Both optional — no conflict
    let source = r#"
interface A {
    x?: number;
}
interface B {
    x?: number;
}
interface C extends A, B {}
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when both are optional with same type"
    );
}
