//! Regression tests for generic-call overload resolution honoring the
//! `strictFunctionTypes` option when comparing callback arguments.
//!
//! Structural rule: when `strictFunctionTypes` is disabled, `tsc` compares
//! function/method parameters bivariantly in *every* relation — its
//! `strictVariance` (in `compareSignaturesRelated`) reads the global option,
//! not which relation is running. A generic call's final "closest-miss"
//! argument check must therefore also be bivariant when the option is off.
//!
//! Before the fix, the generic-call resolver's final check routed callback
//! arguments through the solver's *strict* assignability relation
//! (`CompatChecker::is_assignable_strict`), which forced
//! `strict_function_types = true` regardless of the compiler option. When no
//! inference candidate succeeded outright (the union-param + union-return shape
//! of `PromiseLike.then`), the reported closest miss compared the callback
//! parameter contravariantly and manufactured a spurious `TS2345` that the
//! plain (non-generic) assignability path never reports.
//!
//! Witness (issue #16632): `Promise.all(xs.map(populate)).then(cb)` under
//! `--strict false`. The reduced, lib-independent form used here is a
//! hand-written `PromiseLike`-shaped interface whose `then` parameter is a
//! union `((value: T) => U | Like<U>) | null` — the union parameter plus the
//! union callback return are what force resolution onto the closest-miss path.

use crate::test_utils::check_source_diagnostics;

/// A `PromiseLike`-shaped interface with the union parameter + union return
/// that reproduces the closest-miss path. `like`/`t`/`u` vary the interface and
/// type-parameter binders so the fix cannot be a name-scoped special case.
fn like_iface(like: &str, t: &str, u: &str) -> String {
    format!(
        r#"
interface {like}<{t}> {{
    then<{u} = {t}>(
        cb?: ((value: {t}) => {u} | {like}<{u}>) | null
    ): {like}<{u}>;
}}
"#
    )
}

fn ts2345_messages(diags: &[crate::diagnostics::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn nonstrict_generic_then_accepts_narrower_callback_parameter() {
    // `--strict false` ⇒ strictFunctionTypes off ⇒ bivariant callback params.
    // `b: Like<unknown[]>` binds the callback parameter to `unknown[]`; the
    // supplied callback takes `number[]`. Bivariance accepts it (tsc: clean).
    let source = format!(
        "// @strict: false\n{}\ndeclare const b: Like<unknown[]>;\nb.then((value: number[]) => {{ }});\n",
        like_iface("Like", "T", "U")
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        ts2345_messages(&diags).is_empty(),
        "Expected no TS2345 for a narrower callback parameter under --strict false, got: {:?}",
        ts2345_messages(&diags)
    );
}

#[test]
fn nonstrict_generic_then_bivariance_survives_renamed_binders() {
    // Same shape with every binder renamed — no user-name/file-name literal can
    // be driving the decision.
    let source = format!(
        "// @strict: false\n{}\ndeclare const p: Thenable<unknown[]>;\np.then((incoming: number[]) => {{ }});\n",
        like_iface("Thenable", "Elem", "Res")
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        ts2345_messages(&diags).is_empty(),
        "Expected no TS2345 (renamed binders) under --strict false, got: {:?}",
        ts2345_messages(&diags)
    );
}

#[test]
fn strict_generic_then_still_rejects_contravariant_callback_parameter() {
    // Adjacent negative case: with `--strict true` (strictFunctionTypes on) the
    // same call is a genuine contravariance error — `unknown[]` is not
    // assignable to `number[]`. tsc reports TS2345; the fix must not suppress it.
    let source = format!(
        "// @strict: true\n{}\ndeclare const b: Like<unknown[]>;\nb.then((value: number[]) => {{ }});\n",
        like_iface("Like", "T", "U")
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        !ts2345_messages(&diags).is_empty(),
        "Expected TS2345 under --strict true (strictFunctionTypes contravariance), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nonstrict_generic_then_accepts_widening_callback_parameter() {
    // Positive control: a callback parameter that is a *supertype* of the bound
    // element type is assignable in every mode. This must stay clean before and
    // after the fix, confirming the change only relaxes the previously
    // over-strict direction.
    let source = format!(
        "// @strict: false\n{}\ndeclare const b: Like<number[]>;\nb.then((value: unknown[]) => {{ }});\n",
        like_iface("Like", "T", "U")
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        ts2345_messages(&diags).is_empty(),
        "Expected no TS2345 for a widening callback parameter, got: {:?}",
        ts2345_messages(&diags)
    );
}
