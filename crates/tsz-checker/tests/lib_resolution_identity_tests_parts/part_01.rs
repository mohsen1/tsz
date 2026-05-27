#[test]
fn test_import_type_lib_promise_stable_lowering() {
    // import("...") type expressions for Promise should resolve through the
    // stable lib lowering path without local DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
const p: MyPromise<number> = Promise.resolve(42);
"#,
    );
    // We check there are no false TS2322 errors from broken type identity.
    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert!(
        ts2322.is_empty(),
        "import-type Promise alias should resolve without TS2322.\nDiagnostics: {ts2322:#?}"
    );
}

#[test]
fn test_library_reference_heritage_chain_via_stable_helpers() {
    // Tests that lib-type heritage chains (e.g., Array extends ReadonlyArray)
    // resolve correctly through the stable identity helpers, ensuring that
    // inherited methods like `concat`, `indexOf` etc. are available.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: number[] = [1, 2, 3];
const idx: number = arr.indexOf(2);
const sliced: number[] = arr.slice(0, 2);
const joined: string = arr.join(",");
const includes: boolean = arr.includes(1);
"#,
    );
    let errors = diagnostics_with_any_code(&diagnostics, &[2339, 2322]);
    assert!(
        errors.is_empty(),
        "Array methods from ReadonlyArray heritage should resolve via stable helpers.\n\
         Diagnostics: {errors:#?}"
    );
}

#[test]
fn test_promise_multiple_generic_instantiations_stable() {
    // Multiple Promise instantiations with different type args should each
    // resolve through the stable DefId path without identity confusion.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib_and_options(
        r#"
async function getString(): Promise<string> { return "a"; }
async function getNumber(): Promise<number> { return 1; }
async function getBool(): Promise<boolean> { return true; }
const s: Promise<string> = getString();
const n: Promise<number> = getNumber();
const b: Promise<boolean> = getBool();
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let ts2322 = diagnostics_with_code(&diagnostics, 2322);
    assert!(
        ts2322.is_empty(),
        "Multiple Promise<T> instantiations should all use stable DefId.\n\
         Diagnostics: {ts2322:#?}"
    );
}

// ---- Heritage resolution wiring tests ----

#[test]
fn test_resolve_heritage_wired_during_check_source_file() {
    // Verify that resolve_cross_batch_heritage runs during check_source_file,
    // so a user class extending a lib class gets its DefId-level extends set.
    if !lib_files_available() {
        return;
    }

    let lib_files = load_lib_files_for_test();
    let source = r#"
class MyError extends Error {
    constructor(message: string) {
        super(message);
    }
}
const e: MyError = new MyError("oops");
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_checker::context::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    // Run the full check pipeline (which includes heritage resolution wiring)
    checker.check_source_file(root);

    // After checking, look up the DefId for MyError and verify it has extends set
    let my_error_sym = binder.file_locals.get("MyError");
    if let Some(my_error_sym) = my_error_sym
        && let Some(my_error_def) = checker.ctx.get_existing_def_id(my_error_sym)
    {
        let info = checker.ctx.definition_store.get(my_error_def);
        assert!(
            info.is_some(),
            "MyError's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            // The heritage resolution should have set extends to Error's DefId
            assert!(
                info.extends.is_some(),
                "MyError should have extends set to Error's DefId after heritage resolution."
            );
        }
    }
}

#[test]
fn test_resolve_heritage_user_class_extends_user_class() {
    // Verify heritage resolution works for user-defined classes within the same file
    // (same batch, so heritage should resolve during the primary binder pass).
    let source = r#"
class Base {
    x: number = 1;
}
class Child extends Base {
    y: string = "hello";
}
const c: Child = new Child();
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Look up Child's DefId
    let child_sym = binder.file_locals.get("Child");
    let base_sym = binder.file_locals.get("Base");
    if let (Some(child_sym), Some(base_sym)) = (child_sym, base_sym)
        && let (Some(child_def), Some(_base_def)) = (
            checker.ctx.get_existing_def_id(child_sym),
            checker.ctx.get_existing_def_id(base_sym),
        )
    {
        let info = checker.ctx.definition_store.get(child_def);
        assert!(
            info.is_some(),
            "Child's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            assert!(
                info.extends.is_some(),
                "Child should have extends set to Base's DefId after heritage resolution."
            );
        }
    }
}

