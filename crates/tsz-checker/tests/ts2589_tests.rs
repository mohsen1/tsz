//! Tests for TS2589: Type instantiation is excessively deep and possibly infinite.

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::{check_source_code_messages as get_diagnostics, check_source_diagnostics};
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::{PropertyInfo, TupleElement, TypeId, TypeParamInfo};

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn excessively_large_tuple_spreads_report_tuple_size_diagnostics() {
    let source = r#"
type T0 = [any];
type T1 = [...T0, ...T0];
type T2 = [...T1, ...T1];
type T3 = [...T2, ...T2];
type T4 = [...T3, ...T3];
type T5 = [...T4, ...T4];
type T6 = [...T5, ...T5];
type T7 = [...T6, ...T6];
type T8 = [...T7, ...T7];
type T9 = [...T8, ...T8];
type T10 = [...T9, ...T9];
type T11 = [...T10, ...T10];
type T12 = [...T11, ...T11];
type T13 = [...T12, ...T12];
type T14 = [...T13, ...T13];

const a0 = [0] as const;
const a1 = [...a0, ...a0] as const;
const a2 = [...a1, ...a1] as const;
const a3 = [...a2, ...a2] as const;
const a4 = [...a3, ...a3] as const;
const a5 = [...a4, ...a4] as const;
const a6 = [...a5, ...a5] as const;
const a7 = [...a6, ...a6] as const;
const a8 = [...a7, ...a7] as const;
const a9 = [...a8, ...a8] as const;
const a10 = [...a9, ...a9] as const;
const a11 = [...a10, ...a10] as const;
const a12 = [...a11, ...a11] as const;
const a13 = [...a12, ...a12] as const;
const a14 = [...a13, ...a13] as const;
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2799_count = diagnostics.iter().filter(|diag| diag.code == 2799).count();
    let ts2800_count = diagnostics.iter().filter(|diag| diag.code == 2800).count();

    assert_eq!(
        ts2799_count, 1,
        "expected one TS2799 diagnostic, got {diagnostics:?}"
    );
    assert_eq!(
        ts2800_count, 1,
        "expected one TS2800 diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diag| diag.code != 2589),
        "tuple size overflow should not surface as TS2589: {diagnostics:?}"
    );
}

/// Two *independent* type-alias chains that both exceed the 10 000-element
/// limit must each emit their own TS2799.  This mirrors the structure of the
/// `excessivelyLargeTupleSpread` tsc conformance fixture, which has exactly
/// two type-alias chains and one const-array chain.
///
/// Structural rule: TS2799 is emitted independently for every alias whose body
/// directly produces a tuple exceeding the limit, regardless of how many other
/// aliases in the file already exceeded it.
#[test]
fn two_independent_tuple_spread_chains_each_emit_ts2799() {
    // First chain: spread 5 copies per step, reaches 5^5 = 3125 at T4,
    // 5^6 = 15625 at T5 → TS2799.
    // Second chain: double each step, reaches 2^14 = 16384 at U14 → TS2799.
    // (Two-char alias names for the first chain, four-char for the second.)
    let source = r#"
type T0 = [any, any, any, any, any];
type T1 = [...T0, ...T0, ...T0, ...T0, ...T0];
type T2 = [...T1, ...T1, ...T1, ...T1, ...T1];
type T3 = [...T2, ...T2, ...T2, ...T2, ...T2];
type T4 = [...T3, ...T3, ...T3, ...T3, ...T3];
type T5 = [...T4, ...T4, ...T4, ...T4, ...T4];

type U000 = [any];
type U001 = [...U000, ...U000];
type U002 = [...U001, ...U001];
type U003 = [...U002, ...U002];
type U004 = [...U003, ...U003];
type U005 = [...U004, ...U004];
type U006 = [...U005, ...U005];
type U007 = [...U006, ...U006];
type U008 = [...U007, ...U007];
type U009 = [...U008, ...U008];
type U010 = [...U009, ...U009];
type U011 = [...U010, ...U010];
type U012 = [...U011, ...U011];
type U013 = [...U012, ...U012];
type U014 = [...U013, ...U013];

const c0 = [0] as const;
const c1 = [...c0, ...c0] as const;
const c2 = [...c1, ...c1] as const;
const c3 = [...c2, ...c2] as const;
const c4 = [...c3, ...c3] as const;
const c5 = [...c4, ...c4] as const;
const c6 = [...c5, ...c5] as const;
const c7 = [...c6, ...c6] as const;
const c8 = [...c7, ...c7] as const;
const c9 = [...c8, ...c8] as const;
const c10 = [...c9, ...c9] as const;
const c11 = [...c10, ...c10] as const;
const c12 = [...c11, ...c11] as const;
const c13 = [...c12, ...c12] as const;
const c14 = [...c13, ...c13] as const;
"#;

    let diagnostics = check_source_diagnostics(source);
    let ts2799_count = diagnostics.iter().filter(|d| d.code == 2799).count();
    let ts2800_count = diagnostics.iter().filter(|d| d.code == 2800).count();
    assert_eq!(
        ts2799_count, 2,
        "expected two TS2799 diagnostics (one per chain), got {diagnostics:?}"
    );
    assert_eq!(
        ts2800_count, 1,
        "expected one TS2800 diagnostic for the const-array chain, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2589),
        "tuple size overflow must not surface as TS2589: {diagnostics:?}"
    );
}

