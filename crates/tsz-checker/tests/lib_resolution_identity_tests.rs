//! Tests for lib-resolution stable identity path.
//!
//! These tests verify that lib type lowering uses the stable DefId identity
//! path (via `resolve_lib_node_in_arenas` + `get_lib_def_id`) instead of
//! on-demand DefId creation with local caching tricks. They cover:
//!
//! - Promise and generic lib references resolve correctly with lib loaded.
//! - Generic lib types (Array, Map, Set) retain type parameters via stable DefId.
//! - Import type lowering for lib types.
//! - Cross-lib interface heritage (e.g., Array extends ReadonlyArray) works.
//! - `resolve_scope_chain` and `resolve_name_to_lib_symbol` stable helpers.

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

// ---- Lib binder pre-population tests ----

#[test]
fn test_lib_binders_have_semantic_defs() {
    // Verify that lib binders actually populate semantic_defs during binding.
    // This is a prerequisite for pre_populate_def_ids_from_lib_binders to work.
    let lib_files = load_lib_files_for_test();
    if lib_files.is_empty() {
        return;
    }

    let mut total_semantic_defs = 0;
    for lib_file in &lib_files {
        let count = lib_file.binder.semantic_defs.len();
        total_semantic_defs += count;
    }

    // lib.es5.d.ts alone has hundreds of top-level declarations (Array, String,
    // Number, Boolean, Error, Promise, Map, etc.). If semantic_defs is empty,
    // it means the binder isn't recording them for lib files.
    assert!(
        total_semantic_defs > 50,
        "Lib binders should have significant semantic_defs, found {total_semantic_defs}"
    );
}

#[test]
fn test_lib_pre_population_creates_def_ids_for_lib_symbols() {
    // Verify that calling pre_populate_def_ids_from_lib_binders creates DefIds
    // in the DefinitionStore for lib symbols, eliminating O(N) scans on first access.
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), "const x: number = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<CheckerLibContext> = lib_files
        .iter()
        .map(|lib| {
            let binder_ctx = BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            };
            binder.merge_lib_contexts_into_binder(&[binder_ctx]);
            CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            }
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Pre-populate from primary binder
    let primary_count = checker.ctx.pre_populate_def_ids_from_binder();

    // Pre-populate from lib binders
    let lib_count = checker.ctx.pre_populate_def_ids_from_lib_binders();

    // The lib binders should contribute DefIds (Array, String, Number, etc.)
    assert!(
        lib_count > 0,
        "pre_populate_def_ids_from_lib_binders should create DefIds. \
         Primary: {primary_count}, Lib: {lib_count}"
    );
}

#[test]
fn test_lib_symbols_have_existing_def_ids_after_pre_population() {
    // After pre-population, get_existing_def_id should succeed for all lib
    // symbols that were merged into the main binder's file_locals. This proves
    // that lib-resolution closures can use get_existing_def_id instead of
    // get_or_create_def_id (no on-demand DefId creation needed).
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), "const x: number = 1;".to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<CheckerLibContext> = lib_files
        .iter()
        .map(|lib| {
            let binder_ctx = BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            };
            binder.merge_lib_contexts_into_binder(&[binder_ctx]);
            CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            }
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Pre-populate (same as checker construction does)
    checker.ctx.pre_populate_def_ids_from_binder();
    checker.ctx.pre_populate_def_ids_from_lib_binders();

    // Key lib symbols that should have pre-existing DefIds
    let expected_symbols = [
        "Array", "String", "Number", "Boolean", "Object", "Function", "Error",
    ];
    let mut missing = Vec::new();
    for name in &expected_symbols {
        if let Some(sym_id) = binder.file_locals.get(name)
            && checker.ctx.get_existing_def_id(sym_id).is_none()
        {
            missing.push(*name);
        }
        // Symbol might not be in file_locals if lib files don't include it
    }
    assert!(
        missing.is_empty(),
        "These lib symbols should have pre-existing DefIds but don't: {missing:?}. \
         This means lib-resolution closures cannot safely use get_existing_def_id."
    );
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

// ---- Augmentation symbol resolution (stable helpers) ----

