//! An instantiation expression `typeof f<X>` used *inline* as a type argument
//! to a built-in utility (`ReturnType<typeof f<X>>`, `Parameters<typeof f<X>>`)
//! must resolve to the instantiated function type, not silently degrade to
//! `any`.
//!
//! The structural rule: an instantiation expression over a value generic whose
//! type query reaches evaluation as `Application(callable, [Args])` without a
//! type-space `DefId` is the (possibly instantiated) callable itself. When the
//! checker has already specialized the callable so the supplied type arguments
//! are vestigial, the application unwraps to that callable; otherwise its
//! type-parameter-bearing signatures are instantiated. Either way the conditional
//! `infer R` in `ReturnType`/`Parameters` sees a function shape, so the
//! mapped-type return preserves its `readonly` / `?` modifier intent.
//!
//! Regression for #10847: the merged fix in #12157 handled the split-alias form
//! (`type F = typeof f<X>; ReturnType<F>`) but the inline form
//! (`ReturnType<typeof f<X>>`) still produced `any`, dropping modifiers.

use crate::context::{CheckerOptions, ScriptTarget};
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, diagnostic_count, load_default_lib_files};

fn strict_diags(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

#[test]
fn inline_return_type_over_instantiation_expression_preserves_readonly_and_optional() {
    let diags = strict_diags(
        r#"
declare function f<T>(x: T): { [K in keyof T]: T[K] };
type Src = { readonly a: number; b?: string };
type RD = ReturnType<typeof f<Src>>;
declare const rd: RD;
rd.a = 5;                 // readonly preserved -> TS2540
const v: string = rd.b;   // optional preserved -> TS2322 (string | undefined)
"#,
    );

    assert_eq!(
        diagnostic_count(&diags, 2540),
        1,
        "readonly modifier must survive the inline instantiation expression: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "optional modifier must survive the inline instantiation expression: {diags:?}"
    );
}

#[test]
fn inline_return_type_does_not_degrade_to_any() {
    // If the return degraded to `any`, assigning it to a literal type would be
    // accepted and this assignment would NOT error. The presence of TS2322
    // proves the return kept its concrete object shape.
    let diags = strict_diags(
        r#"
declare function f<T>(x: T): { [K in keyof T]: T[K] };
type Src = { a: number };
type RD = ReturnType<typeof f<Src>>;
declare const rd: RD;
const probe: 0 = rd;   // object is not assignable to 0 -> TS2322
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "inline ReturnType<typeof f<X>> must not be `any`: {diags:?}"
    );
}

#[test]
fn inline_parameters_over_instantiation_expression_is_concrete() {
    let diags = strict_diags(
        r#"
declare function f<T>(x: T): { [K in keyof T]: T[K] };
type Src = { a: number };
type P = Parameters<typeof f<Src>>;
const bad: number = (null as unknown as P)[0];   // P[0] is Src, not number -> TS2322
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "inline Parameters<typeof f<X>> must yield the concrete tuple [Src]: {diags:?}"
    );
}

#[test]
fn inline_modifier_preservation_is_type_parameter_name_agnostic() {
    // Renamed type parameters (`Item`/`P`) must behave identically: the fix is
    // structural, not keyed on the spelling `T`/`K`.
    let diags = strict_diags(
        r#"
declare function g<Item>(v: Item): { readonly [P in keyof Item]: Item[P] };
type RG = ReturnType<typeof g<{ q: number }>>;
declare const rg: RG;
rg.q = 1;   // readonly preserved -> TS2540
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2540),
        1,
        "renamed type parameters must preserve readonly through the inline form: {diags:?}"
    );
}

#[test]
fn inline_optional_removing_mapped_is_respected() {
    // `-?` strips optionality; the concrete return must report `a` as required
    // `number`, so assigning it to `number` is allowed (no false TS2322).
    let diags = strict_diags(
        r#"
declare function h<T>(x: T): { [K in keyof T]-?: T[K] };
type RH = ReturnType<typeof h<{ a?: number }>>;
declare const rh: RH;
const v: number = rh.a;   // optionality removed -> no error
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "`-?` must strip optionality through the inline form (no false TS2322): {diags:?}"
    );
}

#[test]
fn split_alias_form_remains_correct() {
    // Regression guard for the form fixed by #12157 — must keep working.
    let diags = strict_diags(
        r#"
declare function f<T>(x: T): { [K in keyof T]: T[K] };
type Src = { readonly a: number; b?: string };
type F = typeof f<Src>;
type RD = ReturnType<F>;
declare const rd: RD;
rd.a = 5;                 // TS2540
const v: string = rd.b;   // TS2322
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2540),
        1,
        "split-alias readonly: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        1,
        "split-alias optional: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Invalid inline instantiation expressions must keep their tsc error parity:
// the vestigial-argument unwrap only applies to an already-validated callable,
// so a non-generic value or a wrong-arity generic still reports TS2635 at the
// instantiation expression and TS2344 at the utility constraint — and must NOT
// let `ReturnType` / `Parameters` observe a concrete return downstream.
// ---------------------------------------------------------------------------

#[test]
fn inline_instantiation_on_non_generic_value_keeps_ts2635_ts2344() {
    let diags = strict_diags(
        r#"
declare function nonGeneric(x: number): string;
type RN = ReturnType<typeof nonGeneric<number>>;
declare const rn: RN;
const probe: 0 = rn;   // RN must stay error-like, NOT leak `string` (no TS2322)
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "non-generic inline instantiation must report TS2635: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "ReturnType constraint must report TS2344 for the failed instantiation: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "the failed instantiation must not leak a concrete return downstream: {diags:?}"
    );
}

#[test]
fn inline_instantiation_with_wrong_arity_keeps_ts2635_ts2344() {
    let diags = strict_diags(
        r#"
declare function oneParam<T>(x: T): T;
type RW = ReturnType<typeof oneParam<number, string>>;
declare const rw: RW;
const probe: 0 = rw;   // RW must stay error-like, NOT leak a concrete return
"#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2635),
        1,
        "wrong-arity inline instantiation must report TS2635: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2344),
        1,
        "ReturnType constraint must report TS2344 for the wrong-arity instantiation: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "the wrong-arity instantiation must not leak a concrete return downstream: {diags:?}"
    );
}