/// A chain that spreads many copies per step reaches the limit in far fewer
/// alias levels than a doubling chain.  Both paths must emit TS2799 (not
/// TS2589) because the size overflow is always unconditional.
///
/// Structural rule: the diagnostic code depends on the *kind* of overflow
/// (unconditional size vs. recursive depth), not on how fast the size grows.
#[test]
fn fast_spread_factor_reaches_tuple_limit_with_ts2799() {
    // 10 copies per step with a 2-element base: 2, 20, 200, 2000, 20000.
    // V4 = 20 000 > 10 000 → TS2799 at V4.  (V0=[any,any] so each step
    // multiplies by 10 starting from 2, ensuring strict overflow at V4.)
    let source_v = r#"
type V0 = [any, any];
type V1 = [...V0, ...V0, ...V0, ...V0, ...V0, ...V0, ...V0, ...V0, ...V0, ...V0];
type V2 = [...V1, ...V1, ...V1, ...V1, ...V1, ...V1, ...V1, ...V1, ...V1, ...V1];
type V3 = [...V2, ...V2, ...V2, ...V2, ...V2, ...V2, ...V2, ...V2, ...V2, ...V2];
type V4 = [...V3, ...V3, ...V3, ...V3, ...V3, ...V3, ...V3, ...V3, ...V3, ...V3];
"#;
    let diags_v = check_source_diagnostics(source_v);
    let ts2799 = diags_v.iter().filter(|d| d.code == 2799).count();
    assert_eq!(
        ts2799, 1,
        "10-spread chain: expected exactly one TS2799, got {diags_v:?}"
    );
    assert!(
        diags_v.iter().all(|d| d.code != 2589),
        "10-spread overflow must not surface as TS2589: {diags_v:?}"
    );

    // 3 copies per step — same rule, different spread factor.
    let source_w = r#"
type W0 = [any, any];
type W1 = [...W0, ...W0, ...W0];
type W2 = [...W1, ...W1, ...W1];
type W3 = [...W2, ...W2, ...W2];
type W4 = [...W3, ...W3, ...W3];
type W5 = [...W4, ...W4, ...W4];
type W6 = [...W5, ...W5, ...W5];
type W7 = [...W6, ...W6, ...W6];
type W8 = [...W7, ...W7, ...W7];
"#;
    let diags_w = check_source_diagnostics(source_w);
    let ts2799_w = diags_w.iter().filter(|d| d.code == 2799).count();
    assert_eq!(
        ts2799_w, 1,
        "3-spread chain: expected exactly one TS2799, got {diags_w:?}"
    );
    assert!(
        diags_w.iter().all(|d| d.code != 2589),
        "3-spread overflow must not surface as TS2589: {diags_w:?}"
    );
}

/// Named rest-element spreads (`[...name: T]`) are syntactically different from
/// anonymous spreads (`[...T]`) but semantically identical — both spread the
/// elements of another tuple type into a new tuple.  The limit check must apply
/// equally to both forms.
///
/// Structural rule: when a tuple element carries `...name:` instead of a bare
/// `...`, the element is still a spread; `NAMED_TUPLE_MEMBER` with
/// `dot_dot_dot_token` must be treated the same as `REST_TYPE` throughout the
/// large-tuple detection code.
#[test]
fn named_rest_member_spread_chain_reports_ts2799_not_ts2589() {
    // Anonymous rest spread — baseline.
    let source_anon = r#"
type A0 = [any];
type A1 = [...A0, ...A0];
type A2 = [...A1, ...A1];
type A3 = [...A2, ...A2];
type A4 = [...A3, ...A3];
type A5 = [...A4, ...A4];
type A6 = [...A5, ...A5];
type A7 = [...A6, ...A6];
type A8 = [...A7, ...A7];
type A9 = [...A8, ...A8];
type A10 = [...A9, ...A9];
type A11 = [...A10, ...A10];
type A12 = [...A11, ...A11];
type A13 = [...A12, ...A12];
type A14 = [...A13, ...A13];
"#;
    let diags_anon = check_source_diagnostics(source_anon);
    assert_eq!(
        diags_anon.iter().filter(|d| d.code == 2799).count(),
        1,
        "anonymous rest: expected one TS2799, got {diags_anon:?}"
    );
    assert!(
        diags_anon.iter().all(|d| d.code != 2589),
        "anonymous rest: must not emit TS2589: {diags_anon:?}"
    );

    // Named rest spreads — must behave identically.
    let source_named = r#"
type N0 = [any];
type N1 = [...a: N0, ...b: N0];
type N2 = [...c: N1, ...d: N1];
type N3 = [...e: N2, ...f: N2];
type N4 = [...g: N3, ...h: N3];
type N5 = [...i: N4, ...j: N4];
type N6 = [...k: N5, ...l: N5];
type N7 = [...m: N6, ...n: N6];
type N8 = [...o: N7, ...p: N7];
type N9 = [...q: N8, ...r: N8];
type N10 = [...s: N9, ...t: N9];
type N11 = [...u: N10, ...v: N10];
type N12 = [...w: N11, ...x: N11];
type N13 = [...y: N12, ...z: N12];
type N14 = [...aa: N13, ...bb: N13];
"#;
    let diags_named = check_source_diagnostics(source_named);
    assert_eq!(
        diags_named.iter().filter(|d| d.code == 2799).count(),
        1,
        "named rest: expected one TS2799, got {diags_named:?}"
    );
    assert!(
        diags_named.iter().all(|d| d.code != 2589),
        "named rest: must not emit TS2589: {diags_named:?}"
    );
}

