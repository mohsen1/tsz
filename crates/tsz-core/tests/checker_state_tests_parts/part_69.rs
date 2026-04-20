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
#[test]
fn test_ts1194_no_error_nested_in_declare_namespace() {
    // Nested namespace inside `declare namespace` is still ambient.
    let source = r#"
        declare namespace A {
            namespace B {
                function foo(): void;
                export { foo };
            }
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 for nested namespace in ambient context, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_block_within_namespace() {
    // When export declarations are inside a block `{}` within a namespace,
    // tsc reports context errors (TS1231-1235) but does NOT additionally
    // emit TS1194 or TS1319. The block-context diagnostics take priority.
    let source = r#"
        namespace P {
            {
                export { };
            }
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 for export in block within namespace, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 1319),
        "Should NOT emit TS1319 for export in block within namespace, got: {diagnostics:?}"
    );
}

// =============================================================================
// Reverse Mapped Type Modifier Preservation Tests
// =============================================================================

#[test]
fn test_reverse_mapped_type_preserves_optional_modifier() {
    // When inferring T from { readonly [P in keyof T]: T[P] }, the optional
    // modifier should be preserved from the source. This tests the fix in
    // constrain_reverse_mapped_type that reverses modifier directives.
    //
    // declare function clone<T>(obj: { readonly [P in keyof T]: T[P] }): T;
    // type Foo = { a?: number; readonly b: string; }
    // declare const foo: Foo;
    // let y = clone(foo);  // should NOT error (T = { a?: number, b: string })
    let source = r#"
        declare function clone<T>(obj: { readonly [P in keyof T]: T[P] }): T;
        type Foo = { a?: number; readonly b: string; }
        declare const foo: Foo;
        let y = clone(foo);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .count();
    assert_eq!(
        ts2345_count,
        0,
        "clone(foo) should NOT emit TS2345 — reverse mapped type inference \
         must preserve optional modifier from source. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_reverse_mapped_type_removes_added_readonly() {
    // When inferring T from { readonly [P in keyof T]: T[P] }, the readonly
    // modifier (which the mapped type adds) should be removed from T.
    //
    // declare function unreadonly<T>(obj: { readonly [P in keyof T]: T[P] }): T;
    // const x = unreadonly({ readonly a: 1, readonly b: "hello" });
    // x should have type { a: number, b: string } (without readonly)
    let source = r#"
        declare function unreadonly<T>(obj: { readonly [P in keyof T]: T[P] }): T;
        declare const input: { readonly a: number; readonly b: string; };
        let result = unreadonly(input);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .count();
    assert_eq!(
        error_count,
        0,
        "unreadonly(input) should NOT emit errors — reverse mapped type inference \
         must remove the readonly modifier that the mapped type adds. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_reverse_mapped_type_validate_preserves_optional() {
    // validate<T>(obj: { [P in keyof T]?: T[P] }): T
    // The mapped type adds optional (?), so reverse should REMOVE it.
    // Calling validate with { a: 1 } should infer T = { a: number } (required).
    let source = r#"
        declare function validate<T>(obj: { [P in keyof T]?: T[P] }): T;
        declare const partial: { a?: number; b: string; };
        let result = validate(partial);
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    crate::test_fixtures::merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .count();
    assert_eq!(
        error_count,
        0,
        "validate(partial) should NOT emit errors — reverse mapped type inference \
         must handle optional modifier correctly. Got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_abstract_class_5plus_missing_uses_ts2655_truncation() {
    // When 5+ abstract members are missing, TSC uses TS2655 (class declaration)
    // with "and N more" truncation instead of TS2654 (lists all).
    use crate::parser::ParserState;

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
    abstract m5(): number;
    abstract m6(): number;
}
class B extends A { }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2655),
        "Expected TS2655 for 5+ missing abstract members, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2654),
        "Should NOT use TS2654 when 5+ members are missing, got: {codes:?}"
    );

    // Check truncation message format
    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2655)
        .expect("TS2655 diagnostic should exist");
    assert!(
        msg.message_text.contains("and 2 more"),
        "TS2655 message should contain 'and 2 more', got: {}",
        msg.message_text
    );
    assert!(
        msg.message_text.contains("'m1'") && msg.message_text.contains("'m4'"),
        "TS2655 should list first 4 members, got: {}",
        msg.message_text
    );
}

#[test]
fn test_abstract_class_expression_5plus_missing_uses_ts2650() {
    // When 5+ abstract members are missing on a class expression, TSC uses TS2650.
    use crate::parser::ParserState;

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
    abstract m5(): number;
}
const C = class extends A {};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2650),
        "Expected TS2650 for 5+ missing abstract members on class expression, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2656),
        "Should NOT use TS2656 when 5+ members are missing, got: {codes:?}"
    );

    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2650)
        .expect("TS2650 diagnostic should exist");
    assert!(
        msg.message_text.contains("and 1 more"),
        "TS2650 message should contain 'and 1 more', got: {}",
        msg.message_text
    );
}

#[test]
fn test_abstract_class_4_missing_still_uses_ts2654() {
    // When 4 or fewer abstract members are missing, TSC uses TS2654 (lists all).
    use crate::parser::ParserState;

    let source = r#"
abstract class A {
    abstract m1(): number;
    abstract m2(): number;
    abstract m3(): number;
    abstract m4(): number;
}
class B extends A { }
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2654),
        "Expected TS2654 for 4 or fewer missing abstract members, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2655),
        "Should NOT use TS2655 when <=4 members are missing, got: {codes:?}"
    );

    let msg = checker
        .ctx
        .diagnostics
        .iter()
        .find(|d| d.code == 2654)
        .expect("TS2654 diagnostic should exist");
    assert!(
        !msg.message_text.contains("more"),
        "TS2654 should list all members without truncation, got: {}",
        msg.message_text
    );
}

#[test]
fn test_abstract_constructor_emits_ts1242() {
    // 'abstract' on a constructor should emit TS1242, not TS1244.
    // TSC anchors at the 'abstract' keyword.
    use crate::parser::ParserState;

    let source = r#"
abstract class A {
    abstract constructor() {}
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1242),
        "Expected TS1242 for abstract constructor, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1244),
        "Should NOT emit TS1244 for abstract constructor, got: {codes:?}"
    );
}

#[test]
fn test_generic_interface_implements_ts2416() {
    // Test: genericSpecializations1.ts
    // Interface method has its own type param <T> shadowing the interface's T.
    // Non-generic implementations are NOT assignable to the generic method.
    let source = r#"
interface IFoo<T> {
    foo<T>(x: T): T;
}
class IntFooBad implements IFoo<number> {
    foo(x: string): string { return null; }
}
class StringFoo2 implements IFoo<string> {
    foo(x: string): string { return null; }
}
class StringFoo3 implements IFoo<string> {
    foo<T>(x: T): T { return null; }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count_2416 = codes.iter().filter(|&&c| c == 2416).count();
    assert!(
        count_2416 == 2,
        "Expected exactly 2 TS2416 errors (IntFooBad and StringFoo2), got {count_2416}: {codes:?}"
    );
}

// =============================================================================
// Stable identity: single-file heritage resolution in checker pre-population
// =============================================================================