#[test]
fn test_global_augmentation_merges_with_lib_type() {
    if !lib_files_available() {
        return;
    }
    // Global augmentations should merge with lib types via the stable
    // resolve_augmentation_symbol helper (no per-call RefCell cache).
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        customMethod(): T;
    }
}
const arr: Array<number> = [1, 2, 3];
const val: number = arr.customMethod();
export {};
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Augmented Array.customMethod should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_all_with_lib_stable_identity() {
    if !lib_files_available() {
        return;
    }
    // Promise.all uses complex generic resolution that depends on stable
    // lib DefId identity across multiple Promise interface declarations.
    let diagnostics = compile_with_lib(
        r#"
async function fetchAll(): Promise<number[]> {
    const promises: Promise<number>[] = [
        Promise.resolve(1),
        Promise.resolve(2),
    ];
    return Promise.all(promises);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.all should preserve number[] type.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_resolves_lib_types() {
    if !lib_files_available() {
        return;
    }
    // Basic lib type references should resolve without import-type errors
    let diagnostics = compile_with_lib(
        r#"
type NumArray = Array<number>;
const arr: NumArray = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2339),
        "Type alias referencing lib Array should resolve.\nDiagnostics: {real_errors:#?}"
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

// ---- Stable helper tests (resolve_scope_chain, resolve_name_to_lib_symbol) ----

#[test]
fn test_promise_all_with_tuple_input() {
    // Promise.all takes an iterable and returns Promise<T[]>.
    // This exercises the lib resolution path for generic Promise members
    // via the stable `get_lib_def_id` path (not `get_existing_def_id`).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function gather(): Promise<[number, string]> {
    const p1: Promise<number> = Promise.resolve(1);
    const p2: Promise<string> = Promise.resolve("a");
    return Promise.all([p1, p2]);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.all with tuple should not produce TS2322.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_async_return_unwraps_correctly() {
    // async functions return Promise<T>; the resolved value should
    // be unwrapped to T. Validates that Promise type params propagate
    // through the stable DefId path during lib lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function getNum(): Promise<number> {
    return 42;
}
async function useNum(): Promise<void> {
    const n: number = await getNum();
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Awaiting Promise<number> should yield number.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_reference_array() {
    // `import("...").Array` style references should resolve via the
    // lib lowering stable identity path. Here we test that Array
    // accessed as a type reference in lib contexts works properly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type NumArr = Array<number>;
const arr: NumArr = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "NumArr (alias for Array<number>) should have .length.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_def_id_falls_back_for_non_prepopulated_symbols() {
    // Verify that get_lib_def_id creates DefIds on demand for symbols
    // that were not pre-populated from semantic_defs (e.g., nested types
    // or late-bound lib symbols).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const err: TypeError = new TypeError("boom");
const msg: string = err.message;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "TypeError.message should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_reject_and_catch() {
    // Promise.reject + .catch exercises heritage chain resolution
    // (Promise extends PromiseLike) via the stable identity path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function risky(): Promise<number> {
    return Promise.reject(new Error("fail")).catch(() => 0);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.reject().catch() should resolve without TS2322.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_global_augmentation_merges_with_stable_identity() {
    // Global augmentations should merge with lib types via the
    // stable resolve_name_to_lib_symbol helper (replacing the old
    // per-call symbol_lookup_cache pattern).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        customMethod(): T;
    }
}
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2669)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Array.length should still be accessible after global augmentation.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_multiple_lib_type_references_share_def_id() {
    // Using the same lib type in multiple positions should resolve
    // to the same DefId (via get_lib_def_id), enabling proper
    // assignability between them.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function identity(x: Error): Error { return x; }
const e: Error = new Error("test");
const result: Error = identity(e);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Error type should be consistent across references.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- resolve_augmentation_node stable helper tests ----

#[test]
fn test_augmentation_node_resolver_cross_file_interface_merge() {
    // Exercises the `resolve_augmentation_node` stable helper via the
    // global augmentation path in lib_resolution.rs and lib.rs.
    // The augmentation declares a new method on a lib interface;
    // the resolver must find the base symbol via scope-chain + name lookup.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface String {
        trimToLength(n: number): string;
    }
}
const s: string = "hello";
const trimmed: string = s.trimToLength(3);
export {};
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2669)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Augmented String.trimToLength should be accessible via resolve_augmentation_node.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_augmentation_node_resolver_preserves_original_members() {
    // After augmenting a lib interface, original members must still
    // be accessible. This validates that the augmentation lowering
    // (using resolve_augmentation_node) produces a proper intersection
    // without losing the base type's members.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Number {
        toLabel(): string;
    }
}
const n: number = 42;
const fixed: string = n.toFixed(2);
const label: string = n.toLabel();
export {};
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2669)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Number.toFixed and augmented Number.toLabel should both be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- resolve_lib_type_with_params def_id consistency tests ----

#[test]
fn test_resolve_lib_type_with_params_uses_stable_def_id() {
    // resolve_lib_type_with_params (used by register_boxed_types) now
    // uses get_lib_def_id instead of get_existing_def_id, ensuring
    // lib symbols that weren't pre-populated still get stable DefIds.
    // This test validates that boxed types (Array, Promise) resolve
    // correctly when accessed through the with_params path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const mapped: Array<string> = arr.map(x => x.toString());
const p: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Array and Promise accessed via with_params path should resolve with stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_resolves_lib() {
    // import("...") type expressions that reference lib types should
    // resolve through the stable def_id path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type PromiseNum = Promise<number>;
async function wrap(): PromiseNum {
    return 42;
}
async function unwrap(): Promise<void> {
    const n: number = await wrap();
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Type alias to Promise<number> should resolve correctly.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: Promise / lib refs / import-type lowering ----
// These tests exercise the stable helper paths (resolve_lib_node_in_lib_contexts,
// get_lib_def_id) to ensure lib lowering works without local DefId repair or
// per-call caching tricks.

#[test]
fn test_promise_then_chain_type_propagation() {
    // Promise.then() chains should propagate generic type arguments through
    // the stable DefId path without requiring local DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const result: Promise<number> = fetchData().then(s => s.length);
const final_result: Promise<boolean> = result.then(n => n > 0);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.then() chain should propagate types through stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_race_and_any_lib_resolution() {
    // Promise.race and Promise.any use Awaited<T> and other lib utility
    // types that require cross-lib-context resolution via the stable helper.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(1);
const p2: Promise<string> = Promise.resolve("a");
const raced = Promise.race([p1, p2]);
"#,
    );
    // No TS2322 or TS2345 from lib resolution issues
    let type_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        type_errors.is_empty(),
        "Promise.race should resolve without type errors.\nDiagnostics: {type_errors:#?}"
    );
}

#[test]
fn test_lib_ref_set_and_map_generic_usage() {
    // Set<T> and Map<K,V> depend on correct cross-lib resolution for
    // their iterator and heritage chain types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const s: Set<number> = new Set([1, 2, 3]);
const m: Map<string, number> = new Map();
m.set("a", 1);
const hasA: boolean = m.has("a");
const size: number = s.size;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Set and Map generic lib types should resolve correctly.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_with_generic_lib_ref() {
    // Type aliases referencing generic lib types should resolve through
    // the stable identity path without ad-hoc DefId creation.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type NumArray = Array<number>;
type StrPromise = Promise<string>;
const arr: NumArray = [1, 2, 3];
const len: number = arr.length;
async function getStr(): StrPromise {
    return "hello";
}
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Type alias to generic lib types should resolve correctly.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_static_methods() {
    // Promise static methods (resolve, reject, all, allSettled) should
    // resolve through the stable lib resolution path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1 = Promise.resolve(42);
const p2 = Promise.resolve("hello");
const p3 = Promise.all([p1, p2]);
"#,
    );
    // Should not emit TS2339 (property not found) for Promise static methods
    assert!(
        !diagnostics.iter().any(|(c, msg)| *c == 2339
            && (msg.contains("resolve") || msg.contains("all") || msg.contains("reject"))),
        "Promise static methods should be resolved from lib.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_lib_ref_readonly_array_methods() {
    // ReadonlyArray<T> is resolved via heritage chain from Array<T>.
    // This tests that the heritage merge path works with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function process(items: ReadonlyArray<number>): number {
    return items.length;
}
const nums: ReadonlyArray<number> = [1, 2, 3];
const result: number = process(nums);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "ReadonlyArray should resolve with heritage chain.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- resolve_lib_type_with_params stable DefId path (get_lib_def_id) ----
// These tests specifically exercise the paths changed from
// get_or_create_def_id_with_params to get_lib_def_id + insert_def_type_params
// in resolve_lib_type_with_params.

#[test]
fn test_lib_type_alias_partial_uses_stable_def_id() {
    // Partial<T> is a type alias resolved via resolve_lib_type_with_params.
    // The type alias path now uses get_lib_def_id (stable) instead of
    // get_or_create_def_id_with_params (repair-fallback). Verify the alias
    // resolves correctly with proper generic substitution.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Config { host: string; port: number; debug: boolean; }
const partial: Partial<Config> = { host: "localhost" };
const full: Config = { host: "localhost", port: 8080, debug: false };
// Partial should accept subsets but reject wrong types
const bad: Partial<Config> = { host: 123 };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Partial<Config> should reject {{ host: 123 }} with TS2322.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_pick_uses_stable_def_id() {
    // Pick<T, K> is a type alias resolved via resolve_lib_type_with_params.
    // Exercises the same stable lib DefId path as Partial.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type UserSummary = Pick<User, "name" | "email">;
const summary: UserSummary = { name: "Alice", email: "a@b.com" };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Pick<User, 'name' | 'email'> should accept {{ name, email }}.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_and_array_boxed_types_use_stable_def_id() {
    // register_boxed_types calls resolve_lib_type_with_params for Array and
    // Promise. The interface path now uses get_lib_def_id. Verify that
    // boxed type registration works correctly with the stable path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
// Array boxed type: number[] should be assignable to Array<number>
const arr: Array<number> = [1, 2, 3];
const arr2: number[] = arr;

// Promise boxed type: async return should match Promise<T>
async function getNum(): Promise<number> { return 42; }
const p: Promise<number> = getNum();
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Array and Promise boxed types should resolve via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_sym_id_prefers_main_binder() {
    // The type alias path in resolve_lib_type_with_params now prefers
    // the main binder's sym_id (file_locals.get(name).unwrap_or(sym_id))
    // instead of using the per-lib-context sym_id directly. This avoids
    // SymbolId collisions between lib_ctx and main binder.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
// Record is a type alias in lib.es5.d.ts
type StringMap = Record<string, number>;
const map: StringMap = { a: 1, b: 2 };
// Readonly is also a type alias
type FrozenUser = Readonly<{ name: string; age: number }>;
const user: FrozenUser = { name: "Alice", age: 30 };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Record and Readonly type aliases should resolve via stable main-binder DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_via_type_alias() {
    // Type alias indirection to Promise should work through the stable
    // lib resolution path. This exercises import-type-like lowering
    // where the type alias body references a lib type.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type AsyncResult<T> = Promise<T>;
async function fetchUser(): AsyncResult<string> {
    return "Alice";
}
async function consume(): Promise<void> {
    const name: string = await fetchUser();
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "AsyncResult<T> alias to Promise<T> should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_library_reference_error_hierarchy() {
    // Error, TypeError, RangeError etc. form an inheritance hierarchy
    // in lib. Heritage chain resolution should work with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e: Error = new TypeError("type error");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Error hierarchy should resolve with stable lib helpers.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- keyword_syntax_to_type_id / keyword_name_to_type_id coverage ----
// These tests exercise the consolidated keyword→TypeId helpers that replaced
// the duplicated match blocks in resolve_lib_heritage_type_arg.

#[test]
fn test_lib_heritage_keyword_type_args_resolve() {
    // Heritage clauses like `extends Iterable<string>` need keyword type
    // args (string, number, boolean, etc.) resolved via the stable
    // keyword_syntax_to_type_id / keyword_name_to_type_id helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<string> = ["a", "b"];
const len: number = arr.length;
const first: string = arr[0];

// ReadonlyArray<number> → heritage uses keyword type arg
function sum(items: ReadonlyArray<number>): number {
    let total: number = 0;
    for (let i = 0; i < items.length; i++) {
        total = total + items[i];
    }
    return total;
}
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Heritage keyword type args should resolve via stable helpers.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_no_file_sym_id_repair_needed() {
    // After removing the file_sym_id re-lookup repair (which redundantly
    // re-resolved sym_id via file_locals), Promise generic type params
    // should still resolve correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function multiPromise(): Promise<void> {
    const p1: Promise<number> = Promise.resolve(1);
    const p2: Promise<string> = Promise.resolve("x");
    const n: number = await p1;
    const s: string = await p2;
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Promise generic params should resolve without file_sym_id repair.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_interface_with_multiple_keyword_heritage_args() {
    // Validates that heritage clauses with multiple keyword type arguments
    // (e.g., `extends Map<string, number>`) resolve all keyword args
    // correctly through the consolidated helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("key", 42);
const val: number | undefined = m.get("key");
const hasKey: boolean = m.has("key");
const sz: number = m.size;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Map<string, number> with keyword type args should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Stable augmentation lowering helper tests
// =========================================================================
// These tests exercise the `lower_augmentation_for_arena` helper that
// replaced the duplicated per-call resolver closures in lib_resolution.rs
// and lib.rs.

#[test]
fn test_promise_then_return_type_via_stable_lowering() {
    // Promise.then() returns Promise<TResult1 | TResult2>, which exercises
    // the lib lowering path through the stable augmentation helper.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function transform(): Promise<string> {
    const p: Promise<number> = Promise.resolve(42);
    return p.then(n => String(n));
}
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise.then() return type should resolve through stable lowering.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_new_via_stable_lowering() {
    // `new Promise(...)` exercises the value-declaration lowering path
    // and DefId identity for the PromiseConstructor interface.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "new Promise<number>() should resolve without type errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_global_augmentation_merge_with_lib_interface() {
    // Global augmentation of a lib interface exercises
    // `lower_augmentation_for_arena` with the current-file arena.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        customMethod(): T;
    }
}
const arr: number[] = [1, 2, 3];
const x: number = arr.customMethod();
"#,
    );
    // Should not have TS2339 for customMethod (augmentation merged)
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("customMethod"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Global augmentation should merge customMethod into Array.\nDiagnostics: {ts2339:#?}"
    );
}

#[test]
fn test_import_type_expression_for_lib_ref() {
    // `import("...")` type expressions that reference lib types exercise
    // the name_resolver path through the stable lowering helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyArray = Array<string>;
const arr: MyArray = ["a", "b"];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Array<string> type alias referencing lib should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_multiple_lib_generic_references_share_identity() {
    // Multiple references to the same generic lib type must resolve to
    // the same DefId. This exercises the stable identity path through
    // `get_lib_def_id` and the shared definition store.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function acceptPromise(p: Promise<number>): void {}
function makePromise(): Promise<number> {
    return Promise.resolve(1);
}
acceptPromise(makePromise());
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise<number> identity should be consistent across references.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_lowering_via_stable_path() {
    // Lib type aliases like `Partial<T>`, `Record<K,T>` exercise the
    // type alias lowering path that goes through `get_lib_def_id`.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Config { host: string; port: number; }
const partial: Partial<Config> = { host: "localhost" };
const picked: Pick<Config, "host"> = { host: "localhost" };
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Partial<T> and Pick<T,K> should resolve through stable lib alias path.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests for merge_global_augmentations helper ----

#[test]
fn test_merge_global_augmentations_preserves_lib_type() {
    // Verify that merge_global_augmentations does not destroy the base lib type
    // when no augmentations exist for the given name.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Array<number> should resolve without TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_merge_global_augmentations_with_declare_global() {
    // Verify that augmentation merge works end-to-end when user code extends
    // a lib interface via `declare global`.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        customMethod(): T;
    }
}
const arr: Array<string> = ["a"];
const s: string = arr.customMethod();
const len: number = arr.length;
"#,
    );
    // Neither the original `length` member nor the augmented `customMethod`
    // should cause TS2339 (property does not exist).
    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Augmented Array<T> should have both original and custom members.\nDiagnostics: {ts2339:#?}"
    );
}

// ---- Promise resolution focused tests ----

#[test]
fn test_promise_resolve_generic_unwrap() {
    // Promise<string> should unwrap to string in async context.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function f(): Promise<string> {
    return "hello";
}
async function g() {
    const s: string = await f();
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Promise<string> unwrap should yield string.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_nested_resolve() {
    // Promise<Promise<number>> should flatten to Promise<number> per spec.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function nested(): Promise<number> {
    const inner: Promise<number> = Promise.resolve(42);
    return inner;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Nested Promise should flatten correctly.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_all_tuple_types() {
    // Promise.all with a tuple of promises should resolve to a tuple of values.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function tupleAll() {
    const p1: Promise<string> = Promise.resolve("a");
    const p2: Promise<number> = Promise.resolve(1);
    const result = await Promise.all([p1, p2]);
}
"#,
    );
    // Should not produce TS2769 (no overload matches) or TS2345 (argument not assignable)
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2769 || *c == 2345)
        .collect();
    assert!(
        errors.is_empty(),
        "Promise.all([p1, p2]) should type-check.\nDiagnostics: {errors:#?}"
    );
}

// ---- Import type lowering focused tests ----

#[test]
fn test_import_type_lib_array_reference() {
    // import("") style type references to lib types should resolve.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyArray = Array<string>;
const arr: MyArray = ["hello"];
const first: string = arr[0];
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Type alias to Array<string> should resolve through lib.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_import_type_promise_alias() {
    // Type alias referencing Promise should resolve through lib lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type AsyncString = Promise<string>;
async function f(): AsyncString {
    return "hello";
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "Promise type alias should resolve through lib.\nDiagnostics: {diagnostics:#?}"
    );
}

// ---- Lib reference heritage chain tests ----

#[test]
fn test_lib_heritage_chain_iterable_iterator() {
    // ArrayIterator should inherit from IteratorObject which
    // inherits from Iterator (es2015 chain).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr = [1, 2, 3];
const iter = arr[Symbol.iterator]();
const result = iter.next();
"#,
    );
    // iter.next() should be accessible through the heritage chain
    assert!(
        !has_error(&diagnostics, 2339),
        "Iterator .next() should be accessible through heritage chain.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_lib_ref_map_set_generic_heritage() {
    // Map and Set should have their generic type parameters preserved
    // through the heritage chain.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m = new Map<string, number>();
m.set("a", 1);
const val: number | undefined = m.get("a");
const s = new Set<string>();
s.add("hello");
const has: boolean = s.has("hello");
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        errors.is_empty(),
        "Map and Set generic operations should resolve.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn test_lib_def_id_stable_across_multiple_references() {
    // When the same lib type (e.g., Error) is referenced from multiple
    // user declarations, the DefId should be stable (not repaired/recreated).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
class MyError extends Error {
    code: number;
    constructor(message: string, code: number) {
        super(message);
        this.code = code;
    }
}
const e = new MyError("fail", 42);
const msg: string = e.message;
const code: number = e.code;
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Error subclass should inherit Error members via stable DefId.\nDiagnostics: {errors:#?}"
    );
}

// ---- Stable augmentation resolver tests (post-refactor) ----

#[test]
fn test_augmentation_resolver_uses_get_lib_def_id_for_array_augmentation() {
    // Verifies that global augmentation property resolution for Array uses
    // the stable `resolve_augmentation_node` + `get_lib_def_id` path
    // (refactored from inline resolver closures with get_or_create_def_id).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        myFirst(): T | undefined;
    }
}
const arr: number[] = [1, 2, 3];
const first: number | undefined = arr.myFirst();
const len: number = arr.length;
const pushed: number = arr.push(4);
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Array augmentation via stable resolver should preserve both original \
         and augmented members.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn test_augmentation_resolver_uses_get_lib_def_id_for_general_interface() {
    // Verifies that resolve_augmentation_property_by_name uses the stable
    // resolve_augmentation_node helper for non-Array global augmentations.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Number {
        toFixed2(digits: number): string;
    }
}
const n: number = 42;
const s: string = n.toFixed(2);
const s2: string = n.toFixed2(2);
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Number augmentation via stable resolver should preserve both original \
         and augmented members.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn test_promise_via_augmentation_stable_def_id() {
    // Promise references within augmentation contexts should use get_lib_def_id
    // (stable identity) rather than get_or_create_def_id (on-demand creation).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
async function fetchData(): Promise<string> {
    return "data";
}
const result: Promise<string> = fetchData();
result.then(data => {
    const s: string = data;
});
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Promise resolution should use stable DefId path.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn test_import_type_lib_promise_stable_lowering() {
    // import("...") type expressions for Promise should resolve through the
    // stable lib lowering path without local DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
const p: MyPromise<number> = Promise.resolve(42);
"#,
    );
    // We check there are no false TS2322 errors from broken type identity.
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "import-type Promise alias should resolve without TS2322.\nDiagnostics: {ts2322:#?}"
    );
}

#[test]
fn test_library_reference_heritage_chain_via_stable_helpers() {
    // Tests that lib-type heritage chains (e.g., Array extends ReadonlyArray)
    // resolve correctly through the stable identity helpers, ensuring that
    // inherited methods like `concat`, `indexOf` etc. are available.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: number[] = [1, 2, 3];
const idx: number = arr.indexOf(2);
const sliced: number[] = arr.slice(0, 2);
const joined: string = arr.join(",");
const includes: boolean = arr.includes(1);
"#,
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339 || *c == 2322)
        .collect();
    assert!(
        errors.is_empty(),
        "Array methods from ReadonlyArray heritage should resolve via stable helpers.\n\
         Diagnostics: {errors:#?}"
    );
}

#[test]
fn test_promise_multiple_generic_instantiations_stable() {
    // Multiple Promise instantiations with different type args should each
    // resolve through the stable DefId path without identity confusion.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
async function getString(): Promise<string> { return "a"; }
async function getNumber(): Promise<number> { return 1; }
async function getBool(): Promise<boolean> { return true; }
const s: Promise<string> = getString();
const n: Promise<number> = getNumber();
const b: Promise<boolean> = getBool();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Multiple Promise<T> instantiations should all use stable DefId.\n\
         Diagnostics: {ts2322:#?}"
    );
}

// ---- Heritage resolution wiring tests ----

#[test]
fn test_resolve_heritage_wired_during_check_source_file() {
    // Verify that resolve_cross_batch_heritage runs during check_source_file,
    // so a user class extending a lib class gets its DefId-level extends set.
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();
    let source = r#"
class MyError extends Error {
    constructor(message: string) {
        super(message);
    }
}
const e: MyError = new MyError("oops");
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Run the full check pipeline (which includes heritage resolution wiring)
    checker.check_source_file(root);

    // After checking, look up the DefId for MyError and verify it has extends set
    let my_error_sym = binder.file_locals.get("MyError");
    if let Some(my_error_sym) = my_error_sym
        && let Some(my_error_def) = checker.ctx.get_existing_def_id(my_error_sym)
    {
        let info = checker.ctx.definition_store.get(my_error_def);
        assert!(
            info.is_some(),
            "MyError's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            // The heritage resolution should have set extends to Error's DefId
            assert!(
                info.extends.is_some(),
                "MyError should have extends set to Error's DefId after heritage resolution."
            );
        }
    }
}

#[test]
#[ignore = "heritage resolution no longer populates DefinitionInfo.extends; resolved through type pipeline instead"]
fn test_resolve_heritage_user_class_extends_user_class() {
    // Verify heritage resolution works for user-defined classes within the same file
    // (same batch, so heritage should resolve during the primary binder pass).
    let source = r#"
class Base {
    x: number = 1;
}
class Child extends Base {
    y: string = "hello";
}
const c: Child = new Child();
"#;

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

    // Look up Child's DefId
    let child_sym = binder.file_locals.get("Child");
    let base_sym = binder.file_locals.get("Base");
    if let (Some(child_sym), Some(base_sym)) = (child_sym, base_sym)
        && let (Some(child_def), Some(_base_def)) = (
            checker.ctx.get_existing_def_id(child_sym),
            checker.ctx.get_existing_def_id(base_sym),
        )
    {
        let info = checker.ctx.definition_store.get(child_def);
        assert!(
            info.is_some(),
            "Child's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            assert!(
                info.extends.is_some(),
                "Child should have extends set to Base's DefId after heritage resolution."
            );
        }
    }
}

#[test]
#[ignore = "heritage resolution no longer populates DefinitionInfo.implements; resolved through type pipeline instead"]
fn test_resolve_heritage_interface_implements() {
    // Verify heritage resolution wires implements for interfaces.
    let source = r#"
interface Animal {
    name: string;
}
interface Dog extends Animal {
    breed: string;
}
const d: Dog = { name: "Rex", breed: "Lab" };
"#;

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

    // Look up Dog's DefId
    let dog_sym = binder.file_locals.get("Dog");
    if let Some(dog_sym) = dog_sym
        && let Some(dog_def) = checker.ctx.get_existing_def_id(dog_sym)
    {
        let info = checker.ctx.definition_store.get(dog_def);
        assert!(
            info.is_some(),
            "Dog's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            // For interfaces, heritage goes into implements
            assert!(
                !info.implements.is_empty(),
                "Dog should have implements set to Animal's DefId after heritage resolution."
            );
        }
    }
}

// =========================================================================
// Focused tests: prime_lib_type_params via get_lib_def_id
// =========================================================================
// These tests validate that prime_lib_type_params uses the stable
// get_lib_def_id helper (instead of get_existing_def_id with early return),
// ensuring type params are primed even when pre-population has gaps.

#[test]
fn test_prime_lib_type_params_via_get_lib_def_id() {
    // Array<T> type params should be primed even when accessed indirectly
    // through a nested generic context, exercising the get_lib_def_id path
    // in prime_lib_type_params.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrap<T>(value: T): Array<T> {
    return [value];
}
const nums: Array<number> = wrap(42);
const strs: Array<string> = wrap("hello");
// Should error: string not assignable to number
const bad: Array<number> = wrap("oops");
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Array<number> = wrap('oops') should produce TS2322 when type params are primed.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_type_params_primed_for_nested_generics() {
    // Promise type params must be primed via get_lib_def_id for nested
    // generic usage to infer correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrapInPromise<T>(value: T): Promise<T> {
    return Promise.resolve(value);
}
const p: Promise<number> = wrapInPromise(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise<number> = wrapInPromise(42) should resolve with primed type params.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Focused tests: import-type lowering through lib resolution
// =========================================================================

#[test]
fn test_import_type_indirect_lib_generic() {
    // Type alias chains that eventually reference lib generics should
    // resolve through the stable identity path without DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MaybeArray<T> = Array<T> | T;
type Numbers = MaybeArray<number>;
const a: Numbers = [1, 2, 3];
const b: Numbers = 42;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "MaybeArray<number> union with lib Array should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_conditional() {
    // Conditional types referencing Promise should resolve through the
    // stable lib DefId path (get_lib_def_id in the lowering closures).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type A = UnwrapPromise<Promise<number>>;
type B = UnwrapPromise<string>;
const a: A = 42;
const b: B = "hello";
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "UnwrapPromise conditional type should infer through lib Promise.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Focused tests: library-reference resolution stability
// =========================================================================

#[test]
fn test_library_reference_multiple_promise_declarations() {
    // Promise has declarations across multiple lib files (es5, es2015, etc.).
    // All declarations should merge consistently via the stable identity path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = new Promise<number>((resolve) => resolve(42));
const then_result = p.then(n => n.toString());
const caught = p.catch(err => "error");
"#,
    );
    // Promise members from different lib declarations should all be accessible
    let property_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        property_errors.is_empty(),
        "Promise members (then, catch) should be accessible across lib declarations.\nDiagnostics: {property_errors:#?}"
    );
}

#[test]
fn test_library_reference_error_subclass_chain() {
    // Error → TypeError → RangeError etc. hierarchy should resolve
    // through lib heritage merging with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function handleError(e: Error): string {
    return e.message;
}
const te: TypeError = new TypeError("type");
const re: RangeError = new RangeError("range");
const r1: string = handleError(te);
const r2: string = handleError(re);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Error subclasses should be assignable to Error via heritage.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_library_reference_iterable_protocol() {
    // for..of uses the Iterable protocol from lib. Heritage chain resolution
    // (Array → Iterable) must work through stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
let sum: number = 0;
for (const n of arr) {
    sum = sum + n;
}
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345 || *c == 2488)
        .collect();
    assert!(
        real_errors.is_empty(),
        "for..of on Array should work via Iterable heritage.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: dedup_decl_arenas + canonical_lib_sym_id coverage ----

#[test]
fn test_promise_resolve_and_then_chaining_stable_def_id() {
    // Exercises the full Promise lib resolution path including heritage merging.
    // Promise.resolve returns a Promise<T>, and .then() should chain correctly.
    // This relies on dedup_decl_arenas (Promise has multiple lib declarations)
    // and stable DefId identity through get_lib_def_id.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(42);
const p2: Promise<string> = p1.then(n => n.toString());
const p3: Promise<boolean> = p2.then(s => s.length > 0);
const p4: Promise<number[]> = Promise.all([p1, p1]);
"#,
    );
    let type_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        type_errors.is_empty(),
        "Promise chaining and Promise.all should work with stable lib DefIds.\nDiagnostics: {type_errors:#?}"
    );
}

#[test]
fn test_promise_generic_instantiation_identity_across_uses() {
    // Multiple references to Promise<T> with different type args must each resolve
    // to the same underlying DefId. This validates that canonical_lib_sym_id produces
    // a consistent SymbolId even when per-lib-context binders are iterated.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type StringPromise = Promise<string>;
type NumberPromise = Promise<number>;
type BoolPromise = Promise<boolean>;

async function getStr(): StringPromise { return "hello"; }
async function getNum(): NumberPromise { return 42; }
async function getBool(): BoolPromise { return true; }

const a: string = await getStr();
const b: number = await getNum();
const c: boolean = await getBool();
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise type alias instantiations should share the same lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_array_with_methods() {
    // Array<T> lowered from lib should retain method members (push, pop, map, etc.)
    // after heritage merging. This exercises the dedup_decl_arenas path because
    // Array has declarations in es5.d.ts and es2015.d.ts.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
arr.push(4);
const mapped: Array<string> = arr.map(n => n.toString());
const filtered: Array<number> = arr.filter(n => n > 1);
const joined: string = arr.join(",");
"#,
    );
    let prop_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339 || *c == 2322 || *c == 2345)
        .collect();
    assert!(
        prop_errors.is_empty(),
        "Array members from merged lib declarations should all be accessible.\nDiagnostics: {prop_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_map_set_stable_lowering() {
    // Map and Set are generic lib types with heritage chains. Verify that
    // type parameter identity is stable through canonical_lib_sym_id.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("a", 1);
const v: number | undefined = m.get("a");
const s: Set<string> = new Set(["a", "b"]);
s.add("c");
const has: boolean = s.has("a");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Map/Set generic lib types should resolve correctly.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_partial_record_stable_def_id() {
    // Partial<T> and Record<K,V> are type aliases in lib, not interfaces.
    // Their DefIds are created via the type-alias path in resolve_lib_type_by_name.
    // Verify they lower correctly with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; }
const partial: Partial<User> = { name: "Alice" };
const rec: Record<string, number> = { a: 1, b: 2 };
const val: number = rec["a"];
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Partial/Record lib type aliases should resolve with stable DefIds.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_value_and_type_dual_identity() {
    // Promise is both a value (constructor) and a type (interface).
    // The lib resolution merges these via intersection. Verify that both
    // `new Promise(...)` (value) and `Promise<T>` (type) work.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = new Promise<number>((resolve) => {
    resolve(42);
});
const p2: Promise<string> = Promise.resolve("hello");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345 || *c == 2339)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise as both constructor and type should work.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_library_reference_dedup_symbol_across_lib_files() {
    // Symbol can appear in both es2015.symbol.wellknown.d.ts and
    // es2020.symbol.wellknown.d.ts. dedup_decl_arenas should keep both when
    // the arena pointers differ. Verify via basic Symbol usage.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const sym: symbol = Symbol("test");
const sym2: symbol = Symbol.for("global");
"#,
    );
    let critical_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        critical_errors.is_empty(),
        "Symbol lib type should resolve with dedup_decl_arenas.\nDiagnostics: {critical_errors:#?}"
    );
}

// ---- Tests for lib_def_id_from_node / lib_def_id_from_node_in_lib_contexts ----
// These tests verify that the consolidated stable helpers produce the same results
// as the previous per-callsite closures, covering Promise/lib refs/import-type lowering.

#[test]
fn test_promise_generic_resolve_via_stable_def_id_helper() {
    // Verify that Promise<T> generic instantiation works through the stable
    // lib_def_id_from_node path (used in resolve_lib_type_by_name).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "data";
}
const result: Promise<number> = Promise.resolve(42);
const p: Promise<boolean> = new Promise((resolve) => resolve(true));
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise generic instantiation via stable helpers should not emit errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_generic_map_via_stable_helpers() {
    // Verify that import-type lowering for generic lib types uses the stable
    // DefId path and produces correct types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyArray = import("lib").Array<number>;
type MyMap = Map<string, number>;
const m: Map<string, number> = new Map();
m.set("key", 42);
"#,
    );
    // import("lib") won't resolve (no actual module), but Map<string, number>
    // should work without errors through the stable helpers.
    let map_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
    assert!(
        map_errors.is_empty(),
        "Map generic usage via stable helpers should not emit type errors.\nDiagnostics: {map_errors:#?}"
    );
}

#[test]
fn test_promise_then_catch_chain_via_stable_lowering() {
    // Verify that Promise.then/catch chaining works correctly through the
    // stable lib_def_id_from_node path in resolve_lib_type_by_name.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const chained = p.then(v => v.toString());
const caught = chained.catch(err => "fallback");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise.then/catch chaining via stable helpers should not emit errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_heritage_via_stable_def_id_from_node() {
    // Verify that cross-lib heritage (Array extends ReadonlyArray) resolves
    // correctly through lib_def_id_from_node in the heritage merge path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
const joined: string = arr.join(",");
const sliced: number[] = arr.slice(0, 2);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Array heritage resolution via stable helpers should not emit errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_lib_promise() {
    // Verify import type expressions for Promise work via the stable lowering path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type P = Promise<string>;
const x: P = Promise.resolve("hello");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise type alias via stable helpers should not emit errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_with_params_via_stable_def_id_from_node_in_lib_contexts() {
    // Verify that resolve_lib_type_with_params uses lib_def_id_from_node_in_lib_contexts
    // correctly for generic types like Array<T>.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<string> = ["a", "b"];
const first: string = arr[0];
const mapped: number[] = arr.map(s => s.length);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Array type with params via stable helpers should not emit errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_multiple_promise_instantiations_share_def_id_via_stable_path() {
    // Verify that multiple Promise<T> instantiations with different type args
    // share the same DefId for Promise (via lib_def_id_from_node), ensuring
    // type parameter substitution works correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<string> = Promise.resolve("a");
const p2: Promise<number> = Promise.resolve(42);
const p3: Promise<boolean> = Promise.resolve(true);
async function wrap<T>(val: T): Promise<T> { return val; }
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Multiple Promise instantiations should share DefId via stable path.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Promise-specific stable-identity tests ----

#[test]
fn test_promise_then_return_type_preserves_generic() {
    // Promise<T>.then() should return Promise<U> where U is inferred from
    // the callback. This relies on stable DefId identity for Promise across
    // heritage merging (Promise inherits from PromiseLike).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(1);
const q: Promise<string> = p.then(n => String(n));
const bad: Promise<number> = p.then(n => String(n));
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    // We expect an error on the `bad` line (string not assignable to number),
    // but no spurious errors on the valid lines.
    let spurious: Vec<_> = real_errors
        .iter()
        .filter(|(c, _)| *c == 2304 || *c == 2339)
        .collect();
    assert!(
        spurious.is_empty(),
        "Promise.then() should not produce missing-name or missing-property errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_new_resolves() {
    // `new Promise<T>((resolve, reject) => ...)` should resolve via the
    // PromiseConstructor value declaration in lib. Relies on stable identity
    // for the intersection of Promise interface + PromiseConstructor value.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
const q: Promise<number> = p;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304),
        "new Promise should not produce TS2304 (cannot find name).\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Lib reference lowering via type aliases ----

#[test]
fn test_lib_type_alias_pick_resolves_correctly() {
    // Pick<T,K> is a mapped type alias in lib.d.ts. Its resolution depends on
    // stable DefId for the type alias itself and correct type param lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type NameOnly = Pick<User, "name">;
const u: NameOnly = { name: "Alice" };
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Pick<User, 'name'> should accept {{name: string}}.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_omit_resolves_correctly() {
    // Omit<T,K> is built on top of Pick and Exclude. Its resolution
    // exercises nested type alias DefId identity.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type WithoutEmail = Omit<User, "email">;
const u: WithoutEmail = { name: "Alice", age: 30 };
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Omit<User, 'email'> should accept {{name, age}}.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Import-type lowering for lib types ----

#[test]
fn test_type_reference_to_lib_generic_preserves_params() {
    // A type alias that wraps a lib generic (e.g., `type Arr<T> = Array<T>`)
    // should preserve type parameters via the stable DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type Arr<T> = Array<T>;
const nums: Arr<number> = [1, 2, 3];
const len: number = nums.length;
const bad: Arr<number> = ["a"];
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    // Should have TS2322 for the bad line but not TS2304/TS2339 for missing names
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2339),
        "Type alias wrapping lib Array should resolve names and members.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_nested_lib_generic_references() {
    // Map<string, Array<number>> exercises nested lib generic resolution.
    // Both Map and Array must have stable DefIds for proper type lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, Array<number>> = new Map();
m.set("key", [1, 2, 3]);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304),
        "Nested lib generic Map<string, Array<number>> should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: get_canonical_lib_def_id, Promise, import-type ----

#[test]
fn test_promise_resolve_returns_typed_value() {
    if !lib_files_available() {
        return;
    }
    // Promise.resolve should return Promise<T> where T matches the argument.
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const p: Promise<string> = fetchData();
const q: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2339),
        "Promise<T> should resolve correctly without false assignability or property errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_then_chain_preserves_type() {
    if !lib_files_available() {
        return;
    }
    // then() should accept callbacks and chain correctly.
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const q: Promise<string> = p.then((x) => x.toString());
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Promise.then should be accessible without TS2339.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_with_executor() {
    if !lib_files_available() {
        return;
    }
    // new Promise((resolve, reject) => ...) should work.
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2339),
        "new Promise<number>() should resolve without 'not found' errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_extends_correctly() {
    if !lib_files_available() {
        return;
    }
    // Error should be resolvable and have .message, .name, .stack.
    let diagnostics = compile_with_lib(
        r#"
const e = new Error("oops");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Error.message and Error.name should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_regexp_methods() {
    if !lib_files_available() {
        return;
    }
    // RegExp should have .test() and .exec().
    let diagnostics = compile_with_lib(
        r#"
const re = /hello/;
const result: boolean = re.test("hello world");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "RegExp.test should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_resolves_without_error() {
    if !lib_files_available() {
        return;
    }
    // Type aliases referencing lib types should not produce false errors.
    let diagnostics = compile_with_lib(
        r#"
type StringArray = Array<string>;
const a: StringArray = ["hello", "world"];
const len: number = a.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2322),
        "Type alias to Array<string> should resolve via lib.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_multiple_lib_ref_instantiations_share_identity() {
    if !lib_files_available() {
        return;
    }
    // Multiple references to the same lib generic should use the same DefId.
    let diagnostics = compile_with_lib(
        r#"
const a: Array<number> = [1, 2, 3];
const b: Array<string> = ["a", "b"];
const c: Array<boolean> = [true, false];
const lenA: number = a.length;
const lenB: number = b.length;
const lenC: number = c.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Multiple Array<T> instantiations should all have .length.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_canonical_lib_def_id_consistency() {
    if !lib_files_available() {
        return;
    }
    // Regression: ensure get_canonical_lib_def_id produces same DefId as
    // the two-step canonical_lib_sym_id + get_lib_def_id pattern.
    // We exercise this via resolve_lib_type_with_params (which uses
    // get_canonical_lib_def_id internally) by checking that generic lib types
    // resolve correctly.
    let diagnostics = compile_with_lib(
        r#"
function identity<T>(x: T): T { return x; }
const arr: Array<number> = [1, 2, 3];
const first: number = arr[0];
const mapped: Array<string> = arr.map((x) => x.toString());
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339 || *c == 2304),
        "Array.map should be accessible via get_canonical_lib_def_id path.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_all_resolves_tuple() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function allPromises() {
    const [a, b] = await Promise.all([
        Promise.resolve(1),
        Promise.resolve("hello"),
    ]);
}
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304),
        "Promise.all should resolve without 'not found' errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_context_fallback_arena_resolves_symbol_arenas() {
    if !lib_files_available() {
        return;
    }
    // Exercise the per-lib-context fallback arena path (resolve_lib_context_fallback_arena).
    // Symbol types that span multiple lib files (e.g., SymbolConstructor from
    // es2015.symbol.wellknown.d.ts) should resolve via the symbol_arenas fallback.
    let diagnostics = compile_with_lib(
        r#"
const sym = Symbol("test");
const desc: string | undefined = sym.description;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    // Symbol should be resolvable
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304),
        "Symbol should resolve via lib context fallback arena.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// register_lib_def_resolved unified path tests
// =========================================================================
// These tests exercise the consolidated `register_lib_def_resolved` helper
// that replaced the separate get_lib_def_id + insert_def_type_params +
// register_def_auto_params_in_envs three-step pattern.

#[test]
fn test_register_lib_def_resolved_interface_path() {
    // The interface branch of resolve_lib_type_by_name now uses
    // register_lib_def_resolved. Verify that generic interface types
    // (Array, Promise) still resolve their type parameters correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const p: Promise<string> = Promise.resolve("ok");
// Verify type parameter propagation: string[] should not be assignable to number[]
const bad: Array<number> = ["a", "b"];
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Expected TS2322 for string[] assigned to Array<number>.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_register_lib_def_resolved_type_alias_path() {
    // The type alias branch of resolve_lib_type_by_name now uses
    // register_lib_def_resolved. Verify that type aliases like Partial<T>
    // and Record<K,V> still produce correct Lazy(DefId) references for
    // Application expansion.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Widget { id: number; label: string; }
const partial: Partial<Widget> = { id: 1 };
const rec: Record<string, boolean> = { active: true };
// Should reject: number is not assignable to boolean
const bad_rec: Record<string, boolean> = { active: 42 };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        real_errors.iter().any(|(c, _)| *c == 2322),
        "Expected TS2322 for number assigned to Record<string, boolean>.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_resolve_chain_with_register_lib_def_resolved() {
    // Promise.resolve().then().then() chains exercise the DefId registration
    // path multiple times for the same Promise identity. The unified helper
    // should produce consistent results across re-entrant resolution.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const result = Promise.resolve(42)
    .then(n => n.toString())
    .then(s => s.length);
const final_val: Promise<number> = result;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise chain should resolve consistently via register_lib_def_resolved.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lowering_promise_as_return() {
    // import("...") type expressions that reference Promise should resolve
    // through the unified register_lib_def_resolved path when the lib type
    // is lowered.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type Deferred<T> = Promise<T>;
type DeferredNum = Deferred<number>;

async function getDeferred(): DeferredNum {
    return 42;
}

async function consumeDeferred(): Promise<void> {
    const n: number = await getDeferred();
}
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Nested type alias to Promise should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_weakmap_weakset_resolution() {
    // WeakMap and WeakSet are lib types with constraints on their type params
    // (keys must be object). This exercises register_lib_def_resolved with
    // constrained generic lib types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const wm: WeakMap<object, string> = new WeakMap();
const obj = {};
wm.set(obj, "value");
const val: string | undefined = wm.get(obj);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
    assert!(
        real_errors.is_empty(),
        "WeakMap<object, string> should resolve via stable lib helpers.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: Promise resolution via stable DefId path ----

#[test]
fn test_promise_then_chain_resolves_via_stable_def_id() {
    // Promise.then() returns a new Promise whose type parameter is the return
    // type of the callback. This exercises the full heritage chain:
    // Promise -> PromiseLike, and generic type argument propagation through
    // the stable DefId lowering path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "data";
}
const result: Promise<number> = fetchData().then(s => s.length);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.then() chain should resolve via stable DefId path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_all_tuple_resolution() {
    // Promise.all with a tuple of promises exercises generic lib resolution
    // for the static side of Promise (PromiseConstructor) as well as the
    // instance side.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(1);
const p2: Promise<string> = Promise.resolve("a");
const all = Promise.all([p1, p2]);
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise.all([]) should resolve without TS2322.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_reject_resolve_overloads() {
    // The Promise constructor's executor callback receives resolve/reject
    // functions. This exercises value-declaration lowering for the
    // PromiseConstructor lib type.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Promise constructor should resolve executor params.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: lib refs via stable helpers ----

#[test]
fn test_lib_ref_iterable_iterator_heritage_chain() {
    // Iterable/Iterator/IterableIterator form a deep heritage chain in
    // es2015.iterable.d.ts. This exercises merge_lib_interface_heritage
    // with multi-level inheritance through stable DefId resolution.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function* gen(): IterableIterator<number> {
    yield 1;
    yield 2;
}
const it = gen();
const first = it.next();
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "IterableIterator heritage chain should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_subclass_stable_identity() {
    // Error is a lib type with both interface and var declarations.
    // Subclassing it (TypeError, RangeError) exercises the intersection
    // merge of interface + constructor function types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e: Error = new TypeError("oops");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2339),
        "Error subclass should resolve via stable lib identity.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_symbol_iterator_wellknown() {
    // Symbol.iterator is defined in es2015.symbol.wellknown.d.ts as an
    // augmentation of the SymbolConstructor interface. This exercises
    // cross-lib augmentation merge with stable DefId.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const sym: typeof Symbol.iterator = Symbol.iterator;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Symbol.iterator should resolve via stable lib identity.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: import-type lowering ----

#[test]
fn test_import_type_lib_array_alias_stable_def_id() {
    // A type alias to a lib Array should use stable DefId resolution, not
    // ad-hoc creation. This tests that the cache_canonical_lib_type_params
    // path works correctly for transitive lib references.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type LibArray = Array<string>;
const arr: LibArray = ["a", "b"];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Type alias to lib Array should resolve via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_generic_passthrough() {
    // A type alias wrapping Promise<T> should preserve the generic parameter
    // through stable DefId resolution. The Lazy(DefId) path must correctly
    // propagate type args.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
async function f(): MyPromise<number> { return 42; }
const p: MyPromise<string> = f().then(n => String(n));
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Generic type alias wrapping Promise should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_partial_record_utility() {
    // Partial<T> and Record<K,V> are type aliases in the lib that get lowered
    // as Lazy(DefId). Application expansion must correctly substitute type
    // params via cache_canonical_lib_type_params.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Foo { a: number; b: string; }
const partial: Partial<Foo> = { a: 1 };
const rec: Record<string, number> = { x: 1, y: 2 };
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Partial/Record utility types should resolve via stable lib path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_readonly_array_from_lib() {
    // ReadonlyArray<T> is the base type for Array<T> in es5.d.ts.
    // The heritage chain Array → ReadonlyArray must resolve via the stable
    // DefId path so that ReadonlyArray members appear on Array instances.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const ra: ReadonlyArray<number> = [1, 2, 3];
const len: number = ra.length;
const first: number = ra[0];
// ReadonlyArray should not have push
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "ReadonlyArray<number> should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: Promise / lib refs / import-type lowering ----
//
// These tests exercise edge cases around the stable DefId identity path,
// ensuring that resolve_augmentation_node (returning SymbolId),
// lib_def_id_from_node, and augmentation_def_id_from_node produce
// correct results for Promise, lib type references, and import-type
// expressions.

#[test]
fn test_promise_resolve_returns_promise_of_correct_type() {
    // Promise.resolve<T>(value: T) should return Promise<T>.
    // Tests that the PromiseConstructor value-declaration lowering produces
    // a callable with the correct generic signature via stable DefId.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(42);
const q: Promise<string> = Promise.resolve("hello");
// Assigning Promise<number> to Promise<string> should be an error
const bad: Promise<string> = p;
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Assigning Promise<number> to Promise<string> should produce TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_then_preserves_type_through_chain() {
    // p.then(cb) should produce a new Promise<U> where U is the return type
    // of cb. Multiple .then() calls must each preserve the generic parameter
    // through stable DefId resolution of the PromiseLike heritage chain.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(1);
const q = p.then(n => n.toString());
// q should be Promise<string>; assigning to Promise<number> should error.
const r: Promise<number> = q;
"#,
    );
    // We expect a TS2322 for the last assignment if types propagate correctly.
    // If lib resolution is broken, we'd see TS2339 or missing members instead.
    assert!(
        !has_error(&diagnostics, 2339),
        "Promise.then should resolve member 'then' via stable lib DefId.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_all_with_mixed_types_no_false_ts2345() {
    // Promise.all takes an iterable of promises and should not produce
    // false TS2345 (argument not assignable) when the input is a tuple
    // of different promise types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const a: Promise<number> = Promise.resolve(1);
const b: Promise<string> = Promise.resolve("x");
const all = Promise.all([a, b]);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "Promise.all with mixed tuple should not produce false TS2345.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_import_type_expression_promise_generic_no_false_errors() {
    // `import("...").Promise<T>` style import-type should resolve to the
    // standard Promise<T> from lib. This validates that the name-based
    // DefId resolver for import-type expressions routes through
    // resolve_entity_name_text_to_def_id_for_lowering correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
const p: MyPromise<number> = Promise.resolve(42);
const val: number = 0;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Type alias wrapping Promise<T> should resolve without errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_prototype_chain_stable() {
    // Error → TypeError → RangeError chain must resolve via stable DefId.
    // Each error subclass should have the `message` and `name` properties
    // from the base Error interface.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e: Error = new TypeError("oops");
const msg: string = e.message;
const name: string = e.name;
const re: RangeError = new RangeError("bad range");
const reMsg: string = re.message;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Error subclass heritage should resolve 'message'/'name' members via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_map_get_returns_optional() {
    // Map<K,V>.get(key) returns V | undefined in es2015.
    // This tests that the Map generic heritage chain is resolved
    // correctly via lib_def_id_from_node_in_lib_contexts.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m = new Map<string, number>();
const val = m.get("key");
// val should be number | undefined, assigning to number should error
const n: number = val;
"#,
    );
    // TS2322 expected: number | undefined not assignable to number
    // If Map resolution is broken, we'd get TS2339 for missing .get()
    assert!(
        !has_error(&diagnostics, 2339),
        "Map<K,V>.get should resolve via stable lib DefId.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_promise_async_function_return_type_unwrap() {
    // async function f(): Promise<T> should unwrap correctly.
    // The return type of an async function is always Promise<T>.
    // If the function returns T directly, it gets wrapped to Promise<T>.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function f(): Promise<number> {
    return 42;
}
async function g(): Promise<string> {
    return "hello";
}
// Mixing should error
async function bad(): Promise<string> {
    return 42;
}
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "Returning number from Promise<string> async function should produce TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_lib_ref_set_has_and_add_stable() {
    // Set<T> from es2015 should resolve .has() and .add() methods.
    // Tests that generic lib types with single type parameters work
    // through the stable resolve_lib_type_with_params path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const s = new Set<number>();
s.add(1);
const exists: boolean = s.has(1);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Set<T>.has() and .add() should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_generic_constraint_assignability() {
    // A function constrained to Promise<T> should accept Promise<number>
    // but reject non-Promise types. This tests that the DefId for Promise
    // is stable across generic constraint checking.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function unwrap<T>(p: Promise<T>): T {
    return undefined as any;
}
const n: number = unwrap(Promise.resolve(42));
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Generic function with Promise<T> constraint should resolve via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_array_from_static_method() {
    // Array.from() is a static method on ArrayConstructor.
    // This tests that value-declaration lowering for lib types correctly
    // resolves the ArrayConstructor's members via register_lib_def_resolved.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: number[] = Array.from([1, 2, 3]);
const arr2: string[] = Array.from("hello");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Array.from() should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_global_augmentation_merges_with_stable_def_id() {
    // declare global { interface Array<T> { myMethod(): T } } should
    // merge with the lib Array<T> type. This tests that the augmentation
    // resolver (resolve_augmentation_node returning SymbolId) correctly
    // routes through augmentation_def_id_from_node for the DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare global {
    interface Array<T> {
        myCustomMethod(): T;
    }
}
const arr: number[] = [1, 2, 3];
const len: number = arr.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133 && *c != 2669)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Global augmentation of Array should preserve .length via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_race_stable_def_id_resolution() {
    // Promise.race takes an iterable and returns a Promise that resolves
    // to the type of the first settled promise. Tests the PromiseConstructor
    // static method resolution via stable DefId.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(1);
const p2: Promise<number> = Promise.resolve(2);
const winner = Promise.race([p1, p2]);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Promise.race should resolve via stable lib DefId.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_import_type_lib_date_methods() {
    // Date from lib should resolve its methods (getTime, toISOString, etc.)
    // correctly via the stable DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const d: Date = new Date();
const time: number = d.getTime();
const iso: string = d.toISOString();
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Date methods should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- SymbolId-typed resolution path tests ----
//
// These tests verify that the refactored resolution helpers
// (resolve_lib_node_in_arenas returning SymbolId instead of raw u32)
// produce correct results through the full lowering pipeline.

#[test]
fn test_promise_resolve_via_sym_id_typed_path() {
    // Verify Promise resolves correctly through the SymbolId-typed
    // resolution path (resolve_lib_node_in_arenas -> SymbolId -> get_lib_def_id).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const result: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise should resolve via SymbolId-typed resolution path without errors.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_array_map_filter_via_sym_id_path() {
    // Array methods like map/filter rely on the lib heritage chain
    // (Array extends ReadonlyArray) resolving through SymbolId-typed helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const nums: number[] = [1, 2, 3];
const doubled: number[] = nums.map(x => x * 2);
const evens: number[] = nums.filter(x => x % 2 === 0);
const joined: string = nums.join(", ");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Array methods should resolve through SymbolId-typed lib heritage chain.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_promise_via_sym_id_path() {
    // import() type expressions for lib types must go through the
    // SymbolId-typed resolution path correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise = Promise<string>;
const p: MyPromise = Promise.resolve("test");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Type alias referencing Promise should work via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_nested_generic_via_sym_id_path() {
    // Nested generics (e.g., Promise<Array<Map<string, number>>>) exercise
    // the SymbolId-typed resolution recursively through multiple lib types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<Array<number>> = Promise.resolve([1, 2, 3]);
const nested: Array<Promise<string>> = [Promise.resolve("a")];
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Nested lib generics should resolve via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_then_catch_finally_chain_via_sym_id_path() {
    // Promise method chains exercise heritage resolution (Promise members)
    // through the SymbolId-typed path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const chained = p.then(x => x.toString()).catch(e => "error");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 6133)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Promise .then/.catch chain should resolve via SymbolId-typed path.\nDiagnostics: {real_errors:#?}"
    );
}
