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
