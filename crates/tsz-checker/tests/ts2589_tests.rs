//! Tests for TS2589: Type instantiation is excessively deep and possibly infinite.

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::{check_source_code_messages, check_source_diagnostics};
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
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
