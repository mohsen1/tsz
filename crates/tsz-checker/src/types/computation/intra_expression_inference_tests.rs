//! Regression tests for intra-expression inference.
//!
//! Anchors `tsc`-parity behaviour for object-literal arguments that mix a
//! non-sensitive contributor (Round 1) with a sensitive callback whose
//! contextual signature references the same type parameters indirectly
//! (mapped types, conditional types, `infer`).

use crate::test_utils::check_source_codes;

/// Regression target for
/// `conformance/types/typeRelationships/typeInference/intraExpressionInferences.ts`
/// at `test.ts(131,5)`.
///
/// When a generic call has an object-literal argument with both:
///   1. A non-sensitive `setup` method that returns the type-parameter shape
///      (Round 1 contributor: `setup(): { inputs: I; outputs: O }`), and
///   2. A sensitive `map` callback whose contextual signature references the
///      same type parameters through a homomorphic mapped type with `infer`
///      (`map: (val: Unwrap<I>) => Unwrap<O>`),
///
/// `tsc` infers `I` and `O` from `setup`'s concrete return value (Round 1)
/// and uses those bindings to instantiate `Unwrap<I>` / `Unwrap<O>` as the
/// contextual types for the `map` callback. The body's actual return is then
/// checked against `Unwrap<O>` and a TS2322 fires when they differ.
///
/// `tsz` currently fails to keep the Round 1 inferences for `I` / `O` when
/// the sensitive callback also references those parameters through a
/// mapped + conditional + `infer` type: both fall back to the constraint
/// (`Record<string, Wrapper>`), so `Unwrap<O>` widens to `Record<string,
/// any>` and the callback body becomes vacuously assignable. The
/// partial-Round-1 extraction in `extract_inference_contributing_object_type`
/// correctly skips the sensitive `map` property (`params_are_concrete=false`),
/// but the downstream Round 2 substitution refinement re-derives `O` from
/// the callback body and overwrites the Round 1 binding even when the new
/// value is incompatible with the existing concrete inference.
///
/// Until the Round 1 / Round 2 precedence and the homomorphic mapped-type
/// reverse inference are wired through `query_boundaries`, this test
/// documents the desired behaviour. Marked `#[ignore]` so the suite stays
/// green; remove the attribute once the precedence fix lands.
#[test]
#[ignore = "intra-expression precedence: sensitive callback overrides Round 1 (intraExpressionInferences:131,5)"]
fn intra_expression_inference_homomorphic_mapped_return_type() {
    // Lib-independent reproduction: replace `Record<string, Wrapper>` with an
    // equivalent index-signature constraint so the test environment (which
    // runs without `lib.es5.d.ts`) reproduces the same inference path.
    let source = r#"
class Wrapper<T = any> { public value?: T; }
type WrappedMap = { [k: string]: Wrapper };
type Unwrap<D extends WrappedMap> = {
    [K in keyof D]: D[K] extends Wrapper<infer T> ? T : never;
};
type MappingComponent<I extends WrappedMap, O extends WrappedMap> = {
    setup(): { inputs: I; outputs: O };
    map?: (inputs: Unwrap<I>) => Unwrap<O>;
};
declare function createMappingComponent<I extends WrappedMap, O extends WrappedMap>(
    def: MappingComponent<I, O>,
): void;
createMappingComponent({
    setup() {
        return {
            inputs: { num: new Wrapper<number>() },
            outputs: { str: new Wrapper<string>() },
        };
    },
    map(inputs) {
        return {
            str: 42,
        };
    },
});
"#;
    let errors = check_source_codes(source);
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    assert!(
        semantic_errors.contains(&2322),
        "Round 1 should infer `O = {{ str: Wrapper<string> }}` from setup so `Unwrap<O>` resolves \
         to `{{ str: string }}` and the map body's `{{ str: 42 }}` must produce TS2322; \
         got: {semantic_errors:?}"
    );
}
