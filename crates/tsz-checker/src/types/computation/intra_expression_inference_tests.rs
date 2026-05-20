//! Regression tests for intra-expression inference.
//!
//! Anchors `tsc`-parity behaviour for object-literal arguments that mix a
//! non-sensitive contributor (Round 1) with a sensitive callback whose
//! contextual signature references the same type parameters indirectly
//! (mapped types, conditional types, `infer`).

use crate::test_utils::{check_source_code_messages, check_source_codes};

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
/// The checker's Round 1 / Round 2 two-pass logic correctly infers
/// `I = { num: Wrapper<number> }` and `O = { str: Wrapper<string> }` from
/// `setup`'s concrete return value, but the solver's single-pass
/// `resolve_call` (run after Round 2 with the refined arg types) cannot
/// recover the binding for `O`: reverse inference through
/// `Unwrap<O>` (a homomorphic mapped + conditional + `infer` type)
/// from the callback body's return position fails, and `O` falls back to
/// its constraint. The recheck against `MappingComponent<I, WrappedMap>`
/// then accepts `{ map(): { str: 42 } }` vacuously.
///
/// Fix: after the solver returns its `instantiated_params`, the checker
/// overlays its Round 1 substitution (the bindings derived from
/// non-sensitive contributors like `setup`) where the solver effectively
/// defaulted to the type parameter's constraint and the checker's binding
/// is strictly more specific (a fresh subtype). Gated on at least one
/// argument having a Round 1 partial extraction so calls without a
/// non-sensitive contributor (e.g., `p.then(() => x, () => 1)`) leave the
/// solver's inference untouched. See
/// `refine_instantiated_params_with_checker_substitution`.
#[test]
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

/// Same structural rule as `intra_expression_inference_homomorphic_mapped_return_type`
/// but with different identifier names for the type parameters and the
/// mapped type's iteration variable. The fix must be expressible as a
/// structural rule over types/symbols, not a name-match — per the
/// anti-hardcoding directive in `.claude/CLAUDE.md` §25, every
/// substitution-refinement test ships with a sibling that proves the rule
/// survives renaming. If the previous test passes but this one fails, the
/// fix is hardcoded against `I` / `O` / `K` / `T`.
#[test]
fn intra_expression_inference_homomorphic_mapped_return_type_renamed() {
    let source = r#"
class Box<X = any> { public value?: X; }
type BoxMap = { [k: string]: Box };
type Open<R extends BoxMap> = {
    [P in keyof R]: R[P] extends Box<infer V> ? V : never;
};
type Comp<In extends BoxMap, Out extends BoxMap> = {
    init(): { src: In; dst: Out };
    proc?: (src: Open<In>) => Open<Out>;
};
declare function buildComp<In extends BoxMap, Out extends BoxMap>(
    spec: Comp<In, Out>,
): void;
buildComp({
    init() {
        return {
            src: { n: new Box<number>() },
            dst: { s: new Box<string>() },
        };
    },
    proc(src) {
        return {
            s: 42,
        };
    },
});
"#;
    let errors = check_source_codes(source);
    let semantic_errors: Vec<_> = errors.into_iter().filter(|&c| c != 2318).collect();
    assert!(
        semantic_errors.contains(&2322),
        "Renamed structural reproduction must still emit TS2322; \
         if this regresses while the original test passes, the fix is \
         hardcoded against the original identifier names. got: {semantic_errors:?}"
    );
}

