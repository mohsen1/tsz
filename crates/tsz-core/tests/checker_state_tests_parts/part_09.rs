// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// Test that TS2307 is emitted for dynamic imports with unresolved module specifiers
#[test]
fn test_ts2307_dynamic_import_unresolved() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
async function loadModule() {
    const mod = await import("./missing-dynamic-module");
    return mod;
}
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    let module_diag = checker.ctx.diagnostics.iter().find(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            || d.code
                == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
    });

    assert!(
        module_diag.is_some(),
        "Expected TS2307 or TS2792 diagnostic for dynamic import, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    let diag = module_diag.unwrap();
    assert!(
        diag.message_text.contains("./missing-dynamic-module"),
        "Module-not-found message should contain module specifier, got: {}",
        diag.message_text
    );
}

/// Test that TS2307 is NOT emitted for dynamic imports with non-string specifiers
/// (e.g., variables or template literals cannot be statically checked)
#[test]
fn test_ts2307_dynamic_import_non_string_specifier_no_error() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
async function loadModule(modulePath: string) {
    const mod = await import(modulePath);
    return mod;
}
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Dynamic specifiers cannot be statically checked, so no TS2307 should be emitted
    let ts2307_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .count();

    assert_eq!(
        ts2307_count, 0,
        "Expected no TS2307 for dynamic import with variable specifier, got {ts2307_count} errors"
    );
}

#[test]
fn test_missing_type_reference_in_function_type_emits_2304() {
    use crate::parser::ParserState;

    let source = r#"
type Fn = (value: MissingType) => void;
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for unresolved type in function type, got: {codes:?}"
    );
}

#[test]
fn test_missing_property_access_emits_2339_not_2304() {
    use crate::parser::ParserState;

    let source = r#"
const obj = { value: 1 };
obj.missing;
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "Expected TS2339 for missing property access, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for missing property access, got: {codes:?}"
    );
}

#[test]
fn test_arguments_in_async_arrow_no_2304() {
    use crate::parser::ParserState;

    let source = r#"
function f() {
    return async () => arguments.length;
}

class C {
    method() {
        var fn = async () => arguments[0];
    }
}
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for 'arguments' in async arrow, got: {codes:?}"
    );
}

#[test]
fn test_ts2496_arguments_in_arrow_function_es5() {
    use crate::parser::ParserState;

    // TS2496: arguments cannot be referenced in an arrow function in ES5.
    let source = r#"
function f() {
    var a = () => arguments;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES5,
        strict: false,
        always_strict: false,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2496),
        "Expected TS2496 for 'arguments' in arrow function at ES5 target, got: {codes:?}"
    );
}

#[test]
fn test_ts2496_not_emitted_for_es2015_target() {
    use crate::parser::ParserState;

    // TS2496 should NOT fire when target is ES2015+ (arrow functions are native).
    let source = r#"
function f() {
    var a = () => arguments;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES2015,
        strict: false,
        always_strict: false,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2496),
        "TS2496 should not fire for ES2015 target, got: {codes:?}"
    );
}

#[test]
fn test_ts1100_arguments_in_strict_mode() {
    use crate::parser::ParserState;

    // TS1100: 'arguments' used as variable name in strict mode.
    let source = r#"
var arguments;
var a = () => arguments;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        target: tsz_common::common::ScriptTarget::ES5,
        strict: false,
        always_strict: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1100),
        "Expected TS1100 for 'var arguments' in strict mode, got: {codes:?}"
    );
    assert!(
        codes.contains(&2496),
        "Expected TS2496 for 'arguments' in arrow at ES5 target, got: {codes:?}"
    );
}

#[test]
fn test_signature_type_params_no_2304() {
    use crate::parser::ParserState;

    let source = r#"
interface BaseConstructor {
    new <T>(x: T): { value: T };
    new <T, U>(x: T, y: U): { x: T, y: U };
}
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for signature type params, got: {codes:?}"
    );
}

#[test]
fn test_extends_undefined_no_2304() {
    use crate::parser::ParserState;

    let source = r#"
class C extends undefined {}
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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for extends undefined, got: {codes:?}"
    );
}
