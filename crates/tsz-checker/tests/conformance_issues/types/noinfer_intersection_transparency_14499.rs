//! `NoInfer<T>` is an inference-only intrinsic: when its inner type `T` is
//! concrete (no free type parameter), it must be TRANSPARENT to every
//! intersection reduction and relation — disjoint members collapse to `never`,
//! `any` absorbs, and object members merge — exactly as if the wrapper were
//! absent. tsz left the wrapper opaque inside intersections, producing tsz-only
//! false TS2322s (and corrupting the `0 extends 1 & NoInfer<T>` `IsAny` idiom).
//! Fixed in solver `intern/intersection.rs` (`normalize_intersection` unwraps a
//! concrete-inner `NoInfer` member for reduction); this guards it. (#14499)

use super::super::core::*;

/// Disjoint unit literals must collapse to `never` through `NoInfer`:
/// `"a" & NoInfer<"b">` is `never`, so it is assignable to a `never` annotation.
#[test]
fn noinfer_disjoint_literals_collapse_to_never_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type R1 = "a" & NoInfer<"b">;
const x1: never = null as any as R1;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `\"a\" & NoInfer<\"b\">` collapses to `never` (disjoint \
         units, NoInfer transparent). Actual: {diagnostics:#?}"
    );
}

/// The `IsAny` idiom: `0 extends 1 & NoInfer<T>` with `T = any` must take the
/// `true` branch (`any` absorbs through `NoInfer`), so `IsAny<any>` is `true`.
#[test]
fn noinfer_any_absorption_isany_idiom_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type IsAny<T> = 0 extends 1 & NoInfer<T> ? true : false;
const a: IsAny<any> = true;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `any` absorbs through `NoInfer`, so `IsAny<any>` is \
         `true`. Actual: {diagnostics:#?}"
    );
}

/// Negative control: the transparency must not over-collapse. The disjoint
/// collapse to `never` is real, so assigning a non-`never` value to the result
/// is rejected (proving the reduction fired, not a blanket suppression).
#[test]
fn noinfer_disjoint_collapse_rejects_non_never_value_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type R1 = "a" & NoInfer<"b">;
const bad: R1 = "a";
export {};
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "TS2322 expected — `\"a\" & NoInfer<\"b\">` is `never`, so the string `\"a\"` \
         is not assignable to it. Actual: {diagnostics:#?}"
    );
}
