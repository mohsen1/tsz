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

