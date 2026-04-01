//! Tests for heritage clause type-only suppression behavior.
//!
//! TS1361/TS2693 should be suppressed in type-only contexts (interface extends,
//! declare class extends) but NOT in value contexts (non-ambient class extends).

use crate::context::{CheckerOptions, LibContext};
use crate::state::CheckerState;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::{BinderState, lib_loader::LibFile, state::LibContext as BinderLibContext};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.dom.d.ts"),
    ];

    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }
    lib_files
}

/// Non-ambient class extending a type-only symbol (interface) should emit TS2689,
/// NOT TS2693.  tsc uses the more specific TS2689 ("Cannot extend an interface")
/// in heritage clause context.
/// `class U extends I {}` where I is an interface → TS2689.
#[test]
fn class_extends_interface_emits_ts2689() {
    let source = r"
interface I { x: number; }
class U extends I {}
";
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

    // Should emit TS2689 (not TS2693) for class extending interface
    let ts2689_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2689)
        .count();
    assert!(
        ts2689_count >= 1,
        "Expected TS2689 for class extending interface, got {} errors: {:?}",
        ts2689_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    // TS2693 should NOT be emitted — tsc suppresses it in heritage clause context
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693 for class extending interface (TS2689 is sufficient), got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn class_extends_generic_interface_prefers_ts2689_over_ts2314() {
    let source = r"
interface I<T> { x: T; }
class U extends I {}
";
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

    let ts2689_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2689)
        .count();
    let ts2314_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2314)
        .count();

    assert!(
        ts2689_count >= 1,
        "Expected TS2689 for class extending generic interface, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ts2314_count,
        0,
        "Expected TS2689 to suppress redundant TS2314 in class heritage, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2314)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Interface extending another interface should NOT emit TS2693.
/// `interface Q extends I {}` → no error.
#[test]
fn interface_extends_interface_no_ts2693() {
    let source = r"
interface I { x: number; }
interface Q extends I {}
";
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

    // Should NOT emit TS2693 for interface extending interface
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693 for interface extends, got {}: {:?}",
        ts2693_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Declare class extending an interface should NOT emit TS2693.
/// `declare class B extends I {}` → no error (ambient context, no runtime code).
#[test]
fn declare_class_extends_interface_no_ts2693() {
    let source = r"
interface I { x: number; }
declare class B extends I {}
";
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

    // Should NOT emit TS2693 for declare class extends (ambient context)
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693 for declare class extends, got {}: {:?}",
        ts2693_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn qualified_heritage_missing_member_emits_namespace_specific_diagnostics() {
    let source = r"
namespace M {
    export interface E<T> { foo: T; }
}

class D extends M.C {}
interface I extends M.C {}
";
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

    let ts2708_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .count();
    let ts2694_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2694)
        .count();

    assert!(
        ts2708_count >= 1,
        "Expected TS2708 for class heritage namespace member value access, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        ts2694_count >= 1,
        "Expected TS2694 for interface heritage missing namespace member, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn instantiated_namespace_member_miss_keeps_property_access_diagnostic() {
    let source = r"
namespace M {
    class C {}
    class D extends M.C {}
}
";
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

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    let ts2708_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2708)
        .count();

    assert!(
        ts2339_count >= 1,
        "Expected TS2339 for missing member on instantiated namespace value, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ts2708_count,
        0,
        "Expected no TS2708 when namespace has a runtime value side, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Non-ambient class extending a type-only import should emit TS1361.
/// `import type { Foo } from './foo'; class U extends Foo {}` → TS1361.
#[test]
fn class_extends_type_only_import_emits_ts1361() {
    let source = r"
import type { Foo } from './foo';
class U extends Foo {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );

    checker.check_source_file(root);

    // Should emit TS1361 for class extending a type-only import
    let ts1361_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1361)
        .count();
    assert!(
        ts1361_count >= 1,
        "Expected TS1361 for class extending type-only import, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    // TS2693 should NOT be emitted
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Interface extending a type-only import should NOT emit TS1361.
/// `import type { Foo } from './foo'; interface Q extends Foo {}` → no TS1361.
#[test]
fn interface_extends_type_only_import_no_ts1361() {
    let source = r"
import type { Foo } from './foo';
interface Q extends Foo {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );

    checker.check_source_file(root);

    // Should NOT emit TS1361 (interface extends is a type-only context)
    let ts1361_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1361)
        .count();
    assert_eq!(
        ts1361_count,
        0,
        "Expected no TS1361 for interface extending type-only import, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 1361)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    // TS2693 should also NOT be emitted
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert_eq!(
        ts2693_count,
        0,
        "Expected no TS2693, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2693)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Declare class extending a type-only import should NOT emit TS1361.
/// `import type { Foo } from './foo'; declare class U extends Foo {}` → no TS1361.
#[test]
fn declare_class_extends_type_only_import_no_ts1361() {
    let source = r"
import type { Foo } from './foo';
declare class U extends Foo {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );

    checker.check_source_file(root);

    // Should NOT emit TS1361 (declare class extends is ambient context)
    let ts1361_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 1361)
        .count();
    assert_eq!(
        ts1361_count,
        0,
        "Expected no TS1361 for declare class extending type-only import, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 1361)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Generic constructor signatures behind a type alias should not trigger TS2315.
#[test]
fn call_expression_heritage_with_generic_constructor_signature_no_ts2315() {
    let lib_files = load_lib_files_for_test();
    let source = r"
interface Base<T, U> {
    x: T;
    y: U;
}

interface BaseConstructor {
    new (x: string, y: string): Base<string, string>;
    new <T>(x: T): Base<T, T>;
    new <T>(x: T, y: T): Base<T, T>;
    new <T, U>(x: T, y: U): Base<T, U>;
}

declare function getBase(): BaseConstructor;

class D2 extends getBase() <number> {}
class D3 extends getBase() <string, number> {}
class D4 extends getBase() <string, string, string> {}

interface BadBaseConstructor {
    new (x: string): Base<string, string>;
    new (x: number): Base<number, number>;
}

declare function getBadBase(): BadBaseConstructor;

    class D5 extends getBadBase() {}
";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics;
    let ts2315_count = diagnostics.iter().filter(|d| d.code == 2315).count();
    assert_eq!(
        ts2315_count,
        0,
        "Expected no TS2315 for generic base constructor signatures, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.code == 2315)
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2508_count = diagnostics.iter().filter(|d| d.code == 2508).count();
    assert!(
        ts2508_count >= 1,
        "Expected TS2508 for D4 constructor argument count, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2510_count = diagnostics.iter().filter(|d| d.code == 2510).count();
    assert!(
        ts2510_count >= 1,
        "Expected TS2510 for D5 return type mismatch, got diagnostics: {:?}",
        diagnostics
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
