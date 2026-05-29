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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2345),
        "Expected TS2345 for boolean argument to Map.set(string, number).\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_map_constructor_rejects_heterogeneous_value_inference() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const map = new Map([["", true], ["", 0]]);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        has_diagnostic_code(&real_errors, 2769),
        "Map constructor should reject heterogeneous value inference instead of widening to a union.\nDiagnostics: {real_errors:#?}"
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    // Should not have type errors in basic Promise chaining
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2345]),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2304, 2339]),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Array.concat and .length should be accessible via heritage.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_reduce_empty_array_concat_failure_surfaces_through_destructuring() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
declare var tuple: [boolean, number, ...string[]];

const [a, b, c, ...rest] = tuple;

declare var receiver: typeof tuple;

[...receiver] = tuple;

const [oops1] = [1, 2, 3].reduce((accu, el) => accu.concat(el), []);

const [oops2] = [1, 2, 3].reduce((acc: number[], e) => acc.concat(e), []);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2488),
        "Destructuring the failed reduce result should report TS2488.\nDiagnostics: {real_errors:#?}"
    );
    assert!(
        has_diagnostic_code(&real_errors, 2769),
        "The reduce/concat overload failure should report TS2769.\nDiagnostics: {real_errors:#?}"
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2669]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2345]),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2669]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2669]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let type_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
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
        diagnostics_with_code_any_message(&diagnostics, 2339, &["resolve", "all", "reject"])
            .is_empty(),
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Array and Promise boxed types should resolve via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_array_boxed_type_registration_preserves_array_method_surface() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
class Ship {
    isSunk = false;
}

class Board {
    ships: Ship[] = [];

    allShipsSunk() {
        return this.ships.every(function (val) { return val.isSunk; });
    }
}
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2339, 7006]),
        "Array boxed type registration should preserve Array<T> methods and callback inference.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_recursive_alias_interface_preserves_array_method_surface() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Box<T> { value: T }
type Box2 = Box<Box2 | number>;

interface HTMLHeadingElement {
    id: string;
    tagName: string;
    textContent: string | null;
}

type Tree = [HTMLHeadingElement, Tree][];

function parse(node: Tree, index: number[] = []): string[] {
    return node.map(([el, children], i) => {
        const idx = [...index, i + 1];
        return `${el.id}:${idx.join(".")}:${children.length}`;
    });
}

function cons(hs: HTMLHeadingElement[]): Tree {
    return hs.reduce<Tree>((node, _h) => node, []);
}
"#,
    );
    let array_surface_errors = diagnostics_with_any_code(&diagnostics, &[2339, 7006, 7031]);
    assert!(
        array_surface_errors.is_empty(),
        "Recursive interface aliases should not overwrite the registered Array<T> base.\nDiagnostics: {array_surface_errors:#?}"
    );
}

#[test]
fn test_array_base_display_properties_preserve_lib_order() {
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();
    let source = "const marker = 1;";
    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.prime_boxed_types();

    let array_base = tsz_solver::construction::TypeDatabase::get_array_base_type(checker.ctx.types)
        .expect("expected registered Array<T> base");
    let display_props: Vec<_> = checker
        .ctx
        .types
        .get_display_properties(array_base)
        .expect("expected Array<T> display properties")
        .iter()
        .map(|prop| checker.ctx.types.resolve_atom_ref(prop.name).to_string())
        .collect();
    let shape_props: Vec<_> =
        tsz_solver::type_queries::get_callable_shape(checker.ctx.types, array_base)
            .expect("expected callable Array<T> base")
            .properties
            .iter()
            .map(|prop| checker.ctx.types.resolve_atom_ref(prop.name).to_string())
            .collect();

    assert!(
        display_props.starts_with(&["toString".to_string(), "toLocaleString".to_string()]),
        "expected Array<T> display order to start with toString/toLocaleString, got display_props={display_props:?} shape_props={shape_props:?}"
    );
    assert!(
        shape_props.iter().any(|name| name == "every"),
        "expected registered Array<T> base to preserve every(); shape_props={shape_props:?}"
    );
}

#[test]
fn test_array_remap_symbol_type_preserves_lib_order_for_diagnostics() {
    if !lib_files_available() {
        return;
    }

    let (formatted, display_props, shape_props) = inspect_symbol_with_lib(
        r#"
type Exclude<T, U> = T extends U ? never : T;
declare let src2: { [K in keyof number[] as Exclude<K, "length">]: (number[])[K] };
"#,
        "src2",
        CheckerOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        formatted.contains("{ [x: number]: number; toString: () => string; toLocaleString:"),
        "expected src2 diagnostic display to start with toString/toLocaleString, got formatted={formatted:?} display_props={display_props:?} shape_props={shape_props:?}"
    );
}

#[test]
fn test_array_remap_missing_property_message_preserves_lib_order() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_with_lib_and_options(
        r#"
type Exclude<T, U> = T extends U ? never : T;
declare let tgt2: number[];
declare let src2: { [K in keyof number[] as Exclude<K, "length">]: (number[])[K] };
tgt2 = src2;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    let message = real_errors
        .iter()
        .find(|(code, _)| *code == 2741)
        .map(|(_, message)| message.as_str())
        .expect("expected TS2741");
    assert!(
        message.contains("{ [x: number]: number; toString: () => string; toLocaleString:")
            && !message.contains("find: {"),
        "expected TS2741 source display to preserve Array<T> lib order, got: {message}"
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2345]),
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
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
    let ts2339 = diagnostics_with_code_message(&diagnostics, 2339, "customMethod");
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
        !has_diagnostic_code(&diagnostics, 2322),
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
    let ts2339 = diagnostics_with_code(&diagnostics, 2339);
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
        !has_diagnostic_code(&diagnostics, 2322),
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
        !has_diagnostic_code(&diagnostics, 2322),
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2769, 2345]);
    assert!(
        errors.is_empty(),
        "Promise.all([p1, p2]) should type-check.\nDiagnostics: {errors:#?}"
    );
}

#[test]
fn promise_all_async_map_tuple_context_evaluates_awaited_elements() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
interface ILocalExtension {
  isApplicationScoped: boolean;
  publisherId: string | null;
}
type Metadata = {
  updated: boolean;
};
declare function scanMetadata(
  local: ILocalExtension
): Promise<Metadata | undefined>;

async function copyExtensions(
  fromExtensions: ILocalExtension[]
): Promise<void> {
  const extensions: [ILocalExtension, Metadata | undefined][] =
    await Promise.all(
      fromExtensions
        .filter((e) => !e.isApplicationScoped)
        .map(async (e) => [e, await scanMetadata(e)])
    );
}
"#,
        CheckerOptions {
            target: ScriptTarget::ESNext,
            no_implicit_any: true,
            ..Default::default()
        },
    );
    let errors = diagnostics_with_code(&diagnostics, 2322);
    assert!(
        errors.is_empty(),
        "Promise.all async map tuple context should not produce TS2322.\nDiagnostics: {errors:#?}"
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
        !has_diagnostic_code(&diagnostics, 2322),
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
        !has_diagnostic_code(&diagnostics, 2322),
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
        !has_diagnostic_code(&diagnostics, 2339),
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
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
    let errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339]);
    assert!(
        errors.is_empty(),
        "Promise resolution should use stable DefId path.\nDiagnostics: {errors:#?}"
    );
}