/// A chain that mixes anonymous and named rest spreads within the same alias
/// body must still be detected as an unconditional tuple-spread chain and
/// produce TS2799 when the element count exceeds the limit.
#[test]
fn mixed_anonymous_and_named_rest_spreads_report_ts2799() {
    let source = r#"
type M0 = [any];
type M1 = [...M0, ...x: M0];
type M2 = [...M1, ...y: M1];
type M3 = [...M2, ...z: M2];
type M4 = [...M3, ...w: M3];
type M5 = [...M4, ...v: M4];
type M6 = [...M5, ...u: M5];
type M7 = [...M6, ...t: M6];
type M8 = [...M7, ...s: M7];
type M9 = [...M8, ...r: M8];
type M10 = [...M9, ...q: M9];
type M11 = [...M10, ...p: M10];
type M12 = [...M11, ...o: M11];
type M13 = [...M12, ...n: M12];
type M14 = [...M13, ...m: M13];
"#;
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2799).count(),
        1,
        "mixed rest: expected one TS2799, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2589),
        "mixed rest: must not emit TS2589: {diagnostics:?}"
    );
}

/// A chain where each alias spreads a starting tuple with multiple elements
/// (not just one) reaches the limit more quickly.  The count of elements in the
/// starting tuple is irrelevant to the diagnostic — what matters is whether the
/// final count exceeds `MAX_REPRESENTABLE_TUPLE_LENGTH`.
#[test]
fn multi_element_base_tuple_spread_reports_ts2799() {
    // Base tuple has 100 elements.  After 3 doublings: 800.  After 4: 1600.
    // After 7 doublings: 100 * 2^7 = 12800 > 10000 → TS2799.
    let source = r#"
type B0 = [
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any,
    any, any, any, any, any, any, any, any, any, any
];
type B1 = [...B0, ...B0];
type B2 = [...B1, ...B1];
type B3 = [...B2, ...B2];
type B4 = [...B3, ...B3];
type B5 = [...B4, ...B4];
type B6 = [...B5, ...B5];
type B7 = [...B6, ...B6];
"#;
    let diagnostics = check_source_diagnostics(source);
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2799).count(),
        1,
        "multi-element base: expected one TS2799, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2589),
        "multi-element base: must not emit TS2589: {diagnostics:?}"
    );
}

/// Aliases whose body is not a tuple spread (e.g. a type reference to a
/// too-large alias, or a non-tuple type) must NOT inherit TS2799.
///
/// Structural rule: TS2799 fires only at the alias that *directly constructs*
/// the over-limit tuple through spread syntax; downstream aliases that merely
/// name or wrap it are not flagged.
#[test]
fn non_spread_aliases_referencing_large_tuple_do_not_inherit_ts2799() {
    // Indirect reference: type Ref = T14.  Ref's body is a type-reference node,
    // not a tuple-spread — tsc does not emit TS2799 here.
    let source = r#"
type X0 = [any];
type X1 = [...X0, ...X0];
type X2 = [...X1, ...X1];
type X3 = [...X2, ...X2];
type X4 = [...X3, ...X3];
type X5 = [...X4, ...X4];
type X6 = [...X5, ...X5];
type X7 = [...X6, ...X6];
type X8 = [...X7, ...X7];
type X9 = [...X8, ...X8];
type X10 = [...X9, ...X9];
type X11 = [...X10, ...X10];
type X12 = [...X11, ...X11];
type X13 = [...X12, ...X12];
type X14 = [...X13, ...X13];
type Ref = X14;
type Wrapped = [X14];
"#;
    let diagnostics = check_source_diagnostics(source);
    // Only X14 should get TS2799; none of X0..X13 should (they're under the
    // limit), and Ref/Wrapped must not inherit it through non-spread bodies.
    assert_eq!(
        diagnostics.iter().filter(|d| d.code == 2799).count(),
        1,
        "only the first over-limit alias should emit TS2799; got {diagnostics:?}"
    );
    // X0..X13 must be clean.
    assert!(
        diagnostics.iter().all(|d| d.code != 2589),
        "no TS2589 should appear: {diagnostics:?}"
    );
}

#[test]
fn property_access_ts2589_recovers_variable_symbol_type_to_any() {
    let source = r#"
type T2<K extends "x" | "y"> = {
    x: T2<K>[K];
    y: number;
};

declare let x2: T2<"x">;
let x2x = x2.x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sym_id = binder.file_locals.get("x2x").expect("expected x2x symbol");

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    assert!(
        checker.ctx.diagnostics.iter().any(|d| d.code == 2589),
        "x2.x should emit TS2589 while recovering the recursive indexed access to any"
    );

    let decl_idx = binder
        .get_symbol(sym_id)
        .and_then(|symbol| symbol.declarations.first().copied())
        .expect("x2x declaration");
    let decl_node = parser.get_arena().get(decl_idx).expect("x2x decl node");
    let decl = parser
        .get_arena()
        .get_variable_declaration(decl_node)
        .expect("x2x decl data");
    let initializer_cached = checker
        .ctx
        .node_types
        .get(&decl.initializer.0)
        .copied()
        .map(|ty| checker.format_type(ty))
        .unwrap_or_else(|| "<missing>".to_string());
    let decl_cached = checker
        .ctx
        .node_types
        .get(&decl_idx.0)
        .copied()
        .map(|ty| checker.format_type(ty))
        .unwrap_or_else(|| "<missing>".to_string());
    let symbol_cached = checker
        .ctx
        .symbol_types
        .get(&sym_id)
        .copied()
        .map(|ty| checker.format_type(ty))
        .unwrap_or_else(|| "<missing>".to_string());

    assert_eq!(
        symbol_cached, "any",
        "x2x symbol cache should recover to any after TS2589 (decl_cached={decl_cached}, initializer_cached={initializer_cached})"
    );
    assert_eq!(
        decl_cached, "any",
        "x2x declaration cache should recover to any after TS2589 (symbol_cached={symbol_cached}, initializer_cached={initializer_cached})"
    );

    let symbol_type = checker.get_type_of_symbol(sym_id);
    assert_eq!(
        checker.format_type(symbol_type),
        "any",
        "x2x lookup should recover to any after TS2589 (symbol_cached={symbol_cached}, decl_cached={decl_cached}, initializer_cached={initializer_cached})"
    );
}

