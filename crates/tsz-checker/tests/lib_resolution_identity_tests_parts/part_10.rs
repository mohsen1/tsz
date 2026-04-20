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

