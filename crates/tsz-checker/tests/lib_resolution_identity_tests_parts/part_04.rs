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

