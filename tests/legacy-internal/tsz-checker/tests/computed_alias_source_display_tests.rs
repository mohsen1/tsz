//! Tests for issue #10799 (display follow-up): the *source* of a
//! `TS2322`/`TS2345` assignability diagnostic must render a non-generic type
//! alias by its underlying type — not its alias name — when tsc drops the
//! `aliasSymbol` for it.
//!
//! Structural rule: tsc attaches no `aliasSymbol` to the shared result of a
//! non-generic alias whose body is a *computed* operator (conditional /
//! indexed-access / `keyof` / utility application / template / string
//! intrinsic) that collapses to a shared singleton, or a direct
//! intrinsic/literal body. It therefore displays the underlying scalar
//! (`string`, `"yes"`, `never`, …), e.g. `type X1 = true extends true ? string
//! : number` renders as `string`. The solver `TypeFormatter` already honors
//! this; these tests lock in the checker's source-display path, which had been
//! repainting the resolved scalar with the alias annotation name.
//!
//! Adjacent guard: aliases whose body resolves to a *structural* shape
//! (tuple/object/array/union-of-literals) or a generic application keep their
//! alias name, exactly as tsc does — so the rewrite suppression must be scoped
//! to displayed-as-underlying aliases only. Binder names are varied across
//! cases so no test depends on a specific identifier string.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

fn assert_source_display(source: &str, expected_source: &str) {
    let messages = ts2322_messages(source);
    assert!(
        messages
            .iter()
            .any(|m| m.contains(&format!("Type '{expected_source}' is not assignable"))),
        "expected source display '{expected_source}', got: {messages:?}"
    );
}

fn assert_argument_source_display(source: &str, expected_source: &str) {
    let messages: Vec<String> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2345)
        .map(|d| d.message_text)
        .collect();
    assert!(
        messages.iter().any(|m| m.contains(&format!(
            "Argument of type '{expected_source}' is not assignable"
        ))),
        "expected argument source display '{expected_source}', got: {messages:?}"
    );
}

#[test]
fn conditional_scalar_alias_source_renders_underlying() {
    // `Pick_A` reduces to `string`; tsc shows `string`, not `Pick_A`.
    assert_source_display(
        r#"
type Pick_A = true extends true ? string : number;
declare const witness_a: Pick_A;
const sink_a: 0 = witness_a;
"#,
        "string",
    );
}

#[test]
fn conditional_literal_alias_source_renders_literal() {
    assert_source_display(
        r#"
type Branch_B = true extends true ? "yes" : "no";
declare const witness_b: Branch_B;
const sink_b: 0 = witness_b;
"#,
        "\"yes\"",
    );
}

#[test]
fn indexed_access_scalar_alias_source_renders_underlying() {
    assert_source_display(
        r#"
type Lookup_C = { field: string }["field"];
declare const witness_c: Lookup_C;
const sink_c: 0 = witness_c;
"#,
        "string",
    );
}

#[test]
fn application_scalar_alias_source_renders_underlying() {
    // A user-defined utility application (no lib dependency) that bottoms out at
    // a shared scalar drops its alias name, exactly like `ReturnType<…>`.
    assert_source_display(
        r#"
type Unwrap_D<F> = F extends () => infer R ? R : never;
type Returns_D = Unwrap_D<() => string>;
declare const witness_d: Returns_D;
const sink_d: 0 = witness_d;
"#,
        "string",
    );
}

#[test]
fn direct_primitive_alias_source_renders_underlying() {
    assert_source_display(
        r#"
type Plain_E = string;
declare const witness_e: Plain_E;
const sink_e: 0 = witness_e;
"#,
        "string",
    );
}

#[test]
fn alias_chain_to_scalar_source_renders_underlying() {
    // `Outer_F -> Inner_F -> string`: the whole chain drops its names.
    assert_source_display(
        r#"
type Inner_F = true extends true ? string : number;
type Outer_F = Inner_F;
declare const witness_f: Outer_F;
const sink_f: 0 = witness_f;
"#,
        "string",
    );
}

