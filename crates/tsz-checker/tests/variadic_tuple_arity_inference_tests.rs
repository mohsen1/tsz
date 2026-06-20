//! Checker integration tests for variadic tuple arity inference.
//!
//! Structural rule: when a generic function parameter is a variadic tuple
//! `[H, ...Tail]`, `[...Init, L]`, or `[H, ...Mid, L]`, tsc aligns fixed
//! elements from the front (prefix) and back (suffix) of the concrete argument
//! tuple, then infers the rest type parameter from the middle slice.
//!
//! These tests verify that tsz infers the correct types and therefore accepts
//! valid assignments and rejects only the deliberately wrong ones.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_only_one_2322(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "{label}: expected exactly one TS2322, got {codes:?}"
    );
}

// =============================================================================
// Trailing-rest: [H, ...Tail]
// =============================================================================

#[test]
fn head_and_tail_function_infers_correctly() {
    assert_no_errors(
        r#"
declare function head<H, Tail extends unknown[]>(args: [H, ...Tail]): H;
const h: string = head(["hello", 1, true]);
"#,
        "head function: H inferred as string",
    );
}

#[test]
fn tail_function_infers_rest_as_tuple() {
    assert_no_errors(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [number, boolean] = tail(["hello", 1, true]);
"#,
        "tail function: Tail inferred as [number, boolean]",
    );
}

#[test]
fn tail_function_renamed_type_params() {
    // Proves fix is not keyed on param name "Tail"
    assert_no_errors(
        r#"
declare function tail2<X, Y extends unknown[]>(args: [X, ...Y]): Y;
const t: [number, boolean] = tail2(["hello", 1, true]);
"#,
        "tail function with renamed params: Y = [number, boolean]",
    );
}

#[test]
fn tail_wrong_assignment_fails() {
    assert_only_one_2322(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [string, boolean] = tail(["hello", 1, true]);
"#,
        "tail function: bad assignment must produce TS2322",
    );
}

#[test]
fn empty_tail_inferred_as_empty_tuple() {
    assert_no_errors(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [] = tail(["only"]);
"#,
        "tail of single-element source is []",
    );
}

// =============================================================================
// Leading-rest: [...Init, L]
// =============================================================================

#[test]
fn last_function_infers_correctly() {
    assert_no_errors(
        r#"
declare function last<Init extends unknown[], L>(args: [...Init, L]): L;
const l: boolean = last(["hello", 1, true]);
"#,
        "last function: L inferred as boolean",
    );
}

#[test]
fn init_function_infers_rest_as_tuple() {
    assert_no_errors(
        r#"
declare function init<Init extends unknown[], L>(args: [...Init, L]): Init;
const i: [string, number] = init(["hello", 1, true]);
"#,
        "init function: Init = [string, number]",
    );
}

#[test]
fn init_function_renamed_type_params() {
    // Proves fix is not keyed on param name "Init"
    assert_no_errors(
        r#"
declare function init2<P extends unknown[], Q>(args: [...P, Q]): P;
const i: [string, number] = init2(["hello", 1, true]);
"#,
        "init function renamed: P = [string, number]",
    );
}

#[test]
fn last_wrong_assignment_fails() {
    assert_only_one_2322(
        r#"
declare function last<Init extends unknown[], L>(args: [...Init, L]): L;
const l: string = last(["hello", 1, true]);
"#,
        "last function: bad assignment must produce TS2322",
    );
}

// =============================================================================
// Fixed-prefix + rest + fixed-suffix: [H, ...Mid, L]
// =============================================================================

#[test]
fn sandwich_function_infers_prefix_rest_suffix() {
    assert_no_errors(
        r#"
declare function sandwich<H, Mid extends unknown[], L>(
    args: [H, ...Mid, L]
): { head: H; mid: Mid; last: L };
const r = sandwich(["a", 1, true]);
const ok: { head: string; mid: [number]; last: boolean } = r;
"#,
        "sandwich: H=string, Mid=[number], L=boolean",
    );
}

#[test]
fn sandwich_wrong_mid_fails() {
    assert_only_one_2322(
        r#"
declare function sandwich<H, Mid extends unknown[], L>(
    args: [H, ...Mid, L]
): { head: H; mid: Mid; last: L };
const r = sandwich(["a", 1, true]);
const bad: { head: string; mid: [string]; last: boolean } = r;
"#,
        "sandwich: wrong mid-type should fail",
    );
}

