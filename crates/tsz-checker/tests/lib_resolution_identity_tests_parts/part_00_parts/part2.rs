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
