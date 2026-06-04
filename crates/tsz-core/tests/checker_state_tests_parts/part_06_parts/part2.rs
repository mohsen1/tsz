/// Test that variable redeclaration with array spread doesn't emit TS2403
///
/// NOTE: Currently ignored - variable redeclaration detection with array spread is not
/// fully implemented. The checker incorrectly emits TS2403 for redeclarations when
/// array spread is involved.
#[test]
fn test_variable_redeclaration_array_spread_no_2403() {
    let source = r#"
function f1() {
    var a = [1, 2, 3];
    var b = ["hello", ...a, true];
    var b: (string | number | boolean)[];
}
"#;

    let (parser, root) = parse_test_source(source);

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
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for array spread redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_variable_redeclaration_inferred_vs_annotated_no_2403() {
    // Test that inferred type from initializer matches explicit annotation
    // Based on conformance test: ambientDeclarationsExternal.ts pattern
    let source = r#"
var n = 42;
var n: number;
"#;

    let (parser, root) = parse_test_source(source);

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
    let error_2403_count = codes.iter().filter(|&&c| c == 2403).count();

    assert_eq!(
        error_2403_count, 0,
        "Expected no error 2403 for inferred vs annotated redeclaration, got: {codes:?}"
    );
}

#[test]
fn test_namespace_member_not_found() {
    let source = r#"
namespace foo {
    export class Provide {}
}
var p: foo.NotExist;
"#;

    let (parser, root) = parse_test_source(source);

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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce error 2694: Namespace 'foo' has no exported member 'NotExist'
    assert!(
        codes.contains(&2694),
        "Expected error 2694 for namespace member not found, got: {codes:?}"
    );
}

#[test]
fn test_namespace_value_member_missing_errors() {
    let source = r#"
namespace NS {
    export const ok = 1;
}
import Alias = NS;
const bad = NS.missing;
const badAlias = Alias.missing;
"#;

    let (parser, root) = parse_test_source(source);

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
    let missing_count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        missing_count, 2,
        "Expected two 2339 errors for missing namespace value members, got: {codes:?}"
    );
}

/// Test import alias type resolution
///
/// NOTE: Currently ignored - import alias type resolution is not fully implemented.
/// The `import Alias = NS.Exported` syntax triggers TS1202 error about import assignments
/// in ES modules.
#[test]
fn test_import_alias_type_resolution() {
    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.Exported;
var x: Alias;
var y: NS.Exported;
"#;

    let (parser, root) = parse_test_source(source);

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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce no errors - both x: Alias and y: NS.Exported should resolve correctly
    assert!(
        codes.is_empty(),
        "Expected no errors for import alias type resolution, got: {codes:?}"
    );
}

#[test]
fn test_import_alias_non_exported_member() {
    let source = r#"
namespace NS {
    export class Exported {}
    class NotExported {}
}
import Alias = NS.NotExported;
var x: Alias;
"#;

    let (parser, root) = parse_test_source(source);

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

    let diags = &checker.ctx.diagnostics;
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();

    // Should produce error 2694 or 2724 (spelling suggestion variant):
    // Namespace 'NS' has no exported member 'NotExported' (Did you mean 'Exported'?)
    assert!(
        codes.contains(&2694) || codes.contains(&2724),
        "Expected error 2694 or 2724 for import alias of non-exported member, got: {codes:?}"
    );
}

#[test]
fn test_import_type_value_usage_errors() {
    let source = r#"
import type { Foo } from "./types";
Foo;
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions {
            module: crate::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS1361: 'Foo' cannot be used as a value because it was imported using 'import type'.
    assert!(
        codes.contains(&1361),
        "Expected TS1361 for type-only import used as value, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1148),
        "Should not emit TS1148 (module=none error) for import type test, got: {codes:?}"
    );
}

#[test]
fn test_numeric_enum_open_and_nominal_assignability() {
    let source = r#"
enum A { X, Y }
enum B { X, Y }
let a: A = 1;
let n: number = a;
let b: B = a;
"#;

    let (parser, root) = parse_test_source(source);

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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 1,
        "Expected one 2322 error for cross-enum assignment, got: {codes:?}"
    );
}
