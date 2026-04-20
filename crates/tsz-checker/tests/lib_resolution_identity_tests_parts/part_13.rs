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

// ---- Stable helper path tests (no DefId repair) ----
//
// These tests verify that lib type lowering uses the stable identity helpers
// (no_value_resolver, get_lib_def_id, register_lib_def_resolved) and does
// not fall back to on-demand DefId creation or local caching tricks.

#[test]
fn test_promise_generic_return_type_stable() {
    // Verify that Promise<T> as a generic return type uses stable DefId lowering.
    // The `then` callback's return type should propagate through the Promise chain.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<{ name: string; age: number }> {
    return { name: "test", age: 30 };
}
const result: Promise<{ name: string; age: number }> = fetchData();
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
        "Promise<{{name, age}}> return type should be stable across references.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_nested_generic_stable_lowering() {
    // Nested generics like Promise<Array<T>> exercise both Promise and Array
    // DefId resolution through stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
async function getItems(): Promise<Array<string>> {
    return ["a", "b"];
}
const items: Promise<Array<string>> = getItems();
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Promise<Array<string>> nested generics should resolve via stable helpers.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_type_stable_identity() {
    // Error is a lib type that exercises the interface heritage path
    // (Error extends Object in lib.es5.d.ts).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e = new Error("test");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 6133).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Error lib type properties should resolve via stable DefId path.\nDiagnostics: {real_errors:#?}"
    );
}