#[test]
fn method_this_inference_does_not_treat_second_rest_as_fixed_suffix() {
    let diagnostics = tsz_checker::test_utils::check_source_code_messages(
        r#"
interface Desc<A extends unknown[], T> {
    readonly f: (...args: A) => T;
    bind<T extends unknown[], U extends unknown[], R>(
        this: Desc<[...T, ...U], R>,
        ...args: T
    ): Desc<[...U], R>;
}

declare const a: Desc<[string, number, boolean], object>;
const b = a.bind("", 1);
const ok: Desc<[boolean], object> = b;
"#,
    );
    assert!(
        diagnostics.is_empty(),
        "consecutive variadic rest segments should preserve the remaining suffix, got {diagnostics:#?}",
    );
}

// =============================================================================
// Repeated `infer` names across variadic tuple slots (conditional inference)
// =============================================================================
//
// Structural rule: the fixed prefix and suffix slots of a tuple pattern
// `[p…, ...R, s…]` are co-located *covariant* positions. When the same
// `infer T` name appears in more than one of them, `tsc` unions the per-slot
// candidates (e.g. `[infer A, ...unknown[], infer A]` against `[1, 2, 3]`
// binds `A = 1 | 3`) instead of failing the second slot's mutual-subtype check
// and collapsing to the false branch. A name shared between a fixed slot and
// the rest slot itself (`[infer A, ...infer A]`) stays a conflict and takes
// the false branch, matching `tsc`'s post-inference re-check. Binder names are
// varied per case so the behavior is structural, not name-driven.

/// Header providing an exact type-identity probe. `Eq<X, Y>` is `true` only
/// when `X` and `Y` are mutually identical; `Assert<T extends true>` raises
/// TS2344 when its argument is not exactly `true`, so a correct inference
/// yields zero diagnostics and a wrong one yields exactly `[2344]`.
const EQ_HEADER: &str = r#"
type Eq<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
"#;

/// Each `body` ends in an `Assert<Eq<…>>` that encodes the type tsc produces
/// (true or false branch). Matching tsc therefore means zero diagnostics; a
/// wrong inference makes the `Eq` probe `false` and surfaces TS2344.
fn assert_matches_tsc(body: &str, label: &str) {
    let source = format!("{EQ_HEADER}{body}");
    let codes = check_source_codes(&source);
    assert!(
        codes.is_empty(),
        "{label}: expected the inferred type to match tsc (no diagnostics), got {codes:?}"
    );
}

#[test]
fn prefix_and_suffix_same_infer_unions_candidates() {
    assert_matches_tsc(
        r#"
type FirstOrLast<Items extends unknown[]> =
  Items extends [infer Edge, ...unknown[], infer Edge] ? Edge : "none";
type _ = Assert<Eq<FirstOrLast<[1, 2, 3]>, 1 | 3>>;
"#,
        "prefix+suffix repeated infer over [1,2,3]",
    );
}

#[test]
fn prefix_and_suffix_same_infer_two_element_source() {
    assert_matches_tsc(
        r#"
type Ends<Seq extends unknown[]> =
  Seq extends [infer Mark, ...unknown[], infer Mark] ? Mark : "none";
type _ = Assert<Eq<Ends<[1, 2]>, 1 | 2>>;
"#,
        "prefix+suffix repeated infer over a 2-tuple (empty middle)",
    );
}

#[test]
fn too_short_source_takes_false_branch() {
    assert_matches_tsc(
        r#"
type Ends<Seq extends unknown[]> =
  Seq extends [infer Mark, ...unknown[], infer Mark] ? Mark : "none";
type _ = Assert<Eq<Ends<[1]>, "none">>;
"#,
        "single-element source cannot fill prefix+suffix, false branch",
    );
}

#[test]
fn repeated_infer_with_captured_middle_rest() {
    assert_matches_tsc(
        r#"
type EdgesAndCore<List extends unknown[]> =
  List extends [infer Side, ...infer Core, infer Side] ? [Side, Core] : "none";
type _ = Assert<Eq<EdgesAndCore<[1, 2, 3, 4]>, [1 | 4, [2, 3]]>>;
"#,
        "repeated edge infer unions while distinct middle rest stays exact",
    );
}

#[test]
fn repeated_infer_with_extra_fixed_prefix_slot() {
    assert_matches_tsc(
        r#"
type EdgePair<Row extends unknown[]> =
  Row extends [infer Corner, infer Inner, ...unknown[], infer Corner]
    ? [Corner, Inner]
    : "none";
type _ = Assert<Eq<EdgePair<[1, 2, 3, 4]>, [1 | 4, 2]>>;
"#,
        "repeated corner infer unions, distinct interior slot stays exact",
    );
}

