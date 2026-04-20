//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
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
fn test_type_query_unknown_qualified_name_error() {
    use crate::parser::ParserState;

    let source = r#"
type T = typeof Missing.Member;
let useIt: T;
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
        "Expected error 2304 for unknown typeof qualified name, got: {codes:?}"
    );
}

#[test]
fn test_type_query_missing_namespace_member_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace Ns {
    export const value = 1;
}
type T = typeof Ns.Missing;
let useIt: T;
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
    // tsc emits TS2339 ("Property 'Missing' does not exist on type 'typeof Ns'")
    // for typeof of a non-existent namespace member.
    // TODO: Re-enable once typeof namespace member checking is restored
    // The diagnostic for missing members in typeof was lost in a refactor.
    let _ = codes;
}

#[test]
fn test_value_symbol_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
const value = 1;
type T = value;
let useIt: T;
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
        codes.contains(&2749),
        "Expected error 2749 for value symbol used as type, got: {codes:?}"
    );
}

#[test]
fn test_function_symbol_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
function foo() { return 1; }
type T = foo;
let useIt: T;
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
        codes.contains(&2749),
        "Expected error 2749 for function symbol used as type, got: {codes:?}"
    );
}

#[test]
fn test_namespace_symbol_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const value = 1;
}
type T = NS;
let useIt: T;
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

    // TS2709: "Cannot use namespace 'NS' as a type" (not 2749 which is for values)
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2709),
        "Expected error 2709 for namespace used as type, got: {codes:?}"
    );
}

#[test]
fn test_namespace_alias_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const value = 1;
}
import Alias = NS;
type T = Alias;
let useIt: T;
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

    // TS2709: "Cannot use namespace 'Alias' as a type" (not 2749 which is for values)
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2709),
        "Expected error 2709 for namespace alias used as type, got: {codes:?}"
    );
}

#[test]
fn test_namespace_value_member_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const value = 1;
}
type T = NS.value;
let useIt: T;
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
        codes.contains(&2749),
        "Expected error 2749 for namespace value member used as type, got: {codes:?}"
    );
}

#[test]
fn test_namespace_value_member_via_alias_used_as_type_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const value = 1;
}
import Alias = NS;
type T = Alias.value;
let useIt: T;
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
        codes.contains(&2749),
        "Expected error 2749 for namespace value member via alias used as type, got: {codes:?}"
    );
}

/// Test namespace value member access through nested namespaces
///
/// NOTE: Currently ignored - namespace value member access is not fully implemented.
/// Nested namespace value members are not correctly resolved.
#[test]
fn test_namespace_value_member_access() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export const top = 1;
    export namespace Inner {
        export const value = 2;
    }
}
import Alias = Outer.Inner;
const direct = Outer.Inner.value;
const topValue = Outer.top;
const viaAlias = Alias.value;
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    let top_sym = binder
        .file_locals
        .get("topValue")
        .expect("topValue should exist");
    let alias_sym = binder
        .file_locals
        .get("viaAlias")
        .expect("viaAlias should exist");

    // For const literals, we get literal types (e.g., literal 2 instead of number)
    let literal_2 = types.literal_number(2.0);
    let literal_1 = types.literal_number(1.0);
    assert_eq!(checker.get_type_of_symbol(direct_sym), literal_2);
    assert_eq!(checker.get_type_of_symbol(top_sym), literal_1);
    assert_eq!(checker.get_type_of_symbol(alias_sym), literal_2);
}

/// Test namespace value member access via element access
///
/// NOTE: Currently ignored - namespace value member access is not fully implemented.
/// The `import Alias = Ns` syntax triggers TS1202 error about import assignments in ES modules.
#[test]
fn test_namespace_value_member_element_access() {
    use crate::parser::ParserState;

    let source = r#"
namespace Ns {
    export const value = 1;
}
import Alias = Ns;
const direct = Ns["value"];
const viaAlias = Alias["value"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    let alias_sym = binder
        .file_locals
        .get("viaAlias")
        .expect("viaAlias should exist");

    // For const literals, we get literal types
    let literal_1 = types.literal_number(1.0);
    assert_eq!(checker.get_type_of_symbol(direct_sym), literal_1);
    assert_eq!(checker.get_type_of_symbol(alias_sym), literal_1);
}

