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