#[test]
fn assertion_source_renders_underlying() {
    // `expr as Alias` source position must also resolve the computed alias.
    assert_source_display(
        r#"
type Cast_G = true extends true ? string : number;
declare const witness_g: Cast_G;
const sink_g: 0 = (witness_g as Cast_G);
"#,
        "string",
    );
}

#[test]
fn tuple_alias_source_keeps_alias_name() {
    // Structural (tuple) body keeps the alias name, matching tsc.
    assert_source_display(
        r#"
type Pair_H = [string, number];
declare const witness_h: Pair_H;
const sink_h: 0 = witness_h;
"#,
        "Pair_H",
    );
}

#[test]
fn object_alias_source_keeps_alias_name() {
    assert_source_display(
        r#"
type Shape_I = { member: number };
declare const witness_i: Shape_I;
const sink_i: 0 = witness_i;
"#,
        "Shape_I",
    );
}

#[test]
fn literal_union_alias_source_keeps_alias_name() {
    // A union of literals is not a single shared singleton — tsc keeps the name.
    assert_source_display(
        r#"
type Choice_J = "a" | "b";
declare const witness_j: Choice_J;
const sink_j: 0 = witness_j;
"#,
        "Choice_J",
    );
}

#[test]
fn generic_application_alias_source_keeps_alias_name() {
    assert_source_display(
        r#"
type Box_K<T> = { value: T };
declare const witness_k: Box_K<string>;
const sink_k: 0 = witness_k;
"#,
        "Box_K<string>",
    );
}

// --- tuple-like-union family (#10799 rxjs-project repro) -------------------
//
// A non-generic alias whose computed body reduces to a tuple, an array, or a
// union of those carries no `aliasSymbol` in tsc, so the *source* of a TS2322
// must render the structural shape — not the alias name. Scalar-bodied aliases
// already expanded; tuple/array bodies regressed because they expose a numeric
// index signature that bypassed the source annotation-preference gate, so the
// declared alias name leaked into the message (`L` instead of `[string, number]`).

#[test]
fn conditional_tuple_alias_source_renders_underlying_tuple() {
    assert_source_display(
        r#"
type Pair_L = true extends true ? [string, number] : never;
declare const witness_l: Pair_L;
const sink_l: 0 = witness_l;
"#,
        "[string, number]",
    );
}

#[test]
fn conditional_tuple_union_alias_source_renders_underlying_union() {
    assert_source_display(
        r#"
type Variant_M = true extends true ? [string] | [number] : never;
declare const witness_m: Variant_M;
const sink_m: 0 = witness_m;
"#,
        "[string] | [number]",
    );
}

#[test]
fn conditional_array_union_alias_source_renders_underlying_union() {
    assert_source_display(
        r#"
type Lists_N = true extends true ? string[] | number[] : never;
declare const witness_n: Lists_N;
const sink_n: 0 = witness_n;
"#,
        "string[] | number[]",
    );
}

#[test]
fn inline_conditional_mixed_tuple_union_source_renders_underlying() {
    // The issue's literal repro family: an inline (non-generic) conditional whose
    // body is a mixed tuple union — including the empty tuple — resolves away with
    // no `aliasSymbol`, so tsc renders the structural union. (A generic *utility
    // application* like `Classify<U>` instead keeps its alias name, since tsc
    // stamps the application result — see `generic_application_alias_*` above.)
    assert_source_display(
        r#"
type Result_O = true extends true ? [string, string] | [number, number] | [] : never;
declare const witness_o: Result_O;
const sink_o: 0 = witness_o;
"#,
        "[string, string] | [number, number] | []",
    );
}

#[test]
fn indexed_access_tuple_alias_source_renders_underlying_tuple() {
    assert_source_display(
        r#"
type Lookup_P = { field: [boolean, string] }["field"];
declare const witness_p: Lookup_P;
const sink_p: 0 = witness_p;
"#,
        "[boolean, string]",
    );
}

