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

/// #17028 item 2: a tuple-length-counter recursion tied through a non-tail
/// (object-property) position genuinely grows its argument by one element on
/// every step — the coarse structural-weight metric alone reads that as
/// unbounded divergence — yet still terminates the moment the literal-number
/// condition trips. `tsc` reaches this via real, concrete
/// `instantiationDepth`-bounded evaluation and never reports `TS2589` here,
/// confirmed against pinned `typescript@7.0.2` at target depths 1, 60, and
/// (unboundedly accepted) 1000. `residual_application_diverges` must re-drive
/// the residual through bounded real expansion rather than declaring
/// divergence from the first step's growth.
#[test]
fn nest_accumulator_grows_but_terminates_no_ts2589() {
    let source = r#"
type Nest<N extends unknown[]> = N["length"] extends 1 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "a non-tail accumulator recursion terminating after one real step must not produce TS2589. Got: {codes:?}"
    );
}

/// Same shape as `nest_accumulator_grows_but_terminates_no_ts2589` at the
/// exact depth from the reported issue (#17028) — the depth at which the old
/// single-step weight comparison first went wrong, oracle-confirmed clean on
/// `typescript@7.0.2`.
#[test]
fn nest_accumulator_grows_but_terminates_no_ts2589_at_reported_depth() {
    let source = r#"
type Nest<N extends unknown[]> = N["length"] extends 60 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "a non-tail accumulator recursion terminating after 60 real steps must not produce TS2589. Got: {codes:?}"
    );
}

/// Renamed binders confirm the fix is structural, not keyed to any particular
/// alias / type-parameter / property identifier.
#[test]
fn nest_accumulator_grows_but_terminates_no_ts2589_renamed() {
    let source = r#"
type Countdown<Acc extends unknown[]> = Acc["length"] extends 12 ? boolean : { held: Countdown<[unknown, ...Acc]> };
type Result = Countdown<[]>;
declare const r: Result;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "renamed non-tail accumulator recursion must not produce TS2589. Got: {codes:?}"
    );
}

/// Sharper witness than the reported depth-60 case (independently found during
/// PR review, oracle-confirmed clean on `typescript@7.0.2`): the old
/// single-step weight comparison mis-fired on this non-tail nest at a mere
/// twenty levels, nowhere near any depth `tsc` itself would treat as
/// excessive.
#[test]
fn nest_accumulator_grows_but_terminates_no_ts2589_at_depth_twenty() {
    let source = r#"
type Nest<N extends unknown[]> = N["length"] extends 20 ? number : { a: Nest<[unknown, ...N]> };
type Z = Nest<[]>;
declare const z: Z;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2589),
        "a non-tail accumulator recursion terminating after 20 real steps must not produce TS2589. Got: {codes:?}"
    );
}

/// Load-bearing negative control (independently found during PR review): a
/// *tail*-recursive, genuinely-terminating counter accumulator stays clean at
/// 200 and 300 real steps (absorbed by `tsc`'s own tail-recursion
/// elimination, `MAX_TAIL_RECURSION_DEPTH` parity) but both `tsc` and tsz
/// still correctly report `TS2589` once it needs 2000 — beyond that budget.
/// This is the case that would catch a regression where this fix's residual
/// termination bound was implemented by simply raising or deleting the
/// divergence threshold instead of re-driving real bounded expansion: such a
/// change would wrongly clear this row too, and `TS2589` would go dead for
/// every genuinely divergent tail recursion. Not easy to reconstruct (a
/// non-tail nest never trips this path at all; 200/300 are both silently
/// absorbed), so it is pinned here rather than left as a one-off manual check.
#[test]
fn tail_recursive_counter_beyond_tail_recursion_budget_still_diverges_ts2589() {
    let bounded_at = |n: u32| {
        format!(
            r#"
type TailNest<N extends number, A extends any[] = []> = A["length"] extends N ? number : TailNest<N, [...A, unknown]>;
type Z = TailNest<{n}>;
declare const z: Z;
"#
        )
    };

    let codes_200 = check_source_codes(&bounded_at(200));
    assert!(
        !codes_200.contains(&2589),
        "a tail-recursive counter terminating at 200 steps must not produce TS2589. Got: {codes_200:?}"
    );

    let codes_300 = check_source_codes(&bounded_at(300));
    assert!(
        !codes_300.contains(&2589),
        "a tail-recursive counter terminating at 300 steps must not produce TS2589. Got: {codes_300:?}"
    );

    let codes_2000 = check_source_codes(&bounded_at(2000));
    assert!(
        codes_2000.contains(&2589),
        "a tail-recursive counter needing 2000 real steps must still produce TS2589 (tsc's own tail-recursion budget). Got: {codes_2000:?}"
    );
}
