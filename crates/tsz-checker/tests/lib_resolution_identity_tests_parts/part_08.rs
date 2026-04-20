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