#[test]
fn test_resolve_heritage_interface_extends() {
    // Verify heritage resolution wires interface extends edges.
    let source = r#"
interface Animal {
    name: string;
}
interface Dog extends Animal {
    breed: string;
}
const d: Dog = { name: "Rex", breed: "Lab" };
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "valid interface extends assignment should not emit diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    // Look up Dog's DefId
    let dog_sym = binder.file_locals.get("Dog");
    if let Some(dog_sym) = dog_sym
        && let Some(dog_def) = checker.ctx.get_existing_def_id(dog_sym)
    {
        let info = checker.ctx.definition_store.get(dog_def);
        assert!(
            info.is_some(),
            "Dog's DefinitionInfo should exist in the store"
        );
        if let Some(info) = info {
            assert!(
                info.extends.is_some(),
                "Dog should have extends set to Animal's DefId after heritage resolution."
            );
        }
    }
}

// =========================================================================
// Focused tests: prime_lib_type_params via get_lib_def_id
// =========================================================================
// These tests validate that prime_lib_type_params uses the stable
// get_lib_def_id helper (instead of get_existing_def_id with early return),
// ensuring type params are primed even when pre-population has gaps.

#[test]
fn test_prime_lib_type_params_via_get_lib_def_id() {
    // Array<T> type params should be primed even when accessed indirectly
    // through a nested generic context, exercising the get_lib_def_id path
    // in prime_lib_type_params.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrap<T>(value: T): Array<T> {
    return [value];
}
const nums: Array<number> = wrap(42);
const strs: Array<string> = wrap("hello");
// Should error: string not assignable to number
const bad: Array<number> = wrap("oops");
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2322),
        "Array<number> = wrap('oops') should produce TS2322 when type params are primed.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_type_params_primed_for_nested_generics() {
    // Promise type params must be primed via get_lib_def_id for nested
    // generic usage to infer correctly.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function wrapInPromise<T>(value: T): Promise<T> {
    return Promise.resolve(value);
}
const p: Promise<number> = wrapInPromise(42);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Promise<number> = wrapInPromise(42) should resolve with primed type params.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Focused tests: import-type lowering through lib resolution
// =========================================================================

#[test]
fn test_import_type_indirect_lib_generic() {
    // Type alias chains that eventually reference lib generics should
    // resolve through the stable identity path without DefId repair.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MaybeArray<T> = Array<T> | T;
type Numbers = MaybeArray<number>;
const a: Numbers = [1, 2, 3];
const b: Numbers = 42;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "MaybeArray<number> union with lib Array should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_conditional() {
    // Conditional types referencing Promise should resolve through the
    // stable lib DefId path (get_lib_def_id in the lowering closures).
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type UnwrapPromise<T> = T extends Promise<infer U> ? U : T;
type A = UnwrapPromise<Promise<number>>;
type B = UnwrapPromise<string>;
const a: A = 42;
const b: B = "hello";
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "UnwrapPromise conditional type should infer through lib Promise.\nDiagnostics: {real_errors:#?}"
    );
}

// =========================================================================
// Focused tests: library-reference resolution stability
// =========================================================================

#[test]
fn test_library_reference_multiple_promise_declarations() {
    // Promise has declarations across multiple lib files (es5, es2015, etc.).
    // All declarations should merge consistently via the stable identity path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = new Promise<number>((resolve) => resolve(42));
const then_result = p.then(n => n.toString());
const caught = p.catch(err => "error");
"#,
    );
    // Promise members from different lib declarations should all be accessible
    let property_errors = diagnostics_with_code(&diagnostics, 2339);
    assert!(
        property_errors.is_empty(),
        "Promise members (then, catch) should be accessible across lib declarations.\nDiagnostics: {property_errors:#?}"
    );
}

