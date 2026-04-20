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
        !has_error(&diagnostics, 2322),
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
        !has_error(&diagnostics, 2322),
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
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2769 || *c == 2345)
        .collect();
    assert!(
        errors.is_empty(),
        "Promise.all([p1, p2]) should type-check.\nDiagnostics: {errors:#?}"
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
        !has_error(&diagnostics, 2322),
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
        !has_error(&diagnostics, 2322),
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
        !has_error(&diagnostics, 2339),
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
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339 || *c == 2345)
        .collect();
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
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
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
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
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
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 2322 || *c == 2339)
        .collect();
    assert!(
        errors.is_empty(),
        "Number augmentation via stable resolver should preserve both original \
         and augmented members.\nDiagnostics: {errors:#?}"
    );
}

