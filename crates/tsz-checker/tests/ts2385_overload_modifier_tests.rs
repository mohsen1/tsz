//! Tests for TS2385: Overload signatures must all be public, private or protected.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
    ];
    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            lib_files.push(Arc::new(LibFile::from_source(file_name, content)));
        }
    }
    lib_files
}

fn get_error_codes(source: &str) -> Vec<u32> {
    let lib_files = load_lib_files_for_test();
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn ts2385_public_overload_private_impl() {
    let codes = get_error_codes("class C { public foo(): void; private foo(x?: any) { } }");
    assert!(codes.contains(&2385), "Expected TS2385, got: {codes:?}");
}

#[test]
fn ts2385_protected_overloads_private_impl() {
    let codes = get_error_codes(
        "class C {
            protected foo(x: string): void;
            protected foo(x: number): void;
            private foo(x: any) { }
        }",
    );
    let count = codes.iter().filter(|&&c| c == 2385).count();
    assert_eq!(count, 2, "Expected 2 TS2385 errors, got {count}: {codes:?}");
}

#[test]
fn ts2385_no_error_when_modifiers_match() {
    let codes =
        get_error_codes("class C { private foo(x: string): void; private foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Should NOT emit TS2385, got: {codes:?}"
    );
}

#[test]
fn ts2385_no_error_all_public() {
    let codes = get_error_codes("class C { public foo(x: string): void; public foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Should NOT emit TS2385, got: {codes:?}"
    );
}

#[test]
fn ts2385_static_methods_checked_separately() {
    let codes = get_error_codes(
        "class C {
            private foo(x: string): void;
            private foo(x: any) { }
            private static foo(x: string): void;
            public static foo(x: any) { }
        }",
    );
    let count = codes.iter().filter(|&&c| c == 2385).count();
    assert_eq!(
        count, 1,
        "Expected 1 TS2385 for static mismatch, got {count}: {codes:?}"
    );
}

#[test]
fn ts2385_implicit_public_matches_explicit_public() {
    let codes = get_error_codes("class C { foo(x: string): void; public foo(x: any) { } }");
    assert!(
        !codes.contains(&2385),
        "Implicit public should match explicit, got: {codes:?}"
    );
}
