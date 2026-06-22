//! Regression tests for #14518: a composition of recursive utility aliases
//! (a homomorphic mapped over a variadic-rebuilt tuple) must preserve
//! per-index tuple identity — the transformed type stays a fixed tuple, so a
//! later literal index resolves to the concrete element rather than collapsing
//! to the element-type union.
//!
//! Structural rule: `tsc` keeps a homomorphic mapped type
//! `{ [K in keyof T]: F<T[K]> }` (and a variadic rebuild
//! `readonly [G<A>, ...G<B>]`) a *tuple* when its source `T` is a tuple whose
//! rest element is still a deferred recursive-alias application
//! (`...DeepRO<B>`) — the `NoopResolver` cannot expand it here, so mapping per
//! slot now would collapse the hidden slots into the rest's element union and
//! permanently drop tuple-ness. The instantiator defers the mapped to the
//! resolver-backed outer evaluator, which expands the rest to a concrete tuple
//! first. A bare type-parameter rest (`...Elements`) is genuinely opaque and
//! stays on the eager per-slot path (so `Foo<[...Elements, "abc"]>`-style
//! variadic-growth recursion still terminates with TS2589, unchanged).

use tsz_checker::diagnostics::Diagnostic;

fn check_source(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

fn assert_clean(source: &str, label: &str) {
    let diagnostics = check_source(source);
    assert!(
        diagnostics.is_empty(),
        "[{label}] expected no diagnostics, got:\n{diagnostics:#?}"
    );
}

const PRELUDE: &str = r#"
type AliasCompute<TValue> = TValue extends (...args: infer P) => infer R ? (...args: P) => R
  : TValue extends readonly [infer H, ...infer T] ? readonly [AliasCompute<H>, ...AliasCompute<T>]
  : TValue extends object ? { [K in keyof TValue]: AliasCompute<TValue[K]> } : TValue;
type NormalizeBox<I> = I extends object ? { [F in keyof I]: NormalizeBox<I[F]> } : I;
type DeepRO<S> = S extends (...a: any[]) => any ? S
  : S extends readonly [infer A, ...infer B] ? readonly [DeepRO<A>, ...DeepRO<B>]
  : S extends object ? { readonly [N in keyof S]: DeepRO<S[N]> } : S;
type Map1<I> = { [F in keyof I]: I[F] };
type Tup = readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }];
type IsT3<T> = T extends readonly [any, any, any] ? "t3" : T extends readonly any[] ? "array" : "other";
"#;

/// A homomorphic map over a variadic-rebuilt tuple (`DeepRO<Tup>`) stays a
/// fixed 3-tuple — `tsc` reports `"t3"`; tsz-main regressed to `"array"`.
#[test]
fn issue_14518_homomorphic_map_over_variadic_rebuild_keeps_tuple() {
    assert_clean(
        &format!("{PRELUDE}\nexport const a: IsT3<NormalizeBox<DeepRO<Tup>>> = \"t3\";"),
        "NormalizeBox<DeepRO<Tup>>",
    );
    assert_clean(
        &format!("{PRELUDE}\nexport const a: IsT3<Map1<DeepRO<Tup>>> = \"t3\";"),
        "identity Map1<DeepRO<Tup>>",
    );
}

/// Re-applying a variadic-rebuild utility to an already-rebuilt tuple keeps
/// tuple-ness (`DeepRO<DeepRO<Tup>>` is a 3-tuple, not an array).
#[test]
fn issue_14518_nested_variadic_rebuild_keeps_tuple() {
    assert_clean(
        &format!("{PRELUDE}\nexport const a: IsT3<DeepRO<DeepRO<Tup>>> = \"t3\";"),
        "DeepRO<DeepRO<Tup>>",
    );
}

/// The full multi-utility composition is still a fixed 3-tuple.
#[test]
fn issue_14518_full_composition_keeps_tuple() {
    assert_clean(
        &format!(
            "{PRELUDE}\nexport const a: IsT3<AliasCompute<NormalizeBox<DeepRO<Tup>>>> = \"t3\";"
        ),
        "AliasCompute<NormalizeBox<DeepRO<Tup>>>",
    );
    assert_clean(
        &format!("{PRELUDE}\nexport const a: IsT3<AliasCompute<DeepRO<Tup>>> = \"t3\";"),
        "AliasCompute<DeepRO<Tup>>",
    );
}

/// Control: a variadic-growth tuple whose rest is a bare *type parameter*
/// (`...Elements`) must stay on the eager per-slot path so the recursive
/// `Foo<[...Elements, "abc"]>` growth still terminates (TS2589), exactly like
/// `tsc` — the deferral gate must not fire here.
#[test]
fn issue_14518_type_parameter_rest_growth_still_terminates() {
    let source = r#"
class Foo<Elements extends readonly unknown[]> {
  public readonly elements: { [P in keyof Elements]: { bar: Elements[P] } };
  public constructor(...elements: { [P in keyof Elements]: { bar: Elements[P] } }) {
    this.elements = elements;
  }
  public add(): Foo<[...Elements, "abc"]> {
    return new Foo<[...Elements, "abc"]>(...this.elements, { bar: "abc" });
  }
}
"#;
    let diagnostics = check_source(source);
    // tsc reports TS2589 here ("excessively deep"); the important property is
    // that tsz terminates with the same diagnostic and does not hang or crash.
    assert!(
        diagnostics.iter().any(|d| d.code == 2589),
        "expected TS2589 (excessively deep), got:\n{diagnostics:#?}"
    );
}

/// The original bench witness: a numeric-literal index into the transformed
/// tuple followed by a property access. The per-index *type* identity is
/// restored by this change, but the final value-index hop still reads an
/// order-dependent (cache-poisoned) form under the checker's limited resolver
/// — the resolver-context-independence root tracked by #13980 / #14330. This
/// case is the campaign's terminal step and is ignored until that lands.
#[ignore = "needs the #13980/#14330 resolver-context-independence campaign for the final value-index hop"]
#[test]
fn issue_14518_value_index_witness() {
    let source = r#"
type AliasCompute<TValue> = TValue extends (...args: infer P) => infer R ? (...args: P) => R
  : TValue extends readonly [infer H, ...infer T] ? readonly [AliasCompute<H>, ...AliasCompute<T>]
  : TValue extends object ? { [K in keyof TValue]: AliasCompute<TValue[K]> } : TValue;
type NormalizeBox<I> = I extends object ? { [F in keyof I]: NormalizeBox<I[F]> } : I;
type DeepRO<S> = S extends (...a: any[]) => any ? S
  : S extends readonly [infer A, ...infer B] ? readonly [DeepRO<A>, ...DeepRO<B>]
  : S extends object ? { readonly [N in keyof S]: DeepRO<S[N]> } : S;
type Tup = readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }];
declare const b: AliasCompute<NormalizeBox<DeepRO<Tup>>>;
export const q: 3 = b[2].wrapped;
"#;
    let diagnostics = check_source(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2339),
        "expected no TS2339, got:\n{diagnostics:#?}"
    );
}
