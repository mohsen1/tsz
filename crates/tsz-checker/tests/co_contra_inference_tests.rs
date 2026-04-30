use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

#[test]
#[ignore = "merged backlog: needs contravariant inference through callable alias unions"]
fn contravariant_callable_alias_union_does_not_produce_ts2345() {
    let source = r#"
type Func1<T> = (x: T) => void;
type Func2<T> = ((x: T) => void) | undefined;

declare let f1: Func1<string>;
declare let f2: Func1<"a">;

declare function foo<T>(f1: Func1<T>, f2: Func1<T>): void;

foo(f1, f2);

declare let g1: Func2<string>;
declare let g2: Func2<"a">;

declare function bar<T>(g1: Func2<T>, g2: Func2<T>): void;

bar(f1, f2);
bar(g1, g2);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.iter().all(|(code, _)| *code != 2345),
        "Callable alias unions should keep contravariant inference and avoid TS2345. Actual diagnostics: {relevant:#?}"
    );
}

/// When the only covariant candidate is `never` (from an empty array) and
/// contra-candidates exist (from callback parameters), the solver should
/// use the contra-candidates. This matches tsc's getInferredType logic:
///   `inferredCovariantType && !(inferredCovariantType.flags & TypeFlags.Never)`
/// Repro from TypeScript#19576 (neverInference.ts).
#[test]
fn never_covariant_falls_through_to_contra_candidates() {
    let source = r#"
type Comparator<T> = (x: T, y: T) => number;

interface LinkedList<T> {
    comparator: Comparator<T>,
    nodes: { value: T, next: any } | null
}

declare function compareNumbers(x: number, y: number): number;
declare function mkList<T>(items: T[], comparator: Comparator<T>): LinkedList<T>;

const list: LinkedList<number> = mkList([], compareNumbers);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Empty array with callback should infer T=number from contra-candidates, not T=never/unknown. Got: {diagnostics:#?}"
    );
}

/// Empty array `[]` passed as argument for a generic `T[]` parameter should
/// be typed as `never[]`, not `unknown[]`. The contextual type parameter
/// should not pollute the element type of the empty array.
#[test]
fn empty_array_in_generic_context_is_never_not_unknown() {
    let source = r#"
declare function mkList<T>(items: T[], other: T): T;
const result: number = mkList([], 42);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Empty array should be never[] (not unknown[]) so T is inferred from second arg. Got: {diagnostics:#?}"
    );
}

/// Multiple inference sources with an empty array: T should be inferred from
/// the non-empty source (f2 repro from TypeScript#19858).
#[test]
fn empty_array_with_multiple_inference_sources() {
    let source = r#"
declare function f2<a>(as1: a[], as2: a[], cmp: (a1: a, a2: a) => number): void;
f2([0], [], (a1, a2) => a1 - a2);
f2([], [0], (a1, a2) => a1 - a2);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Empty array alongside non-empty array should still infer 'a' correctly. Got: {diagnostics:#?}"
    );
}

/// Repeated naked type-parameter parameters use first-wins semantics for
/// incompatible direct argument candidates (e.g. `f<T>(a: T, b: T)` called
/// with `(1, "")` keeps `T = number` and rejects `""`). The first-wins skip
/// must NOT fire when the later argument's type is a union containing
/// `null`/`undefined` — tsc still seeds inference from the non-nullable
/// members and adds the nullable back via `getNullableType` after BCT
/// reduction. Without this nullable-union exception the second argument's
/// candidate is dropped entirely, `T` resolves to the first argument's type
/// alone, and the second argument is rechecked against that narrowed `T`,
/// surfacing as `Argument of type 'never' is not assignable to parameter of
/// type '"a"'.` Conformance test
/// `compiler/inferenceOfNullableObjectTypesWithCommonBase.ts` exercises
/// this on lines 29 (`equal(v as 'a', v as 'b' | undefined)`) and 34
/// (`equal(v as string, v as string & { tag: 'foo' } | undefined)`).
#[test]
fn nullable_union_second_arg_does_not_skip_inference() {
    let source = r#"
function equal<T>(a: T, b: T) { }
let v = null!;
equal(v as 'a', v as 'b' | undefined);
equal(v as string, v as string & { tag: 'foo' } | undefined);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "equal<T>(a: T, b: T) with a literal first arg and a nullable-union second arg \
         should still infer T from both args. Got: {diagnostics:#?}"
    );
}

/// When T has no constraint and only covariant candidates are `never`,
/// and there are no contra-candidates, T should resolve to `never` (not unknown).
#[test]
fn only_never_candidates_resolves_to_never() {
    let source = r#"
declare function f1<T>(x: T[]): T;
let a1 = f1([]);
let check: never = a1;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "f1([]) should infer T=never when there are no contra-candidates. Got: {diagnostics:#?}"
    );
}

/// Regression for `subtypeRelationForNever.ts` (TS issue #51999).
///
/// When a return-position type variable has both a `never` candidate (from a
/// function returning `never`) and a non-`never` covariant candidate, the
/// pre-Round-2 fix layer correctly picks the non-`never` value (BCT filters
/// `never`), but the final `resolve_return_position_inference_type` was
/// then promoting the lone surviving concrete bound — which IS `never` —
/// back into the result whenever the BCT result was `unknown`/`any`/
/// placeholder-bearing. The promotion contradicts BCT and forces the
/// later argument check (e.g. `id` against `(values: a[]) => never`) to
/// reject a perfectly valid `<a>(value: a) => a` lambda.
///
/// The fix excludes `never` from the concrete-bounds promotion list so
/// the BCT-chosen result stands.
#[test]
fn never_return_candidate_does_not_force_never_inference() {
    let source = r#"
function fail(message: string): never { throw new Error(message); }
function withFew<a, r>(values: a[], haveFew: (values: a[]) => r, haveNone: (reason: string) => r): r {
    return values.length > 0 ? haveFew(values) : haveNone('No values.');
}
function id<a>(value: a): a { return value; }
const result = withFew([1, 2, 3], id, fail);
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let blocking: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        blocking.is_empty(),
        "withFew([1,2,3], id, fail) should infer r from id (not collapse to never via fail). Got: {diagnostics:#?}"
    );
}