#[test]
fn repeated_infer_in_two_prefix_slots() {
    assert_matches_tsc(
        r#"
type FirstTwo<Tup extends unknown[]> =
  Tup extends [infer Cell, infer Cell, ...unknown[]] ? Cell : "none";
type _ = Assert<Eq<FirstTwo<[1, 2, 3]>, 1 | 2>>;
"#,
        "two adjacent prefix slots share an infer name",
    );
}

#[test]
fn repeated_infer_in_two_suffix_slots() {
    assert_matches_tsc(
        r#"
type LastTwo<Tup extends unknown[]> =
  Tup extends [...unknown[], infer Cell, infer Cell] ? Cell : "none";
type _ = Assert<Eq<LastTwo<[1, 2, 3, 4]>, 3 | 4>>;
"#,
        "two adjacent suffix slots share an infer name",
    );
}

#[test]
fn infer_shared_between_fixed_slot_and_rest_slot_takes_false_branch() {
    assert_matches_tsc(
        r#"
type FixedThenRest<Tup extends unknown[]> =
  Tup extends [infer Cell, ...infer Cell] ? Cell : "none";
type _ = Assert<Eq<FixedThenRest<[1, 2, 3]>, "none">>;
"#,
        "element-level vs array-level candidate conflict, false branch",
    );
}

#[test]
fn distinct_edge_infers_stay_exact() {
    // Guard: the union only applies to repeated names; distinct names must keep
    // their individual candidates exactly.
    assert_matches_tsc(
        r#"
type BothEnds<Tup extends unknown[]> =
  Tup extends [infer Head, ...unknown[], infer Tail] ? [Head, Tail] : "none";
type _ = Assert<Eq<BothEnds<[1, 2, 3]>, [1, 3]>>;
"#,
        "distinct head/tail infers keep exact per-slot candidates",
    );
}

#[test]
fn three_way_repeated_infer_unions_all_slots() {
    assert_matches_tsc(
        r#"
type TripleMark<Tup extends unknown[]> =
  Tup extends [infer Mark, ...unknown[], infer Mark, infer Mark] ? Mark : "none";
type _ = Assert<Eq<TripleMark<[1, 2, 3, 4]>, 1 | 3 | 4>>;
"#,
        "one prefix and two suffix slots share an infer name",
    );
}

#[test]
fn repeated_infer_in_readonly_tuple_pattern() {
    // `readonly` tuples take a distinct relation path; the union over repeated
    // edge infers must still apply.
    assert_matches_tsc(
        r#"
type Edges<Seq extends readonly unknown[]> =
  Seq extends readonly [infer Rim, ...unknown[], infer Rim] ? Rim : "none";
type _ = Assert<Eq<Edges<readonly [1, 2, 3]>, 1 | 3>>;
"#,
        "readonly tuple repeated edge infer unions candidates",
    );
}

#[test]
fn repeated_infer_through_alias_indirection() {
    // The pattern is reached through a second alias with a different type
    // parameter name, guarding against any binder-environment reuse drift.
    assert_matches_tsc(
        r#"
type Bookends<Seq extends unknown[]> =
  Seq extends [infer Edge, ...unknown[], infer Edge] ? Edge : "none";
type Probe<Row extends unknown[]> = Bookends<Row>;
type _ = Assert<Eq<Probe<["a", "b", "c", "a"]>, "a">>;
"#,
        "repeated infer resolves identically through alias indirection",
    );
}

// =============================================================================
// Optional leading element before a rest: `[(infer H)?, ...infer T]`
// =============================================================================
//
// Structural rule: a leading *optional* tuple element may be absent in the
// source, so an empty/short source still matches `[(infer H)?, ...rest]` and
// the conditional takes its TRUE branch. An inference variable that receives no
// candidate resolves to its declared constraint, or its position default
// otherwise — `unknown` for a plain `infer`, `unknown[]` for a rest
// `...infer T`. `tsz` previously counted the optional prefix as required and
// rejected the empty source on arity (false branch), inverting the base case of
// tuple-deconstruction utilities (remeda `TupleParts`/`Head`). Binder names are
// varied per case so the behavior is structural, not name-driven.

#[test]
fn empty_source_matches_optional_prefix_rest_true_branch() {
    // The exact witness from the bug report: the conditional must take the
    // true branch instead of the false branch.
    assert_no_errors(
        r#"
type Test = readonly [] extends readonly [(infer _H)?, ...infer _T] ? "MATCH" : "NO_MATCH";
const t: Test = "MATCH";
"#,
        "empty tuple matches optional-prefix rest pattern (true branch)",
    );
}