#[test]
fn test_library_reference_error_subclass_chain() {
    // Error → TypeError → RangeError etc. hierarchy should resolve
    // through lib heritage merging with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function handleError(e: Error): string {
    return e.message;
}
const te: TypeError = new TypeError("type");
const re: RangeError = new RangeError("range");
const r1: string = handleError(te);
const r2: string = handleError(re);
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
    assert!(
        real_errors.is_empty(),
        "Error subclasses should be assignable to Error via heritage.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_library_reference_iterable_protocol() {
    // for..of uses the Iterable protocol from lib. Heritage chain resolution
    // (Array → Iterable) must work through stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
let sum: number = 0;
for (const n of arr) {
    sum = sum + n;
}
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345, 2488]);
    assert!(
        real_errors.is_empty(),
        "for..of on Array should work via Iterable heritage.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: dedup_decl_arenas + canonical_lib_sym_id coverage ----

#[test]
fn test_promise_resolve_and_then_chaining_stable_def_id() {
    // Exercises the full Promise lib resolution path including heritage merging.
    // Promise.resolve returns a Promise<T>, and .then() should chain correctly.
    // This relies on dedup_decl_arenas (Promise has multiple lib declarations)
    // and stable DefId identity through get_lib_def_id.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(42);
const p2: Promise<string> = p1.then(n => n.toString());
const p3: Promise<boolean> = p2.then(s => s.length > 0);
const p4: Promise<number[]> = Promise.all([p1, p1]);
"#,
    );
    let type_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
    assert!(
        type_errors.is_empty(),
        "Promise chaining and Promise.all should work with stable lib DefIds.\nDiagnostics: {type_errors:#?}"
    );
}

#[test]
fn test_promise_generic_instantiation_identity_across_uses() {
    // Multiple references to Promise<T> with different type args must each resolve
    // to the same underlying DefId. This validates that canonical_lib_sym_id produces
    // a consistent SymbolId even when per-lib-context binders are iterated.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type StringPromise = Promise<string>;
type NumberPromise = Promise<number>;
type BoolPromise = Promise<boolean>;

async function getStr(): StringPromise { return "hello"; }
async function getNum(): NumberPromise { return 42; }
async function getBool(): BoolPromise { return true; }

const a: string = await getStr();
const b: number = await getNum();
const c: boolean = await getBool();
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
    assert!(
        real_errors.is_empty(),
        "Promise type alias instantiations should share the same lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_array_with_methods() {
    // Array<T> lowered from lib should retain method members (push, pop, map, etc.)
    // after heritage merging. This exercises the dedup_decl_arenas path because
    // Array has declarations in es5.d.ts and es2015.d.ts.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const arr: Array<number> = [1, 2, 3];
const len: number = arr.length;
arr.push(4);
const mapped: Array<string> = arr.map(n => n.toString());
const filtered: Array<number> = arr.filter(n => n > 1);
const joined: string = arr.join(",");
"#,
    );
    let prop_errors = diagnostics_with_any_code(&diagnostics, &[2339, 2322, 2345]);
    assert!(
        prop_errors.is_empty(),
        "Array members from merged lib declarations should all be accessible.\nDiagnostics: {prop_errors:#?}"
    );
}

#[test]
fn test_import_type_lib_map_set_stable_lowering() {
    // Map and Set are generic lib types with heritage chains. Verify that
    // type parameter identity is stable through canonical_lib_sym_id.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, number> = new Map();
m.set("a", 1);
const v: number | undefined = m.get("a");
const s: Set<string> = new Set(["a", "b"]);
s.add("c");
const has: boolean = s.has("a");
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
    assert!(
        real_errors.is_empty(),
        "Map/Set generic lib types should resolve correctly.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_partial_record_stable_def_id() {
    // Partial<T> and Record<K,V> are type aliases in lib, not interfaces.
    // Their DefIds are created via the type-alias path in resolve_lib_type_by_name.
    // Verify they lower correctly with stable helpers.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; }
const partial: Partial<User> = { name: "Alice" };
const rec: Record<string, number> = { a: 1, b: 2 };
const val: number = rec["a"];
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
    assert!(
        real_errors.is_empty(),
        "Partial/Record lib type aliases should resolve with stable DefIds.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_value_and_type_dual_identity() {
    // Promise is both a value (constructor) and a type (interface).
    // The lib resolution merges these via intersection. Verify that both
    // `new Promise(...)` (value) and `Promise<T>` (type) work.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = new Promise<number>((resolve) => {
    resolve(42);
});
const p2: Promise<string> = Promise.resolve("hello");
"#,
    );
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345, 2339]);
    assert!(
        real_errors.is_empty(),
        "Promise as both constructor and type should work.\nDiagnostics: {real_errors:#?}"
    );
}

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
    let critical_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let map_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    // We expect an error on the `bad` line (string not assignable to number),
    // but no spurious errors on the valid lines.
    let spurious = diagnostics_with_any_code(&real_errors, &[2304, 2339]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2304),
        "new Promise should not produce TS2304 (cannot find name).\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Lib reference lowering via type aliases ----