#[test]
fn recursive_type_alias_emits_ts2589() {
    // type Foo<T, B> = { "true": Foo<T, Foo<T, B>> }[T] is infinitely recursive
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    assert!(
        has_error_with_code(source, 2589),
        "Should emit TS2589 for infinitely recursive type alias instantiation"
    );
}

#[test]
fn recursive_type_alias_ts2589_at_usage_not_definition() {
    // TS2589 should be at the usage site (f1's type annotation), not the definition
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    let diags = get_diagnostics(source);
    let ts2589_count = diags.iter().filter(|d| d.0 == 2589).count();
    // Expect exactly 1 TS2589 (at the usage), not 2 (at definition + usage)
    assert_eq!(
        ts2589_count, 1,
        "TS2589 should be emitted once at the usage site, got {ts2589_count}"
    );
}

#[test]
fn non_recursive_type_alias_no_ts2589() {
    // A non-recursive generic type alias should not trigger TS2589
    let source = r#"
type Wrapper<T> = { value: T };
let w: Wrapper<string>;
"#;
    assert!(
        !has_error_with_code(source, 2589),
        "Should NOT emit TS2589 for non-recursive type alias"
    );
}

#[test]
fn shallow_recursive_type_alias_no_ts2589() {
    // A type alias that is self-referential but bounded (via conditional) should not trigger TS2589
    // if the recursion terminates before hitting the depth limit
    let source = r#"
type StringOnly<T> = T extends string ? T : never;
let s: StringOnly<"hello">;
"#;
    assert!(
        !has_error_with_code(source, 2589),
        "Should NOT emit TS2589 for bounded conditional type"
    );
}

#[test]
fn ts2589_message_text() {
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    let diags = get_diagnostics(source);
    let ts2589 = diags.iter().find(|d| d.0 == 2589);
    assert!(ts2589.is_some(), "TS2589 should be emitted");
    assert_eq!(
        ts2589.unwrap().1,
        "Type instantiation is excessively deep and possibly infinite."
    );
}

/// TS2615: circular mapped type in a type alias with indexed access should
/// emit both TS2589 and TS2615 when the mapped type constraint resolves to
/// a concrete string literal key.
///
/// Repro from microsoft/TypeScript#30050 (`recursivelyExpandingUnionNoStackoverflow.ts`):
/// `type N<T, K extends string> = T | { [P in K]: N<T, K> }[K];`
/// `type M = N<number, "M">;`
///
/// tsc emits TS2615 alongside TS2589 because `K = "M"` is a concrete key.
#[test]
fn circular_mapped_type_alias_emits_ts2615_alongside_ts2589() {
    let source = r#"
type N<T, K extends string> = T | { [P in K]: N<T, K> }[K];
type M = N<number, "M">;
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2589),
        "Should emit TS2589 for excessively deep type instantiation, got: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 2615),
        "Should emit TS2615 for circular mapped type property, got: {diags:?}"
    );
    let ts2615 = diags.iter().find(|d| d.0 == 2615).unwrap();
    assert!(
        ts2615.1.contains("'M'"),
        "TS2615 message should reference property 'M', got: {}",
        ts2615.1
    );
    assert!(
        ts2615.1.contains(r#"[P in "M"]"#),
        "TS2615 message should include mapped type with quoted key, got: {}",
        ts2615.1
    );
}

/// TS2615 should NOT be emitted when the mapped type constraint resolves to
/// multiple keys (e.g., `keyof T`). In that case, tsc only emits TS2589.
#[test]
fn circular_mapped_type_alias_no_ts2615_for_keyof_constraint() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
type tup = [number, number, number, number];
function foo(arg: Circular<tup>): tup {
    return arg;
}
"#;
    let diags = get_diagnostics(source);
    // tsc does not emit TS2615 for `Circular<tup>` because `keyof tup`
    // doesn't resolve to a single concrete string literal key.
    // (The interface-level TS2615 is a separate check in interface_checks.rs.)
    // Here we only verify the type-alias-application path doesn't false-positive.
    let alias_ts2615 = diags
        .iter()
        .filter(|d| d.0 == 2615 && d.1.contains("'?'"))
        .count();
    assert_eq!(
        alias_ts2615, 0,
        "Should NOT emit TS2615 with '?' placeholder for keyof constraint, got: {diags:?}"
    );
}

/// A recursive mapped type whose template contains the alias itself in a union
/// with a ground type should NOT emit TS2589.  tsc handles this coinductively.
///
/// Regression for: <https://github.com/mohsen1/tsz/issues/6169>
#[test]
fn recursive_mapped_type_with_union_ground_type_no_ts2589() {
    let source = r#"
type RecursiveRecord<K extends string, V> = {
    [P in K]: V | RecursiveRecord<K, V>;
};
const rec: RecursiveRecord<string, number> = {
    a: 1,
    b: { c: 2, d: { e: 3 } },
};
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "RecursiveRecord<K,V> should NOT emit TS2589; got: {diags:?}"
    );
}

/// A mapped type whose template IS purely the self-reference (no ground union)
/// should also not emit TS2589 — tsc uses coinductive handling for it too.
#[test]
fn purely_self_referential_mapped_type_no_ts2589() {
    let source = r#"
type Circular<T> = { [P in keyof T]: Circular<T> };
const x: Circular<{ a: string }> = { a: { a: { a: {} as any } } };
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "A direct-body mapped type alias should NOT emit TS2589; got: {diags:?}"
    );
}

