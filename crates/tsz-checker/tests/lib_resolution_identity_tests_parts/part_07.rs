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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let property_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345 || *c == 2488)
        .collect();
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
    let type_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
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
    let prop_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2339 || *c == 2322 || *c == 2345)
        .collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345)
        .collect();
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
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2345 || *c == 2339)
        .collect();
    assert!(
        real_errors.is_empty(),
        "Promise as both constructor and type should work.\nDiagnostics: {real_errors:#?}"
    );
}