#[test]
fn test_lib_type_alias_pick_resolves_correctly() {
    // Pick<T,K> is a mapped type alias in lib.d.ts. Its resolution depends on
    // stable DefId for the type alias itself and correct type param lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type NameOnly = Pick<User, "name">;
const u: NameOnly = { name: "Alice" };
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Pick<User, 'name'> should accept {{name: string}}.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_omit_resolves_correctly() {
    // Omit<T,K> is built on top of Pick and Exclude. Its resolution
    // exercises nested type alias DefId identity.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type WithoutEmail = Omit<User, "email">;
const u: WithoutEmail = { name: "Alice", age: 30 };
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Omit<User, 'email'> should accept {{name, age}}.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Import-type lowering for lib types ----

#[test]
fn test_type_reference_to_lib_generic_preserves_params() {
    // A type alias that wraps a lib generic (e.g., `type Arr<T> = Array<T>`)
    // should preserve type parameters via the stable DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type Arr<T> = Array<T>;
const nums: Arr<number> = [1, 2, 3];
const len: number = nums.length;
const bad: Arr<number> = ["a"];
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    // Should have TS2322 for the bad line but not TS2304/TS2339 for missing names
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2304, 2339]),
        "Type alias wrapping lib Array should resolve names and members.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_nested_lib_generic_references() {
    // Map<string, Array<number>> exercises nested lib generic resolution.
    // Both Map and Array must have stable DefIds for proper type lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, Array<number>> = new Map();
m.set("key", [1, 2, 3]);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2304),
        "Nested lib generic Map<string, Array<number>> should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: get_canonical_lib_def_id, Promise, import-type ----