#[test]
fn direct_tuple_alias_source_keeps_alias_name() {
    // A directly-written tuple alias is `aliasSymbol`-stamped by tsc, so the
    // source keeps its name — the rewrite must stay scoped to computed bodies.
    assert_source_display(
        r#"
type Pair_Q = [string, number];
declare const witness_q: Pair_Q;
const sink_q: 0 = witness_q;
"#,
        "Pair_Q",
    );
}

#[test]
fn object_mentioning_union_alias_source_keeps_alias_name() {
    // A computed body whose union mentions an object stays deferred to the
    // alias name (the shared-shape reverse-lookup repaint is unsafe to expand);
    // this guards that the tuple/array rewrite does not widen that exclusion.
    assert_source_display(
        r#"
type Mixed_R = true extends true ? { a: 1 } | string : never;
declare const witness_r: Mixed_R;
const sink_r: 0 = witness_r;
"#,
        "Mixed_R",
    );
}

// --- generic conditional/indexed-access applications -----------------------
//
// A *generic* application whose alias body is a conditional or indexed access
// (`Classify<"x">`, `Head<[a, b]>`, `Val<{…}>`) drops tsc's `aliasSymbol` once
// it reduces to a concrete shape, so the diagnostic source must render the
// resolved structural form. The scalar/literal reductions already collapsed to
// a shared singleton; the object/tuple/array reductions retained the
// application surface and leaked the `Classify<"x">` spelling. Binder names are
// varied so the rule is structural, not keyed on `Classify`/`Head`/`Val`.

#[test]
fn generic_conditional_application_to_object_renders_structural() {
    assert_source_display(
        r#"
type Sort_Sa<T> = T extends string ? { s: T } : { n: T };
declare const probe_sa: Sort_Sa<"x">;
const sink_sa: 0 = probe_sa;
"#,
        "{ s: \"x\"; }",
    );
}

#[test]
fn renamed_generic_conditional_application_to_tuple_renders_structural() {
    assert_source_display(
        r#"
type LeadTail_Tb<U> = U extends [infer H, ...infer R] ? [H, ...R] : [];
declare const probe_tb: LeadTail_Tb<[string, number]>;
const sink_tb: 0 = probe_tb;
"#,
        "[string, number]",
    );
}

#[test]
fn generic_conditional_application_to_array_renders_structural() {
    assert_source_display(
        r#"
type Listify_Uc<V> = V extends unknown ? V[] : never;
declare const probe_uc: Listify_Uc<string>;
const sink_uc: 0 = probe_uc;
"#,
        "string[]",
    );
}

#[test]
fn generic_indexed_access_application_to_object_renders_structural() {
    assert_source_display(
        r#"
type Center_Vd<W> = W[keyof W];
declare const probe_vd: Center_Vd<{ only: { z: 1 } }>;
const sink_vd: 0 = probe_vd;
"#,
        "{ z: 1; }",
    );
}

#[test]
fn generic_application_through_alias_chain_renders_structural() {
    // `Outer_We -> Inner_We<…>` (a generic conditional through an alias chain)
    // still drops every name down to the resolved object.
    assert_source_display(
        r#"
type Inner_We<X> = X extends unknown ? { wrapped: X } : never;
type Outer_We<X> = Inner_We<X>;
declare const probe_we: Outer_We<number>;
const sink_we: 0 = probe_we;
"#,
        "{ wrapped: number; }",
    );
}

#[test]
fn generic_conditional_application_argument_position_renders_structural() {
    // The same reduction applies to a TS2345 call-argument source.
    assert_argument_source_display(
        r#"
type Shape_Xf<T> = T extends string ? { s: T } : { n: T };
declare function take_xf(value: 0): void;
declare const probe_xf: Shape_Xf<"y">;
take_xf(probe_xf);
"#,
        "{ s: \"y\"; }",
    );
}