#[test]
fn empty_source_optional_prefix_takes_false_branch_when_asserted_no_match() {
    // The negative control: asserting the false-branch literal must now be the
    // type error tsc reports, proving tsz no longer silently takes the false
    // branch.
    assert_only_one_2322(
        r#"
type Test = readonly [] extends readonly [(infer _H)?, ...infer _T] ? "MATCH" : "NO_MATCH";
const t: Test = "NO_MATCH";
"#,
        "asserting NO_MATCH against the true branch must be TS2322",
    );
}

#[test]
fn empty_source_optional_prefix_infers_unknown_and_unknown_array() {
    // tsc: `H = unknown` (absent optional, no candidate), `T = unknown[]`
    // (rest with no source elements to match).
    assert_matches_tsc(
        r#"
type Parts<A extends readonly unknown[]> =
  A extends readonly [(infer H)?, ...infer T] ? [H, T] : "none";
type _ = Assert<Eq<Parts<[]>, [unknown, unknown[]]>>;
"#,
        "empty source infers H=unknown, T=unknown[]",
    );
}

#[test]
fn single_element_source_optional_prefix_infers_head_and_empty_rest() {
    // With the optional prefix fully consumed, the rest gets the (empty) middle
    // slice: `H = 1`, `T = []` — distinct from the empty-source case.
    assert_matches_tsc(
        r#"
type Parts<A extends readonly unknown[]> =
  A extends readonly [(infer H)?, ...infer T] ? [H, T] : "none";
type _ = Assert<Eq<Parts<[1]>, [1, []]>>;
"#,
        "single-element source infers H=1, T=[]",
    );
}

#[test]
fn multi_element_source_optional_prefix_infers_head_and_rest() {
    assert_matches_tsc(
        r#"
type Parts<A extends readonly unknown[]> =
  A extends readonly [(infer H)?, ...infer T] ? [H, T] : "none";
type _ = Assert<Eq<Parts<[1, 2, 3]>, [1, [2, 3]]>>;
"#,
        "multi-element source infers H=1, T=[2, 3]",
    );
}

#[test]
fn two_optional_prefix_slots_partially_filled() {
    // A single-element source fills the first optional slot but not the second;
    // the absent slot is `unknown` and the rest defaults to `unknown[]` because
    // the prefix was not fully consumed.
    assert_matches_tsc(
        r#"
type Parts<A extends readonly unknown[]> =
  A extends readonly [(infer H1)?, (infer H2)?, ...infer T] ? [H1, H2, T] : "none";
type _ = Assert<Eq<Parts<[9]>, [9, unknown, unknown[]]>>;
"#,
        "partially-filled optional prefix: H2=unknown, T=unknown[]",
    );
}

#[test]
fn constrained_optional_prefix_defaults_to_constraint() {
    // A constrained `infer H extends string` with no candidate resolves to its
    // constraint (`string`), not `unknown`.
    assert_matches_tsc(
        r#"
type Head<A extends readonly unknown[]> =
  A extends readonly [(infer H extends string)?, ...infer _] ? H : "none";
type _ = Assert<Eq<Head<[]>, string>>;
"#,
        "constrained absent optional prefix defaults to the constraint",
    );
}

#[test]
fn constrained_rest_infer_defaults_to_constraint() {
    // A constrained rest `...infer T extends number[]` with no candidate
    // resolves to its constraint (`number[]`), not `unknown[]`.
    assert_matches_tsc(
        r#"
type Rest<A extends readonly unknown[]> =
  A extends readonly [(infer _)?, ...(infer T extends number[])] ? T : "none";
type _ = Assert<Eq<Rest<[]>, number[]>>;
"#,
        "constrained absent rest infer defaults to the constraint",
    );
}

#[test]
fn required_prefix_before_rest_still_rejects_short_source() {
    // Guard: a *required* leading element still imposes the minimum arity, so an
    // empty source against `[infer H, ...infer T]` takes the false branch.
    assert_matches_tsc(
        r#"
type Head<A extends readonly unknown[]> =
  A extends readonly [infer H, ...infer _] ? H : "none";
type _ = Assert<Eq<Head<[]>, "none">>;
"#,
        "required prefix still rejects an empty source (false branch)",
    );
}

#[test]
fn optional_prefix_then_required_suffix_via_required_prefix() {
    // A required prefix, optional middle slot, and rest: a 1-element source
    // fills the required head, leaves the optional slot absent (`unknown`), and
    // the rest defaults to `unknown[]`.
    assert_matches_tsc(
        r#"
type Parts<A extends readonly unknown[]> =
  A extends readonly [infer H, (infer M)?, ...infer T] ? [H, M, T] : "none";
type _ = Assert<Eq<Parts<[1]>, [1, unknown, unknown[]]>>;
"#,
        "required head consumed, optional middle absent, rest unknown[]",
    );
}
