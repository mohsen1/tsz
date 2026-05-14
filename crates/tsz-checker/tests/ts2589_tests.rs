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
