//! `globalThis.X` in type position resolves to the global type `X`. (#14227)

use super::super::core::*;

/// `globalThis.X` in a type position (here a type-predicate's asserted type)
/// must resolve `globalThis` to the synthetic global namespace and `X` to the
/// ambient global type, matching tsc. tsz previously failed to resolve the
/// `globalThis` qualifier and reported a false TS2339 on the narrowed value
/// (typebox). Binder-varied across distinct global types.
#[test]
fn global_this_qualified_type_resolves_no_ts2339() {
    for ty in ["RegExp", "Boolean", "Number"] {
        let source = format!(
            r#"
function isThing(value: unknown): value is globalThis.{ty} {{
  return typeof value === "object";
}}
declare const v: unknown;
function use() {{
  if (isThing(v)) {{
    return v.valueOf();
  }}
  return undefined;
}}
export {{ use }};
"#
        );
        let diagnostics = compile_and_get_diagnostics(&source);
        assert!(
            !has_error(&diagnostics, 2339),
            "[globalThis.{ty}] must resolve to the global type; no TS2339 expected. \
             Diagnostics: {diagnostics:#?}"
        );
        assert!(
            !has_error(&diagnostics, 2304),
            "[globalThis.{ty}] global qualifier must resolve; no TS2304 expected. \
             Diagnostics: {diagnostics:#?}"
        );
    }
}

/// Negative control: an unknown member of `globalThis` must still fail to
/// resolve (the fix routes the member through the normal global type lookup,
/// it does not blanket-accept any `globalThis.X`).
#[test]
fn global_this_unknown_member_still_errors() {
    let diagnostics = compile_and_get_diagnostics(
        r"
type Bad = globalThis.ThisIsDefinitelyNotARealGlobalType;
export type { Bad };
        ",
    );
    let resolved_error = diagnostics
        .iter()
        .any(|(code, _)| *code == 2304 || *code == 2694 || *code == 2552);
    assert!(
        resolved_error,
        "an unknown `globalThis` member must report a not-found error. \
         Diagnostics: {diagnostics:#?}"
    );
}