#[test]
fn generic_mapped_application_keeps_alias_name() {
    // A mapped body is not a conditional/indexed access; tsc keeps the
    // homomorphic application name, so the reduction must not fire.
    assert_source_display(
        r#"
type Clone_Yg<T> = { [K in keyof T]: T[K] };
declare const probe_yg: Clone_Yg<{ a: string }>;
const sink_yg: 0 = probe_yg;
"#,
        "Clone_Yg<{ a: string; }>",
    );
}

#[test]
fn generic_distributive_application_renders_distributed_branches() {
    // A *distributive* conditional over a concrete union renders the
    // distributed branch union — tsc resolves each branch and the conditional
    // never stamps the alias onto the result (`{ v: 2; } | { v: 1; }` in tsc).
    // tsz renders the same branch set in check-arg source order; tsc's member
    // order for position-less synthesized branches follows its global type
    // creation order, which tsz does not reproduce (branches with source
    // positions — the discriminated-union case — match tsc's order exactly).
    assert_source_display(
        r#"
type Spread_Zh<T> = T extends unknown ? { v: T } : never;
declare const probe_zh: Spread_Zh<1 | 2>;
const sink_zh: 0 = probe_zh;
"#,
        "{ v: 1; } | { v: 2; }",
    );
}

#[test]
fn unreduced_generic_conditional_application_keeps_alias_name() {
    // A free type parameter leaves the conditional deferred, so tsc keeps the
    // `Defer_Ai<T>` spelling rather than a partially-resolved shape.
    let messages = ts2322_messages(
        r#"
type Defer_Ai<T> = T extends unknown ? { v: T } : never;
function probe_ai<T>(value: Defer_Ai<T>): void {
  const sink_ai: 0 = value;
}
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'Defer_Ai<T>' is not assignable")),
        "expected deferred application to keep its alias name, got: {messages:?}"
    );
}

#[test]
fn free_type_parameter_application_reducing_to_concrete_keeps_alias_name() {
    // Regression for the conformance fixture
    // `genericConditionalConstrainedToUnknownNotAssignableToConcreteObject`: tsc
    // only drops the `aliasSymbol` once an application is instantiated with
    // *concrete* arguments. `Ret_Cj<T>` reduces to a concrete type under `T`'s
    // constraint, but its argument `T` is a free type parameter, so tsc keeps
    // `Ret_Cj<T>` rather than rendering the reduced result. (Mirrors the fixture's
    // `ReturnType<T[M]>`, whose argument `T[M]` reduces to `unknown` with the lib
    // loaded yet stays spelled `ReturnType<T[M]>`.)
    let messages = ts2322_messages(
        r#"
type Ret_Cj<F> = F extends () => infer R ? R : never;
function probe_cj<T extends () => string>(value: Ret_Cj<T>, sink: number) {
  sink = value;
}
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type 'Ret_Cj<T>' is not assignable")),
        "expected free-type-parameter application to keep its alias name, got: {messages:?}"
    );
}

#[test]
fn non_generic_alias_wrapping_conditional_application_keeps_alias_in_identifier_source() {
    // Negative guard for the scope boundary: when a *non-generic* alias wraps a
    // reducible generic application (`type Frozen_Bj = Resolve_Bj<{ a: number }>`)
    // and appears as a bare identifier source, the reduction here is intentionally
    // deferred to the non-generic computed-body path (which currently keeps the
    // `Frozen_Bj` spelling in this position). This proves the generic-application
    // reduction does not leak into the non-generic-alias identifier-source path —
    // that residual is tracked separately. The assertion-source RO case (which the
    // computed-body path *does* render structurally) is covered in
    // `type_alias_computed_display_tests`.
    assert_source_display(
        r#"
type Resolve_Bj<T> = T extends object ? { readonly [K in keyof T]: T[K] } : T;
type Frozen_Bj = Resolve_Bj<{ a: number }>;
declare const probe_bj: Frozen_Bj;
const sink_bj: 0 = probe_bj;
"#,
        "Frozen_Bj",
    );
}
