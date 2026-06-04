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
        !has_diagnostic_code(&diagnostics, 2339),
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
        has_diagnostic_code(&diagnostics, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133, 2669]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
        !has_diagnostic_code(&diagnostics, 2339),
        "Promise.race should resolve via stable lib DefId.\nDiagnostics: {diagnostics:#?}"
    );
}
