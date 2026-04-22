use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::TypeInterner;

#[test]
fn test_variable_redeclaration_incompatible() {
    let source = r#"
var x: number = 1;
var x: string = "string"; // TS2403
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for incompatible redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_compatible() {
    let source = r#"
var x: number = 1;
var x: number = 2; // OK
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2403),
        "Unexpected TS2403 for compatible redeclaration, got: {codes:?}"
    );
}

/// Helper to check a TypeScript source and return diagnostic codes.
fn check_and_get_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// Regression test for indexSignatureTypeInference conformance failure.
///
/// When `stringMapToArray(numberMap)` is called, T should be inferred as `unknown`
/// (not `Function`) because `NumberMap<Function>` has a number index signature but
/// `StringMap<T>` requires a string index signature. The return type `unknown[]`
/// differs from the prior `Function[]` declaration, triggering TS2403.
#[test]
#[ignore = "merged backlog: needs tsc-compatible failed generic-call inference to also surface TS2403"]
fn test_index_signature_type_inference_ts2403() {
    let source = r#"
interface NumberMap<T> {
    [index: number]: T;
}
interface StringMap<T> {
    [index: string]: T;
}
declare function numberMapToArray<T>(object: NumberMap<T>): T[];
declare function stringMapToArray<T>(object: StringMap<T>): T[];
declare var numberMap: NumberMap<Function>;
declare var stringMap: StringMap<Function>;
var v1: Function[];
var v1 = numberMapToArray(numberMap);
var v1 = numberMapToArray(stringMap);
var v1 = stringMapToArray(numberMap);
var v1 = stringMapToArray(stringMap);
"#;
    let codes = check_and_get_codes(source);
    // Line "var v1 = stringMapToArray(numberMap);" should produce:
    // TS2345: Argument of type 'NumberMap<Function>' is not assignable to parameter of type 'StringMap<unknown>'
    // TS2403: Subsequent variable declarations must have the same type (Function[] vs unknown[])
    assert!(
        codes.contains(&2345),
        "Expected TS2345 for incompatible argument, got: {codes:?}"
    );
    assert!(
        codes.contains(&2403),
        "Expected TS2403 for redeclaration with different inferred type (Function[] vs unknown[]), got: {codes:?}"
    );
}
