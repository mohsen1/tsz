//! Regressions for TS2344 checks anchored inside `lib.dom.d.ts` after a
//! user file declaration-merges a global lib interface like `Node`.
//!
//! Before the fix, the post-merge re-validation pass (`check_checker_lib_file`
//! in `tsz-cli`) re-resolved every type reference inside the lib's
//! interface declarations, including
//! `HTMLCollectionOf<HTMLElementTagNameMap[K]>`. Constraint validation
//! must check the apparent union of map values: compatible augmentations
//! still pass, while conflicting augmentations should match tsc and report
//! TS2344 at lib lines like `13098:101`.
//!
//! `error_type_constraint_not_satisfied` now suppresses these via
//! `indexed_access_into_object_uniformly_satisfies_constraint`: the
//! type-arg `M[K]` (with M a Lazy reference to a closed object shape) is
//! considered to satisfy the constraint only when every value type in M's
//! shape is structurally assignable to the constraint.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

/// Direct case: declaration-merging a compatible `interface Node` member must
/// not create TS2344 errors at the DOM
/// `HTMLCollectionOf<HTMLElementTagNameMap[K]>` call sites.
#[test]
fn declaration_merge_of_lib_node_does_not_cascade_ts2344_inside_lib() {
    let source = r#"
interface Node { forEachChild(): void }
declare var x: HTMLElement;
x.appendChild(x);
"#;
    let diags = check_strict(source);
    let lib_ts2344: Vec<_> = diags.iter().filter(|(code, _)| *code == 2344).collect();
    assert!(
        lib_ts2344.is_empty(),
        "Declaration-merging `Node` must not cascade TS2344 errors anchored in lib.dom.d.ts; got: {diags:?}"
    );
}

/// User-code case: a plain indexed access `M[K]` over a closed user-defined
/// map type satisfies a function constraint when every map value is
/// structurally a function. The shape-based per-property check covers
/// this case without relying on the lib-context fast path.
#[test]
fn indexed_access_into_user_map_with_function_values_satisfies_function_constraint() {
    let source = r#"
type ReturnTypeOf<F extends (...args: any) => any> = ReturnType<F>;
interface FnMap {
    a: () => number;
    b: () => string;
}
type Picked<K extends keyof FnMap> = ReturnTypeOf<FnMap[K]>;
"#;
    let diags = check_strict(source);
    let ts2344: Vec<_> = diags.iter().filter(|(code, _)| *code == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "FnMap[K] with K extends keyof FnMap must satisfy `(...args: any) => any` because every value is a function; got: {diags:?}"
    );
}

/// Anti-regression: nested generic indexed access `M[T][F]` should still
/// emit TS2344 because the outer operand `M[T]` is itself an unresolved
/// indexed access, not a closed object shape. The carve-out must NOT
/// fire here — tsc reports TS2344 in this exact shape.
#[test]
fn nested_generic_indexed_access_still_reports_ts2344() {
    let source = r#"
type DataFetchFns = {
    Boat: { name: number };
    Plane: { name: number };
};
interface Box<T extends string> {}
type Bad<T extends 'Boat', F extends keyof DataFetchFns[T]> = Box<DataFetchFns[T][F]>;
"#;
    let diags = check_strict(source);
    let ts2344: Vec<_> = diags.iter().filter(|(code, _)| *code == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Nested generic indexed access `DataFetchFns[T][F]` should report TS2344 against Box's `T extends string` constraint; got: {diags:?}"
    );
}