#[test]
fn intra_expression_inference_homomorphic_mapped_return_diagnostic_surface() {
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
            inputs: {
                num: new Wrapper<number>(),
                str: new Wrapper<string>(),
            },
            outputs: {
                bool: new Wrapper<boolean>(),
                str: new Wrapper<string>(),
            },
        };
    },
    map(inputs) {
        return {
            bool: inputs.nonexistent,
            str: inputs.num,
        };
    },
});
"#;
    let messages = check_source_code_messages(source);
    let ts2322 = messages
        .iter()
        .find_map(|(code, message)| {
            (*code == 2322 && message.contains("=> Unwrap<")).then_some(message)
        })
        .unwrap_or_else(|| panic!("expected TS2322 diagnostic, got: {messages:#?}"));

    assert!(
        ts2322.contains("bool: any; str: number"),
        "function return source display should render the error property as any; got: {ts2322}"
    );
    assert!(
        !ts2322.contains("bool: error"),
        "function return source display should not expose internal error type; got: {ts2322}"
    );
    assert!(
        ts2322.contains(
            "(inputs: Unwrap<{ num: Wrapper<number>; str: Wrapper<string>; }>) => Unwrap<{ bool: Wrapper<boolean>; str: Wrapper<string>; }>"
        ),
        "present optional callable target should display as the callable type; got: {ts2322}"
    );
    assert!(
        !ts2322.contains("| undefined"),
        "present optional callable target should not display synthetic undefined; got: {ts2322}"
    );
}

#[test]
fn intra_expression_inference_homomorphic_mapped_return_diagnostic_surface_renamed() {
    let source = r#"
class Box<X = any> { public value?: X; }
type BoxMap = { [k: string]: Box };
type Open<R extends BoxMap> = {
    [P in keyof R]: R[P] extends Box<infer V> ? V : never;
};
type Comp<In extends BoxMap, Out extends BoxMap> = {
    init(): { src: In; dst: Out };
    proc?: (src: Open<In>) => Open<Out>;
};
declare function buildComp<In extends BoxMap, Out extends BoxMap>(
    spec: Comp<In, Out>,
): void;
buildComp({
    init() {
        return {
            src: {
                n: new Box<number>(),
                label: new Box<string>(),
            },
            dst: {
                ok: new Box<boolean>(),
                label: new Box<string>(),
            },
        };
    },
    proc(src) {
        return {
            ok: src.missing,
            label: src.n,
        };
    },
});
"#;
    let messages = check_source_code_messages(source);
    let ts2322 = messages
        .iter()
        .find_map(|(code, message)| {
            (*code == 2322 && message.contains("=> Open<")).then_some(message)
        })
        .unwrap_or_else(|| panic!("expected TS2322 diagnostic, got: {messages:#?}"));

    assert!(
        ts2322.contains("ok: any; label: number"),
        "renamed source display should render the error property as any; got: {ts2322}"
    );
    assert!(
        !ts2322.contains("ok: error"),
        "renamed source display should not expose internal error type; got: {ts2322}"
    );
    assert!(
        ts2322.contains(
            "(src: Open<{ n: Box<number>; label: Box<string>; }>) => Open<{ ok: Box<boolean>; label: Box<string>; }>"
        ),
        "renamed optional callable target should display as the callable type; got: {ts2322}"
    );
    assert!(
        !ts2322.contains("| undefined"),
        "renamed optional callable target should not display synthetic undefined; got: {ts2322}"
    );
}

/// Regression test for issue #5928.
///
/// When `T extends unknown[]` and both a function arg and an array literal arg
/// contribute to inferring T, the contra-candidate `[string, number]` (from the
/// function's parameter list) must win over the covariant array candidate when
/// the covariant candidate is not assignable to the contra-candidate.
#[test]
fn issue_5928_generic_rest_param_infers_tuple_not_array() {
    let codes = check_source_codes(
        r#"
function apply<T extends unknown[]>(fn: (...args: T) => void, args: T): void {
    fn(...args);
}
function log(a: string, b: number): void {}
apply(log, ["hello", 42]);
"#,
    );
    assert!(
        codes.is_empty(),
        "Expected no errors: T should infer as [string, number], not (string|number)[]; got codes: {codes:?}"
    );
}

/// Higher-order function (curry) pattern: rest-param tuple inference must propagate.
#[test]
fn issue_5928_curry_pattern_no_error() {
    let codes = check_source_codes(
        r#"
function curry<A extends unknown[], B>(fn: (...args: A) => B): (...args: A) => B {
    return fn;
}
declare function add(a: number, b: number): number;
const curriedAdd = curry(add);
"#,
    );
    assert!(
        codes.is_empty(),
        "Expected no errors for curry(add); got codes: {codes:?}"
    );
}
