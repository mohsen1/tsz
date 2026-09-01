//! When source and target are same-base Application types AND the target has
//! an `any` arg AND variance metadata is unavailable (e.g. recursive aliases
//! like `FlatArray` whose variance computation bottoms out at empty), the
//! assignability check should accept the `any` position as a universal sink
//! instead of falling through to structural expansion (which can spuriously
//! reject deeply recursive conditional aliases).

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

// `Recur<Arr, Depth>` is a self-referential alias whose body re-applies the
// alias to itself. Variance computation for such recursive aliases bottoms
// out at `Variance::empty()` for every parameter. Without the any-target
// shortcut, the structural fallback would expand the body and spuriously
// reject assignments where the target's arg is `any`.

#[test]
fn recur_x_eq_y_with_any_target_arg_no_error() {
    let diags = diags_strict(
        r#"
type Recur<Arr, Depth extends number> = {
    value: Arr;
    nested: Recur<Arr, Depth>;
};

function f<Arr, D extends number>(x: Recur<Arr, any>, y: Recur<Arr, D>) {
    x = y;
}
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "Expected no TS2322 for `x = y;` where target has `any` arg; got: {diags:?}"
    );
}

#[test]
fn recur_any_target_arg_still_checks_non_any_args() {
    let diags = diags_strict(
        r#"
type Recur<Arr, Depth extends number> = {
    value: Arr;
    nested: Recur<Arr, Depth>;
};

function f<D extends number>(x: Recur<string, any>, y: Recur<number, D>) {
    x = y;
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for incompatible non-any type argument; got: {diags:?}"
    );
}
