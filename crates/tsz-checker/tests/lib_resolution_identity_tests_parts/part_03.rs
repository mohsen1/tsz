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

