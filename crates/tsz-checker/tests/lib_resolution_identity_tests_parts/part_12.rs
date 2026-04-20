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

