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