/// Regression test for react16.d.ts infinite loop.
///
/// Deeply-nested generic types with cross-referencing interfaces and type
/// aliases (like React's `InferProps`, `RequiredKeys`, Validator chains) used to
/// cause `evaluate_application_type` to recurse unboundedly because:
/// - Per-context `instantiation_depth` resets on cross-arena delegation
/// - The worklist in `ensure_application_symbols_resolved` expanded transitively
///
/// This test creates a simplified version of the pathological pattern and
/// verifies it completes within a reasonable time (< 5 seconds).
#[test]
fn deeply_nested_generics_do_not_hang() {
    use std::time::Instant;

    // This pattern mimics react16.d.ts: multiple generic interfaces that
    // cross-reference each other through type parameters, creating a deep
    // expansion graph when types are eagerly resolved.
    let source = r#"
interface Validator<T> {
    validate(props: T): Error | null;
}

type ValidationMap<T> = { [K in keyof T]?: Validator<T[K]> };

type InferType<V> = V extends Validator<infer T> ? T : any;

type InferProps<V extends ValidationMap<any>> = {
    [K in keyof V]: InferType<V[K]>;
};

type RequiredKeys<V extends ValidationMap<any>> = {
    [K in keyof V]: V[K] extends Validator<infer T> ? K : never;
}[keyof V];

interface Requireable<T> extends Validator<T | undefined | null> {
    isRequired: Validator<T>;
}

interface ReactPropTypes {
    any: Requireable<any>;
    array: Requireable<any[]>;
    bool: Requireable<boolean>;
    func: Requireable<(...args: any[]) => any>;
    number: Requireable<number>;
    object: Requireable<object>;
    string: Requireable<string>;
    node: Requireable<any>;
    element: Requireable<any>;
}

declare const PropTypes: ReactPropTypes;

type MyProps = InferProps<{
    name: typeof PropTypes.string;
    count: typeof PropTypes.number;
    items: typeof PropTypes.array;
}>;

let p: MyProps;
"#;

    let start = Instant::now();
    let _diags = get_diagnostics(source);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "Deeply nested generics took {elapsed:?} — should complete in < 5s (was hanging before fix)"
    );
}

/// TS2589 at type alias DEFINITION site (no usage needed).
/// tsc emits TS2589 for `type Foo<T> = T extends unknown ? Foo<T> : unknown`
/// at the definition because the conditional body is infinitely recursive.
#[test]
fn recursive_conditional_type_alias_definition_emits_ts2589() {
    let source = r#"
type Foo<T> = T extends unknown
  ? unknown extends `${infer $Rest}`
    ? Foo<T>
    : Foo<unknown>
  : unknown;
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2589),
        "Should emit TS2589 for recursive conditional type alias at definition site. Got: {diags:?}"
    );
}

/// TS2589 at a type alias definition is anchored at the LAST recursive
/// self-reference in source order — the same node `tsc` reports against
/// (its `currentNode` when `instantiationDepth === 100` fires while
/// instantiating the alias body).
///
/// `forEachChild` visits conditional-type children in
/// check→extends→true→false order, so for `type Foo<T> = T extends U ?
/// Foo<T> : Foo<unknown>;` the anchor lands on `Foo<unknown>` (false
/// branch) rather than the body's first token or the alias name.
///
/// This locks in the anchor used by the
/// `compiler/recursiveConditionalCrash4.ts` conformance fixture.
#[test]
fn recursive_conditional_type_alias_anchors_ts2589_at_last_self_reference() {
    let source = r#"type Foo<T> = T extends unknown
  ? unknown extends `${infer $Rest}`
    ? Foo<T>
    : Foo<unknown>
  : unknown;
"#;
    let diags = check_source_diagnostics(source);
    let ts2589 = diags
        .iter()
        .find(|d| d.code == 2589)
        .expect("TS2589 should be emitted for recursive conditional alias");

    // The anchor must point at the start of `Foo<unknown>` — the last
    // self-reference in source order, matching tsc's currentNode.
    let start = ts2589.start as usize;
    let snippet = &source[start..(start + "Foo<unknown>".len()).min(source.len())];
    assert_eq!(
        snippet,
        "Foo<unknown>",
        "TS2589 should anchor at the last recursive self-reference (`Foo<unknown>`). \
         Anchor was at byte {start}: {:?}, full diag: {ts2589:?}",
        &source[start..(start + 20).min(source.len())]
    );
    // Length should cover only the type reference, not the entire body.
    assert_eq!(
        ts2589.length as usize,
        "Foo<unknown>".len(),
        "TS2589 span should cover only the type reference, got {}",
        ts2589.length
    );
}

/// Valid bounded recursive types should NOT trigger TS2589 at definition.
#[test]
fn bounded_recursive_conditional_no_ts2589_at_definition() {
    let source = r#"
type TrimLeft<T extends string> = T extends ` ${infer R}` ? TrimLeft<R> : T;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit TS2589 for bounded recursive conditional type. Got: {diags:?}"
    );
}

#[test]
fn non_conditional_recursive_alias_with_unresolved_qualified_arg_no_ts2589() {
    let source = r#"
type Foo<T> = Foo<Bar.Baz<T>>;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "non-conditional recursive aliases should not hit conditional-body TS2589 definition checks. Got: {diags:?}"
    );
}