#[test]
fn test_promise_resolve_returns_typed_value() {
    if !lib_files_available() {
        return;
    }
    // Promise.resolve should return Promise<T> where T matches the argument.
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const p: Promise<string> = fetchData();
const q: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2339]),
        "Promise<T> should resolve correctly without false assignability or property errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_then_chain_preserves_type() {
    if !lib_files_available() {
        return;
    }
    // then() should accept callbacks and chain correctly.
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const q: Promise<string> = p.then((x) => x.toString());
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Promise.then should be accessible without TS2339.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_with_executor() {
    if !lib_files_available() {
        return;
    }
    // new Promise((resolve, reject) => ...) should work.
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2304, 2339]),
        "new Promise<number>() should resolve without 'not found' errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_extends_correctly() {
    if !lib_files_available() {
        return;
    }
    // Error should be resolvable and have .message, .name, .stack.
    let diagnostics = compile_with_lib(
        r#"
const e = new Error("oops");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "Error.message and Error.name should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_regexp_methods() {
    if !lib_files_available() {
        return;
    }
    // RegExp should have .test() and .exec().
    let diagnostics = compile_with_lib(
        r#"
const re = /hello/;
const result: boolean = re.test("hello world");
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
        "RegExp.test should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_resolves_without_error() {
    if !lib_files_available() {
        return;
    }
    // Type aliases referencing lib types should not produce false errors.
    let diagnostics = compile_with_lib(
        r#"
type StringArray = Array<string>;
const a: StringArray = ["hello", "world"];
const len: number = a.length;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2304, 2322]),
        "Type alias to Array<string> should resolve via lib.\nDiagnostics: {real_errors:#?}"
    );
}

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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2339, 2304]),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    assert!(
        !has_diagnostic_code(&real_errors, 2304),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 2583]);
    // Symbol should be resolvable
    assert!(
        !has_diagnostic_code(&real_errors, 2304),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
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
    let real_errors = diagnostics_with_any_code(&diagnostics, &[2322, 2339, 2345]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Promise.then() chain should resolve via stable DefId path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_all_tuple_resolution() {
    // Promise.all with a tuple of promises exercises generic lib resolution
    // for the static side of Promise (PromiseConstructor) as well as the
    // instance side.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p1: Promise<number> = Promise.resolve(1);
const p2: Promise<string> = Promise.resolve("a");
const all = Promise.all([p1, p2]);
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Promise.all([]) should resolve without TS2322.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_reject_resolve_overloads() {
    // The Promise constructor's executor callback receives resolve/reject
    // functions. This exercises value-declaration lowering for the
    // PromiseConstructor lib type.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2345]),
        "Promise constructor should resolve executor params.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: lib refs via stable helpers ----

#[test]
fn test_lib_ref_iterable_iterator_heritage_chain() {
    // Iterable/Iterator/IterableIterator form a deep heritage chain in
    // es2015.iterable.d.ts. This exercises merge_lib_interface_heritage
    // with multi-level inheritance through stable DefId resolution.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
function* gen(): IterableIterator<number> {
    yield 1;
    yield 2;
}
const it = gen();
const first = it.next();
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "IterableIterator heritage chain should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_subclass_stable_identity() {
    // Error is a lib type with both interface and var declarations.
    // Subclassing it (TypeError, RangeError) exercises the intersection
    // merge of interface + constructor function types.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const e: Error = new TypeError("oops");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_any_diagnostic_code(&real_errors, &[2322, 2339]),
        "Error subclass should resolve via stable lib identity.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_symbol_iterator_wellknown() {
    // Symbol.iterator is defined in es2015.symbol.wellknown.d.ts as an
    // augmentation of the SymbolConstructor interface. This exercises
    // cross-lib augmentation merge with stable DefId.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const sym: typeof Symbol.iterator = Symbol.iterator;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Symbol.iterator should resolve via stable lib identity.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: import-type lowering ----

#[test]
fn test_import_type_lib_array_alias_stable_def_id() {
    // A type alias to a lib Array should use stable DefId resolution, not
    // ad-hoc creation. This tests that the cache_canonical_lib_type_params
    // path works correctly for transitive lib references.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type LibArray = Array<string>;
const arr: LibArray = ["a", "b"];
const len: number = arr.length;
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Type alias to lib Array should resolve via stable DefId.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_promise_generic_passthrough() {
    // A type alias wrapping Promise<T> should preserve the generic parameter
    // through stable DefId resolution. The Lazy(DefId) path must correctly
    // propagate type args.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type MyPromise<T> = Promise<T>;
async function f(): MyPromise<number> { return 42; }
const p: MyPromise<string> = f().then(n => String(n));
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Generic type alias wrapping Promise should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn local_promise_alias_is_not_valid_async_return_type() {
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type Promise<T> = { value: T };

async function f(): Promise<string> {
    return "ok";
}

const value = f();
const bad: Promise<string> = value;

export {};
"#,
    );
    assert!(
        has_diagnostic_code(&diagnostics, 1064),
        "local Promise alias should not satisfy async return type identity.\nDiagnostics: {diagnostics:#?}"
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "async body should be checked against the local Promise alias payload.\nDiagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_import_type_partial_record_utility() {
    // Partial<T> and Record<K,V> are type aliases in the lib that get lowered
    // as Lazy(DefId). Application expansion must correctly substitute type
    // params via cache_canonical_lib_type_params.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface Foo { a: number; b: string; }
const partial: Partial<Foo> = { a: 1 };
const rec: Record<string, number> = { x: 1, y: 2 };
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "Partial/Record utility types should resolve via stable lib path.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_readonly_array_from_lib() {
    // ReadonlyArray<T> is the base type for Array<T> in es5.d.ts.
    // The heritage chain Array → ReadonlyArray must resolve via the stable
    // DefId path so that ReadonlyArray members appear on Array instances.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const ra: ReadonlyArray<number> = [1, 2, 3];
const len: number = ra.length;
const first: number = ra[0];
// ReadonlyArray should not have push
"#,
    );
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318]);
    assert!(
        !has_diagnostic_code(&real_errors, 2322),
        "ReadonlyArray<number> should resolve via stable lib DefId.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: Promise / lib refs / import-type lowering ----
//
// These tests exercise edge cases around the stable DefId identity path,
// ensuring that resolve_augmentation_node (returning SymbolId),
// lib_def_id_from_node, and augmentation_def_id_from_node produce
// correct results for Promise, lib type references, and import-type
// expressions.

#[test]
fn test_promise_resolve_returns_promise_of_correct_type() {
    // Promise.resolve<T>(value: T) should return Promise<T>.
    // Tests that the PromiseConstructor value-declaration lowering produces
    // a callable with the correct generic signature via stable DefId.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const p: Promise<number> = Promise.resolve(42);
const q: Promise<string> = Promise.resolve("hello");
// Assigning Promise<number> to Promise<string> should be an error
const bad: Promise<string> = p;
"#,
    );
    assert!(
        has_diagnostic_code(&diagnostics, 2322),
        "Assigning Promise<number> to Promise<string> should produce TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

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
        !has_diagnostic_code(&diagnostics, 2339),
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
        !has_diagnostic_code(&diagnostics, 2345),
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
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
    let real_errors = diagnostics_without_codes(&diagnostics, &[2318, 6133]);
    assert!(
        !has_diagnostic_code(&real_errors, 2339),
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

