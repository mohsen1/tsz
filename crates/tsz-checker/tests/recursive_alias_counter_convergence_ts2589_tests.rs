//! Regression tests: a recursive generic type alias that terminates via a
//! second (counter) type parameter, or that ties a finite knot through a
//! homomorphic mapped type over a recursive object, must not raise a spurious
//! TS2589 ("Type instantiation is excessively deep and possibly infinite").
//!
//! Structural rule: "When a use site instantiates a recursive alias and the
//! evaluator leaves a residual self-application in a deferred position, that
//! residual is divergence evidence only if its arguments grow strictly along an
//! unbounded dimension (string length, tuple/template arity). A residual that
//! stays the same size or shrinks is making progress — via a numeric depth
//! counter (`N` -> `Exclude<N, 0>`) or a structural descent into `T[K]` — or is
//! tying a finite knot the way tsc defers recursive object/mapped references, and
//! must not raise TS2589."
//!
//! Owner layer: solver convergence metric (`self_application_arg_weight`) +
//! checker use-site probe (`evaluate_type_for_ts2589_check`). The guard against
//! over-correction (genuine string/tuple builders must still raise TS2589) is
//! covered by the `*_still_diverges_*` cases below.

use tsz_checker::test_utils::check_source_codes;

/// A mapped-over-object recursion bounded by a numeric counter (`N`) terminates
/// at the `N extends 0` base case. The counter and the structural descent into
/// `T[K]` are the termination measure; neither is visible to the coarse growth
/// metric, so the residual self-application must not be mistaken for divergence.
#[test]
fn numeric_counter_recursion_no_ts2589() {
    let source = r#"
type Prev = [never, 0, 1, 2, 3, 4, 5, 6];
type DeepObject<T, N extends number> =
    N extends 0 ? T : { [K in keyof T]: DeepObject<T[K], Prev[N]> };
type Anchor = { value: string; nested?: Anchor };
type Fixed = DeepObject<Anchor, 6>;
declare const probe: Fixed;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "counter-bounded mapped recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// Same shape with renamed binders confirms the fix is structural, not keyed to
/// any particular alias / type-parameter / property identifier.
#[test]
fn numeric_counter_recursion_no_ts2589_renamed() {
    let source = r#"
type StepDown = [never, 0, 1, 2, 3, 4, 5, 6];
type WalkTree<Node, Budget extends number> =
    Budget extends 0 ? Node : { [Field in keyof Node]: WalkTree<Node[Field], StepDown[Budget]> };
type Branch = { label: string; child?: Branch };
type Walked = WalkTree<Branch, 6>;
declare const w: Walked;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "renamed counter-bounded recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// The literal issue-family shape (#10818/#10826/#10834/#10867/#10875): a
/// `BuildTuple` length guard plus an `Exclude<N, 0>` counter (a no-op for a
/// single literal) over a self-referential optional property. tsc accepts this
/// by tying a finite knot for the recursive object/mapped reference.
#[test]
fn build_tuple_guarded_recursion_no_ts2589() {
    let source = r#"
type BuildTuple<N extends number, A extends any[] = []> =
    A['length'] extends N ? A : BuildTuple<N, [...A, unknown]>;
type DeepObject<T, N extends number> =
    BuildTuple<N> extends []
        ? T
        : { [K in keyof T]: DeepObject<T[K], Exclude<N, 0>> };
type Anchor = { value: string; nested?: Anchor };
type Fixed = DeepObject<Anchor, 6>;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "BuildTuple-guarded object recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// A real-world `DeepPartial` with an explicit recursion budget must resolve to
/// a usable optional-chained type without a spurious TS2589.
#[test]
fn deep_partial_with_budget_no_ts2589() {
    let source = r#"
type Prev = [never, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
type DeepPartial<T, N extends number = 6> =
    N extends 0 ? T : { [K in keyof T]?: DeepPartial<T[K], Prev[N]> };
type Config = { a: { b: { c: { d: string } } } };
type R = DeepPartial<Config>;
declare const cfg: R;
const leaf = cfg.a?.b?.c?.d;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "budgeted DeepPartial must not produce TS2589. Got: {codes:?}"
    );
}

/// Guard against over-correction: a recursion that genuinely grows a
/// template-literal string without bound must still raise TS2589.
#[test]
fn template_string_builder_still_diverges_ts2589() {
    let source = r#"
type Grow<S extends string> = S extends `${infer _}` ? Grow<`${S}x`> : never;
type X = Grow<"a">;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2589),
        "unbounded template-string growth must still produce TS2589. Got: {codes:?}"
    );
}

/// Guard against over-correction: a recursion that genuinely grows a tuple
/// without bound must still raise TS2589.
#[test]
fn tuple_builder_still_diverges_ts2589() {
    let source = r#"
type Grow<A extends any[]> = A['length'] extends 0 ? never : Grow<[...A, 1]>;
type X = Grow<[1]>;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2589),
        "unbounded tuple growth must still produce TS2589. Got: {codes:?}"
    );
}

/// #17028 item 2: a tuple-length-bounded recursion whose self-reference sits
/// in a NON-TAIL position (an object property, not the alias body itself) is
/// the shape the single-round residual-growth check flagged after exactly one
/// evaluation round: the evaluator only expands one property lookup per round
/// (`{ a: Nest<[...N, unknown]> }` grows the tuple by one element per round),
/// so a single round of growth looks identical to genuine divergence unless
/// the probe keeps following it toward the base case. tsc accepts this up to
/// its real `instantiationDepth`; a length target trivially inside that bound
/// must not raise TS2589.
#[test]
fn numeric_length_bounded_nest_no_ts2589() {
    let source = r#"
type Nest<N extends unknown[]> =
    N["length"] extends 8 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "length-bounded non-tail object recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// Same shape at the exact depth-1 boundary from #17028's own repro: a single
/// round of tuple growth reaching the base case on the very next round. This
/// is the shape that used to fire TS2589 unconditionally regardless of the
/// target depth, since the old check compared only the first round's residual
/// against the original input.
#[test]
fn numeric_length_bounded_nest_no_ts2589_at_depth_one() {
    let source = r#"
type Nest<N extends unknown[]> =
    N["length"] extends 1 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "depth-1 non-tail object recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// Renamed-binder variant confirms the fix is structural, not keyed to any
/// particular alias / type-parameter / property identifier.
#[test]
fn numeric_length_bounded_nest_no_ts2589_renamed() {
    let source = r#"
type Recurse<Args extends unknown[]> =
    Args["length"] extends 8 ? string : { wrapped: Recurse<[unknown, ...Args]> };
type Done = Recurse<[]>;
declare const done: Done;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "renamed length-bounded non-tail object recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// Guard against over-correction: the same non-tail object-property shape
/// with no reachable base case (the length target is far past the real
/// instantiation-depth bound) must still raise TS2589 — sustained growth all
/// the way to the bound is still divergence evidence, not just the first
/// round of growth.
#[test]
fn numeric_length_unbounded_nest_still_diverges_ts2589() {
    let source = r#"
type Nest<N extends unknown[]> =
    N["length"] extends 999999 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2589),
        "non-tail object recursion with no reachable base case must still produce TS2589. Got: {codes:?}"
    );
}