#[test]
fn bounded_recursive_alias_with_indexed_type_parameter_arg_no_ts2589() {
    let source = r#"
type Append<Arr extends any[]> = [...Arr, 0];

interface PathsOptions {
    depth: any[];
}

type RecursivePaths<Value, CallOptions extends PathsOptions> =
    CallOptions["depth"]["length"] extends 3
        ? never
        : Value extends object
            ? {
                [Key in keyof Value]: RecursivePaths<
                    Value[Key],
                    { depth: Append<CallOptions["depth"]> }
                >
            }[keyof Value]
            : never;

type Paths<Type> = RecursivePaths<Type, { depth: [] }>;
type Example = Paths<{ a: { b: string } }>;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit TS2589 while resolving recursive alias args that still reference scoped type parameters. Got: {diags:?}"
    );
}

#[test]
fn recursive_conditional_alias_with_parameter_dependent_helper_args_no_definition_ts2589() {
    let source = r#"
type Wrap<T> = T;
type Step<N extends number> = Wrap<N>;
type Combine<Left extends number, Right extends number> = number;

type TailRec<
    Num extends number,
    Count extends number,
    Result extends number,
> = number extends Count
    ? number
    : Count extends 0
    ? Result
    : TailRec<Num, Step<Count>, Combine<Result, Num>>;

type Use<Num extends number, Count extends number> = TailRec<Num, Count, 1>;
"#;
    let diags = get_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.0 == 2589),
        "Should NOT emit definition-site TS2589 for recursive conditional aliases whose recursive type arguments still depend on scoped type parameters through helper aliases. Got: {diags:?}"
    );
}

