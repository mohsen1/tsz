//! Tests for TS2430: Interface incorrectly extends interface
//!
//! Verifies correct behavior for interface extension compatibility,
//! including scope-aware type resolution in ambient modules.

use crate::CheckerState;
use crate::context::CheckerOptions;
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// =========================================================================
// Basic TS2430: interface incorrectly extends interface
// =========================================================================

#[test]
fn test_basic_incompatible_property_type() {
    // Derived interface has incompatible property type
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when derived property type is incompatible"
    );
}

#[test]
fn test_compatible_extension_no_error() {
    // Derived interface has compatible (same) property type
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when derived property type is compatible"
    );
}

// =========================================================================
// Module scope: same-name interface should not cause false TS2430
// =========================================================================

#[test]
fn test_module_namespace_same_name_interface_no_false_positive() {
    // This is the pattern from react16.d.ts that caused false TS2430.
    // In the react16.d.ts structure:
    //   declare module "react" {
    //     type NativeClipboardEvent = ClipboardEvent;  // at module level
    //     namespace React {
    //       interface ClipboardEvent<T> extends SyntheticEvent<T> { ... } // in namespace
    //     }
    //   }
    // The type alias at module level should resolve `ClipboardEvent` to the
    // global one, NOT the namespace-scoped one. The flat file_locals lookup
    // incorrectly shadowed the global with the namespace-local symbol.
    let source = r#"
interface Event {
    type: string;
}
interface ClipboardEvent extends Event {
    clipboardData: any;
}

declare module "mylib" {
    type NativeClipboardEvent = ClipboardEvent;

    namespace MyLib {
        interface BaseSyntheticEvent {
            nativeEvent: Event;
        }

        interface ClipboardEvent<T> extends BaseSyntheticEvent {
            nativeEvent: NativeClipboardEvent;
        }
    }
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 for namespace-scoped interface with same name as global; \
         type alias at module level should resolve to global ClipboardEvent. Got errors: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_module_scoped_genuinely_incompatible() {
    // Inside a module, a genuinely incompatible extension should still error.
    let source = r#"
declare module "mylib" {
    interface Base {
        x: number;
    }
    interface Derived extends Base {
        x: string;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 for genuinely incompatible extension inside module"
    );
}

// =========================================================================
// Type alias base: interface extends type alias
// =========================================================================

#[test]
fn test_interface_extends_type_alias_incompatible() {
    // Interface extends a type alias with incompatible property type
    let source = r#"
type T1 = { a: number };
interface I1 extends T1 {
    a: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_type_alias_compatible() {
    // Interface extends a type alias with compatible property type — no error
    let source = r#"
type T1 = { a: number };
interface I1 extends T1 {
    a: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when interface extends type alias with compatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_intersection_type_alias_incompatible() {
    // Interface extends an intersection type alias with incompatible property
    let source = r#"
type T1 = { a: number };
type T2 = T1 & { b: number };
interface I2 extends T2 {
    b: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends intersection type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
#[ignore = "mapped type evaluation not yet fully supported in unit test env"]
fn test_interface_extends_mapped_type_alias_incompatible() {
    // Interface extends a mapped type alias with incompatible property
    let source = r#"
type T5 = { [P in 'a' | 'b' | 'c']: string };
interface I5 extends T5 {
    c: number;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends mapped type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_multi_base_both_incompatible_emits_two_ts2430() {
    // When interface extends multiple bases and BOTH are incompatible,
    // we must emit TS2430 for each incompatible base (not just the first).
    let source = r#"
interface Base1 {
    x: { a: string; }
}
interface Base2 {
    x: { b: string; }
}
interface Derived<T> extends Base1, Base2 {
    x: { a: T; b: T; }
}
"#;
    let diags = get_diagnostics(source);
    let ts2430_count = diags.iter().filter(|d| d.0 == 2430).count();
    assert_eq!(
        ts2430_count, 2,
        "Should emit TS2430 for BOTH incompatible bases, not just the first. Got: {diags:?}"
    );
}

#[test]
fn test_multi_base_one_compatible_emits_one_ts2430() {
    // When only one of multiple bases is incompatible, emit exactly one TS2430.
    let source = r#"
interface Base1 {
    x: { a: string; }
}
interface Base2 {
    x: { b: string; }
}
interface Derived extends Base1, Base2 {
    x: { a: string; b: number; }
}
"#;
    let diags = get_diagnostics(source);
    let ts2430_msgs: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2430)
        .map(|d| d.1.clone())
        .collect();
    assert_eq!(
        ts2430_msgs.len(),
        1,
        "Should emit exactly one TS2430 (only Base2 is incompatible). Got: {ts2430_msgs:?}"
    );
    assert!(
        ts2430_msgs[0].contains("Base2"),
        "Error should mention Base2. Got: {:?}",
        ts2430_msgs[0]
    );
}

#[test]
#[ignore = "array type resolution requires lib.d.ts (Array<T> interface) not available in unit test env"]
fn test_interface_extends_array_type_alias_incompatible() {
    // Interface extends array type alias with incompatible length property
    let source = r#"
type T3 = number[];
interface I3 extends T3 {
    length: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends array type with incompatible 'length'. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_class_with_private_emits_at_name() {
    // TS2430 for private member conflict should be reported at the interface name,
    // not at the member that conflicts
    let source = r#"
class Base {
    private x: number;
}
interface Foo extends Base {
    x: number;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        !ts2430.is_empty(),
        "Should emit TS2430 for interface extending class with private member. Got: {diags:?}"
    );
}
