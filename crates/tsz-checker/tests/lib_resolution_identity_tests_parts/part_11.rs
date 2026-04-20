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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2345),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2339),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Generic type alias wrapping Promise should resolve.\nDiagnostics: {real_errors:#?}"
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
    let real_errors: Vec<_> = diagnostics.iter().filter(|(c, _)| *c != 2318).collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
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
        has_error(&diagnostics, 2322),
        "Assigning Promise<number> to Promise<string> should produce TS2322.\nDiagnostics: {diagnostics:#?}"
    );
}