#[test]
fn recursive_mapped_tuple_spread_depth_shape_is_detected() {
    let types = TypeInterner::new();
    let elements = types.type_param(TypeParamInfo {
        name: types.intern_string("Elements"),
        constraint: Some(types.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
    });
    let bar = types.intern_string("bar");

    let source_bar = types.index_access(elements, TypeId::NUMBER);
    let source_elem = types.object(vec![PropertyInfo {
        name: bar,
        type_id: source_bar,
        write_type: source_bar,
        ..PropertyInfo::default()
    }]);
    let spread_type = types.array(source_elem);

    let tuple = types.tuple(vec![
        TupleElement {
            type_id: elements,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: types.literal_string("abc"),
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let expected_bar = types.index_access(tuple, types.literal_string("0"));
    let expected_type = types.object(vec![PropertyInfo {
        name: bar,
        type_id: expected_bar,
        write_type: expected_bar,
        ..PropertyInfo::default()
    }]);

    assert!(
        CheckerState::recursive_mapped_tuple_spread_may_exceed_depth_in_types(
            &types,
            spread_type,
            expected_type,
        ),
        "Should detect the collapsed mapped tuple spread shape that needs TS2589 recovery"
    );
}

mod issue_6761 {
    //! Tests for <https://github.com/mohsen1/tsz/issues/6761>.
    //!
    //! Structural rule: the `TypeEvaluator`'s per-`TypeId` recursion guard
    //! is a stack-protection limit, not tsc's `instantiationDepth`. When it
    //! trips, we surface TS2589 only when the per-DefId expansion counter
    //! confirms a real instantiation runaway; otherwise we treat the bailout
    //! as the cost of legitimate finite recursion and leave the type opaque.
    use std::sync::{Arc, OnceLock};

    use tsz_binder::lib_loader::LibFile;

    use crate::context::CheckerOptions;
    use crate::test_utils::{
        check_source_with_libs, diagnostics_with_code, has_diagnostic_code, load_default_lib_files,
    };

    fn check_with_libs(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
        static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
        let libs = LIBS.get_or_init(load_default_lib_files);
        check_source_with_libs(source, "test.ts", CheckerOptions::default(), libs)
    }

    #[test]
    fn permutation_no_ts2589() {
        let diags = check_with_libs(
            r#"
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
    ? [K, ...Permutation<Exclude<T, K>>]
    : never;
type Perm1 = Permutation<"A" | "B" | "C">;
type Perm2 = Permutation<"A">;
"#,
        );
        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Permutation<U> should NOT emit TS2589 for small unions; got: {ts2589:?}"
        );
    }

    #[test]
    fn combination_no_ts2589() {
        let diags = check_with_libs(
            r#"
type Combination<T extends string, U extends string = T> =
  T extends unknown
    ? T | `${T} ${Combination<Exclude<U, T>>}`
    : never;
type C2 = Combination<'a' | 'b'>;
"#,
        );
        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Combination<U> should NOT emit TS2589 for small unions; got: {ts2589:?}"
        );
    }

    #[test]
    fn permutation_renamed_params_no_ts2589() {
        let diags = check_with_libs(
            r#"
type Permute<X, Y = X> = [X] extends [never]
  ? []
  : Y extends Y
    ? [Y, ...Permute<Exclude<X, Y>>]
    : never;
type P = Permute<"A" | "B" | "C">;
"#,
        );
        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Permute<X, Y = X> must NOT emit TS2589 regardless of param names; got: {ts2589:?}"
        );
    }

    #[test]
    fn combination_renamed_alias_no_ts2589() {
        let diags = check_with_libs(
            r#"
type Mix<A extends string, B extends string = A> =
  A extends unknown
    ? A | `${A} ${Mix<Exclude<B, A>>}`
    : never;
type R = Mix<'x' | 'y'>;
"#,
        );
        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Mix<A, B = A> must NOT emit TS2589 with renamed alias/params; got: {ts2589:?}"
        );
    }

    /// Regression guard: the discriminator must keep surfacing TS2589 when
    /// the per-DefId expansion counter clears `REAL_INSTANTIATION_BAILOUT_THRESHOLD`.
    #[test]
    fn unbounded_doubling_recursion_still_emits_ts2589() {
        let diags = check_with_libs(
            r#"
type Doubler<T extends "yes", B> = { "yes": Doubler<T, Doubler<T, B>> }[T];
let bad: Doubler<"yes", {}>;
"#,
        );
        assert!(
            has_diagnostic_code(&diags, 2589),
            "Doubling recursion through alias must still emit TS2589; got: {diags:?}"
        );
    }

    /// Performance regression guard for the structural silent-bail policy.
    ///
    /// Mirrors `ts-toolbelt`'s `Any/Compute.ts` body: a recursive mapped /
    /// conditional alias whose tree, evaluated against a placeholder type
    /// parameter, trips the per-`TypeId` structural recursion guard well
    /// before any `def_depth` real-instantiation pressure exists. Before this
    /// rule's silent-bail signal was propagated through `EvalWithCacheResult`,
    /// callers that ran a follow-up `CheckerContext`-resolver pass re-walked
    /// the entire structural tree at the same shape and burned multi-second
    /// time per file. The check below caps wall time so a regression to that
    /// double-walk behavior fails loudly. Wall-time budgets in compiler tests
    /// are an unusual choice; here the cap is generous (10× the observed
    /// stable single-file budget on a slow CI runner) so it only fires on
    /// algorithmic regressions, not noise.
    #[test]
    fn recursive_mapped_alias_body_check_is_fast_no_ts2589() {
        use std::time::Instant;
        let start = Instant::now();
        let diags = check_with_libs(
            r#"
type BuiltIn = Function | Error | Date | RegExp;
type Key = string | number | symbol;
type Has<U, U1> = [U1] extends [U] ? 1 : 0;
type If<B extends 0 | 1, Then, Else = never> = B extends 1 ? Then : Else;

type ComputeRaw<A> = A extends Function ? A : { [K in keyof A]: A[K] } & unknown;

type ComputeFlat<A> =
    A extends BuiltIn ? A :
    A extends Array<any>
    ? A extends Array<Record<Key, any>>
      ? Array<{ [K in keyof A[number]]: A[number][K] } & unknown>
      : A
    : A extends ReadonlyArray<any>
      ? A extends ReadonlyArray<Record<Key, any>>
        ? ReadonlyArray<{ [K in keyof A[number]]: A[number][K] } & unknown>
        : A
      : { [K in keyof A]: A[K] } & unknown;

type ComputeDeep<A, Seen = never> =
    A extends BuiltIn ? A : If<Has<Seen, A>, A, (
      A extends Array<any>
      ? A extends Array<Record<Key, any>>
        ? Array<{ [K in keyof A[number]]: ComputeDeep<A[number][K], A | Seen> } & unknown>
        : A
      : A extends ReadonlyArray<any>
        ? A extends ReadonlyArray<Record<Key, any>>
          ? ReadonlyArray<{ [K in keyof A[number]]: ComputeDeep<A[number][K], A | Seen> } & unknown>
          : A
        : { [K in keyof A]: ComputeDeep<A[K], A | Seen> } & unknown
    )>;

type Compute<A, depth extends 'flat' | 'deep' = 'deep'> = {
    flat: ComputeFlat<A>,
    deep: ComputeDeep<A>,
}[depth];
"#,
        );
        let elapsed = start.elapsed();

        // Permutation-style finite recursion: TS2589 must stay quiet.
        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Recursive mapped / conditional alias body must NOT emit TS2589; got: {ts2589:?}"
        );

        // Algorithmic regression guard. macOS ARM observation in dist /
        // native-cpu is ~50ms; the unoptimized test profile lands around
        // 160ms locally. The pre-fix CI Linux observation was ~3.6s. 2s
        // is well clear of both runner noise plus first-test lib-load
        // amortization, while still failing loudly on a regression to the
        // redundant resolver-pass double-walk.
        assert!(
            elapsed.as_millis() < 2000,
            "Recursive mapped alias body check took {elapsed:?}, expected < 2s. \
             Possible regression to the redundant CheckerContext resolver pass \
             after a structural silent-bail in TypeEvaluator."
        );
    }

    /// Same algorithmic invariant as above for the `ts-toolbelt`
    /// `Object/Invert.ts` shape: a distributive conditional that delegates to
    /// a generic helper which composes mapped + `IndexAccess` + intersection of
    /// a union member projection. Pre-fix this also triggered the redundant
    /// double-walk through the silent-bail result.
    #[test]
    fn recursive_invert_alias_body_check_is_fast_no_ts2589() {
        use std::time::Instant;
        let start = Instant::now();
        let diags = check_with_libs(
            r#"
type Key = string | number | symbol;
type IntersectOf<U> = (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never;
type ComputeRaw<A> = A extends Function ? A : { [K in keyof A]: A[K] } & unknown;

type _Invert<O extends Record<Key, Key>> =
  ComputeRaw<IntersectOf<
    { [K in keyof O]: Record<O[K], K> }[keyof O]
  >>;

type Invert<O extends Record<keyof O, Key>> =
  O extends unknown ? _Invert<O> : never;
"#,
        );
        let elapsed = start.elapsed();

        let ts2589 = diagnostics_with_code(&diags, 2589);
        assert!(
            ts2589.is_empty(),
            "Invert alias body must NOT emit TS2589; got: {ts2589:?}"
        );
        assert!(
            elapsed.as_millis() < 2000,
            "Invert alias body check took {elapsed:?}, expected < 2s."
        );
    }
}

