//! A conditional whose EXTENDS-type is a generic *wrapper alias* carrying an
//! `infer` (`X extends AB<infer U> ? U : never`, `type AB<T> = Promise<T[]>`)
//! must bind `U` even when the CHECK-type is the expanded structural form
//! (`Promise<number[]>`) rather than the alias form. tsz previously collapsed the
//! conditional to its `never` false branch — failing to reduce the alias pattern
//! to its application form for structural infer matching — producing a false
//! TS2322 at the use site. Fixed in solver `infer_pattern.rs`; this guards it.
//! (#14489)

use super::super::core::*;

/// The witness: `Ex<Promise<number[]>>` over `AB<infer U> = Promise<T[]>` must
/// resolve to `number`, so `const a: R = 7` type-checks (no false TS2322).
#[test]
fn wrapper_alias_infer_over_expanded_source_binds_u_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<Promise<number[]>>;
const a: R = 7;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `Ex<Promise<number[]>>` binds `U = number` through \
         the wrapper alias `AB<infer U>`, so `R = number` accepts `7`. \
         Actual: {diagnostics:#?}"
    );
}

/// Renamed binders (`Wrap`/`Elem`/`In`, distinct names) guard against any
/// name-based shortcut: the structural reduction must hold regardless of the
/// alias/type-parameter identifiers.
#[test]
fn wrapper_alias_infer_renamed_binders_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Wrap<Elem> = Promise<Elem[]>;
type Pick1<In> = In extends Wrap<infer Out> ? Out : never;
type Res = Pick1<Promise<string[]>>;
const s: Res = "ok";
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — renamed wrapper-alias infer binds `Out = string`. \
         Actual: {diagnostics:#?}"
    );
}

/// Negative control: when the check-type genuinely does NOT match the wrapper
/// (`number` is not a `Promise<T[]>`), the conditional must stay on its false
/// branch (`never`), so assigning a value to the result still errors. The fix
/// reduces the alias pattern for matching — it does not force a match.
#[test]
fn wrapper_alias_infer_non_matching_source_keeps_false_branch_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<number>;
const a: R = 7;
export {};
"#,
    );
    assert!(
        has_error(&diagnostics, 2322),
        "TS2322 expected — `number` does not match `Promise<T[]>`, so `R = never` \
         and `7` is not assignable. Actual: {diagnostics:#?}"
    );
}
