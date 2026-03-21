//! Tests for lib-resolution stable identity path.
//!
//! These tests verify that lib type lowering uses the stable DefId identity
//! path (via `resolve_lib_node_in_arenas` + `get_or_create_def_id`) instead
//! of local per-call caches. They cover:
//!
//! - Promise and generic lib references resolve correctly with lib loaded.
//! - Generic lib types (Array, Map, Set) retain type parameters via stable DefId.
//! - Import type lowering for lib types.
//! - Cross-lib interface heritage (e.g., Array extends ReadonlyArray) works.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("scripts/emit/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../TypeScript/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/emit/node_modules/typescript/lib/lib.es2015.d.ts"),
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("lib.d.ts")
                .to_string();
            if !seen_files.insert(file_name.clone()) {
                continue;
            }
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }
    lib_files
}

fn lib_files_available() -> bool {
    !load_lib_files_for_test().is_empty()
}

fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn compile_with_lib(source: &str) -> Vec<(u32, String)> {
    compile_with_lib_and_options(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

fn compile_with_lib_and_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// ---- Promise / generic lib reference tests ----

#[test]
fn test_promise_resolve_with_lib_no_false_errors() {
    if !lib_files_available() {
        return;
    }
    // Basic Promise usage should not produce errors when lib is loaded
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(42);
async function f(): Promise<string> { return "hello"; }
"#,
    );
    // Filter out TS2318 (missing global type) which is acceptable
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !has_error(
            &real_errors
                .iter()
                .map(|&&(c, ref m)| (c, m.clone()))
                .collect::<Vec<_>>(),
            2322
        ),
        "Promise<number> should not produce TS2322 with lib loaded.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_generic_array_with_lib_retains_type_params() {
    if !lib_files_available() {
        return;
    }
    // Array<T> should be generic and retain its type parameter
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const first: number = arr[0];
// This should error: string is not assignable to number[]
const bad: Array<number> = ["a", "b"];
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Expected TS2322 for string[] assigned to number[].\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_map_generic_lib_reference_with_stable_identity() {
    if !lib_files_available() {
        return;
    }
    // Map<K,V> should resolve correctly with lib loaded
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("key", 42);
// This should error: boolean is not assignable to number
m.set("key", true);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2345),
        "Expected TS2345 for boolean argument to Map.set(string, number).\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_chaining_identity_stable() {
    if !lib_files_available() {
        return;
    }
    // Promise chaining should work with stable DefId identity
    let diagnostics = compile_with_lib(
        r#"
async function chain(): Promise<number> {
    const p = Promise.resolve(42);
    return p.then(x => x + 1);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    // Should not have type errors in basic Promise chaining
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Promise chaining should not produce type errors.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Generic lib type parameter resolution ----

#[test]
fn test_readonly_array_heritage_resolves() {
    if !lib_files_available() {
        return;
    }
    // ReadonlyArray<T> is a base of Array<T> - heritage should resolve
    let diagnostics = compile_with_lib(
        r#"
const arr: ReadonlyArray<number> = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "ReadonlyArray.length should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_partial_type_alias_lib_resolution() {
    if !lib_files_available() {
        return;
    }
    // Partial<T> is a utility type alias in lib - should resolve correctly
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; }
const partial: Partial<User> = { name: "Alice" };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Partial<User> should accept partial objects.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_record_type_alias_lib_resolution() {
    if !lib_files_available() {
        return;
    }
    // Record<K,T> is a utility type alias in lib - should resolve correctly
    let diagnostics = compile_with_lib(
        r#"
const rec: Record<string, number> = { a: 1, b: 2 };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Record<string, number> should accept object literals.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Cross-lib interface heritage ----

#[test]
fn test_array_inherits_from_readonly_array_concat() {
    if !lib_files_available() {
        return;
    }
    // Array should inherit from ReadonlyArray, making concat available
    let diagnostics = compile_with_lib(
        r#"
const arr = [1, 2, 3];
const result = arr.concat([4, 5]);
const len: number = result.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Array.concat and .length should be accessible via heritage.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_identity_across_multiple_references() {
    if !lib_files_available() {
        return;
    }
    // Multiple references to Promise should resolve to the same identity
    let diagnostics = compile_with_lib(
        r#"
function f(p1: Promise<number>, p2: Promise<number>): void {
    const p3: Promise<number> = p1;
    const p4: Promise<number> = p2;
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise<number> identity should be stable across references.\nDiagnostics: {real_errors:#?}"
    );
}