/// TS2799 false positive: Permutation type should NOT trigger "tuple too large"
/// for a small union. For `Permutation<"a" | "b">`, there are only 2 permutations
/// and the recursion terminates in a few steps. tsc accepts this without error.
///
/// Repro for: <https://github.com/mohsen1/tsz/issues/6515>
#[test]
fn permutation_type_small_union_no_ts2799() {
    // Use T parameter name
    let source_t = r#"
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
    ? [K, ...Permutation<Exclude<T, K>>]
    : never;
type Perm = Permutation<"a" | "b">;
"#;
    let diags = get_diagnostics(source_t);
    assert!(
        !diags.iter().any(|d| d.0 == 2799),
        "Should NOT emit TS2799 for Permutation<\"a\" | \"b\"> (T param): {diags:?}"
    );

    // Use U parameter name — rule must not be hardcoded to 'T'
    let source_u = r#"
type Permutation<U, J = U> = [U] extends [never]
  ? []
  : J extends J
    ? [J, ...Permutation<Exclude<U, J>>]
    : never;
type Perm2 = Permutation<"x" | "y">;
"#;
    let diags2 = get_diagnostics(source_u);
    assert!(
        !diags2.iter().any(|d| d.0 == 2799),
        "Should NOT emit TS2799 for Permutation<\"x\" | \"y\"> (U param): {diags2:?}"
    );

    // 3-element union: only 6 permutations, must not trigger TS2799
    let source_3 = r#"
type Permutation<T, K = T> = [T] extends [never]
  ? []
  : K extends K
    ? [K, ...Permutation<Exclude<T, K>>]
    : never;
type Perm3 = Permutation<"A" | "B" | "C">;
"#;
    let diags3 = get_diagnostics(source_3);
    assert!(
        !diags3.iter().any(|d| d.0 == 2799),
        "Should NOT emit TS2799 for Permutation<\"A\" | \"B\" | \"C\">: {diags3:?}"
    );
}

/// Concrete instantiations of infinitely-recursive conditional aliases must emit TS2589.
/// Covers the sort-like multi-parameter pattern from issue #6614.
#[test]
fn infinite_recursive_conditional_alias_with_concrete_args_emits_ts2589_sort_pattern() {
    // Param names T / Sorted — exact repro from issue #6614
    let source_t = r#"
type Sort<T extends number[], Sorted extends boolean = true> =
  T extends [infer A extends number, infer B extends number, ...infer R extends number[]]
    ? A extends B
      ? Sort<[A, ...Sort<[B, ...R]>], Sorted>
      : `${A}` extends `${B}${string}`
        ? Sort<[B, A, ...R], false>
        : Sort<[A, ...Sort<[B, ...R]>], Sorted>
    : Sorted extends true ? T : Sort<T>;
type S = Sort<[3, 1, 2]>;
"#;
    let diags_t = get_diagnostics(source_t);
    assert!(
        diags_t.iter().any(|d| d.0 == 2589),
        "Must emit TS2589 for infinite recursive sort-like alias. Got: {diags_t:?}"
    );

    // Renamed params (U / Done) — rule must not be tied to the spelling 'T' or 'Sorted'
    let source_u = r#"
type Bubble<U extends number[], Done extends boolean = true> =
  U extends [infer X extends number, infer Y extends number, ...infer Rest extends number[]]
    ? X extends Y
      ? Bubble<[X, ...Bubble<[Y, ...Rest]>], Done>
      : `${X}` extends `${Y}${string}`
        ? Bubble<[Y, X, ...Rest], false>
        : Bubble<[X, ...Bubble<[Y, ...Rest]>], Done>
    : Done extends true ? U : Bubble<U>;
type B = Bubble<[5, 2, 8]>;
"#;
    let diags_u = get_diagnostics(source_u);
    assert!(
        diags_u.iter().any(|d| d.0 == 2589),
        "Must emit TS2589 for infinite recursive sort-like alias (renamed params). Got: {diags_u:?}"
    );
}

/// Concrete instantiations of simple infinite tail-recursive aliases must emit TS2589.
#[test]
fn infinite_tail_recursive_conditional_alias_with_concrete_args_emits_ts2589() {
    // Param name: T
    let source_t = r#"
type Cycle<T> = T extends any ? Cycle<T> : never;
type X = Cycle<number>;
"#;
    let diags_t = get_diagnostics(source_t);
    assert!(
        diags_t.iter().any(|d| d.0 == 2589),
        "Must emit TS2589 for infinite tail-recursive alias. Got: {diags_t:?}"
    );

    // Param name: U — proves the rule is not tied to the spelling 'T'
    let source_u = r#"
type Forever<U> = U extends string | number ? Forever<U> : never;
type Y = Forever<42>;
"#;
    let diags_u = get_diagnostics(source_u);
    assert!(
        diags_u.iter().any(|d| d.0 == 2589),
        "Must emit TS2589 for infinite tail-recursive alias (renamed param). Got: {diags_u:?}"
    );
}

/// Terminating concrete instantiations of recursive aliases must NOT emit TS2589.
/// These converge in a bounded number of steps regardless of input.
#[test]
fn terminating_recursive_alias_with_concrete_args_no_ts2589() {
    // Length counter: Len<[1,2,3]> converges in 4 steps
    let source_len = r#"
type Len<T extends any[]> = T extends [any, ...infer R] ? Len<R> : 0;
type L = Len<[1, 2, 3]>;
"#;
    let diags_len = get_diagnostics(source_len);
    assert!(
        !diags_len.iter().any(|d| d.0 == 2589),
        "Len<[1,2,3]> is bounded; must NOT emit TS2589. Got: {diags_len:?}"
    );

    // String trimming: TrimRight<"hello   "> terminates when no trailing space remains
    let source_trim = r#"
type TrimRight<S extends string> = S extends `${infer R} ` ? TrimRight<R> : S;
type T = TrimRight<"hello   ">;
"#;
    let diags_trim = get_diagnostics(source_trim);
    assert!(
        !diags_trim.iter().any(|d| d.0 == 2589),
        "TrimRight<\"hello   \"> is bounded; must NOT emit TS2589. Got: {diags_trim:?}"
    );
}
