//! Regression coverage for distributive conditional evaluation over
//! tuple-like unions.
//!
//! Tracks the bug family described in issue #12175 (witnesses #10799,
//! #10815, #10823, #10831, #10848, #10856, #10864, #10872). The structural
//! rule under test:
//!
//! > When conditional types distribute over tuple-like union inputs, each
//! > union arm's tuple constraints must be preserved and its inferred
//! > results merged independently, matching tsc behavior.
//!
//! Every test varies binder names so a fixture-name fast path cannot pass
//! these checks. Patterns cover head, tail, head+rest, init+last, sandwich
//! (init+mid+last), recursive, constrained-infer, mixed readonly/mutable,
//! and aliased-variant shapes; both distributive (`T extends ...`) and
//! non-distributive (`[T] extends [...]`) forms are exercised so the
//! per-arm-merge path and the all-arms-must-match path stay distinct.

use super::super::core::*;

fn no_assignability_errors(source: &str) {
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        !has_error(&diagnostics, 2322) && !has_error(&diagnostics, 2345),
        "expected no assignability errors. diagnostics: {diagnostics:#?}\nsource:\n{source}"
    );
}

#[test]
fn distributive_head_over_tuple_union_keeps_per_variant_precision() {
    no_assignability_errors(
        r#"
type FirstElement<Tup> = Tup extends [infer Head, ...unknown[]] ? Head : never;

type TupleUnion = [string, boolean] | [number];
type Resolved = FirstElement<TupleUnion>;

declare const resolved: Resolved;
const accepted: string | number = resolved;
"#,
    );
}

#[test]
fn distributive_head_rejects_widening_to_unrelated_member() {
    let source = r#"
type FirstSlot<Tup> = Tup extends [infer Head, ...unknown[]] ? Head : never;

type Pair = [string, number] | [boolean];
type Pulled = FirstSlot<Pair>;

declare const pulled: Pulled;
const onlyString: string = pulled;
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        has_error(&diagnostics, 2322),
        "expected TS2322 because boolean does not fit string. diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn distributive_head_with_alias_chain_preserves_variant_precision() {
    no_assignability_errors(
        r#"
type LeftPair = [string, number];
type RightPair = [boolean];
type Combined = LeftPair | RightPair;

type Front<Tup> = Tup extends [infer Hd, ...unknown[]] ? Hd : never;
type Picked = Front<Combined>;

declare const picked: Picked;
const allowed: string | boolean = picked;
"#,
    );
}

#[test]
fn distributive_tail_rest_keeps_per_variant_tuples() {
    no_assignability_errors(
        r#"
type RestSlot<Tup> = Tup extends [unknown, ...infer Tail] ? Tail : never;

type Mixed = [string, number, boolean] | [string, boolean] | [string];
type Rest = RestSlot<Mixed>;

declare const rest: Rest;
const allowed: [number, boolean] | [boolean] | [] = rest;
"#,
    );
}

#[test]
fn distributive_head_and_rest_box_pattern_preserves_each_arm() {
    // Mirrors issue #10856 / #10848 minimal-repro intent (`Box<T> = T extends
    // [infer H, ...infer R] ? [H, ...R] : []`) with renamed binders.
    no_assignability_errors(
        r#"
type Rebuild<Seq> = Seq extends [infer Front, ...infer Back] ? [Front, ...Back] : [];

type Variants = [string, string] | [number, number] | [];
type Rebuilt = Rebuild<Variants>;

declare const rebuilt: Rebuilt;
const allowed: [string, string] | [number, number] | [] = rebuilt;
"#,
    );
}

#[test]
fn distributive_init_last_pattern_preserves_per_variant_arity() {
    no_assignability_errors(
        r#"
type Trunk<Tup> = Tup extends [...infer Init, infer Last] ? { init: Init; last: Last } : never;

type AllTuples = [1, 2, 3] | [10, 20] | [99];
type Split = Trunk<AllTuples>;

declare const split: Split;
const allowed:
  | { init: [1, 2]; last: 3 }
  | { init: [10]; last: 20 }
  | { init: []; last: 99 } = split;
"#,
    );
}

#[test]
fn distributive_sandwich_pattern_extracts_mid_per_variant() {
    no_assignability_errors(
        r#"
type Inner<Tup> = Tup extends [unknown, ...infer Mid, unknown] ? Mid : never;

type Cases = [1, 2, 3] | [10, 11, 12, 13];
type MidSlices = Inner<Cases>;

declare const slices: MidSlices;
const allowed: [2] | [11, 12] = slices;
"#,
    );
}

#[test]
fn distributive_constrained_head_filters_per_variant() {
    no_assignability_errors(
        r#"
type StrictHead<Tup extends readonly unknown[]> =
  Tup extends readonly [infer Head extends string, ...infer Rest]
    ? readonly [Head, ...Rest]
    : "ERROR";

type Variants = [string, number] | [number, string] | ["literal", boolean] | [];
type Output = StrictHead<Variants>;

declare const output: Output;
const allowed:
  | readonly [string, number]
  | "ERROR"
  | readonly ["literal", boolean] = output;
"#,
    );
}

#[test]
fn distributive_classify_over_tuple_union_preserves_variant_value() {
    // Original-shape repro from issues #10848/#10856 (broken in templated
    // body; this is the well-formed form with renamed binders).
    no_assignability_errors(
        r#"
type Categorise<Item> = Item extends unknown
  ? Item extends string
    ? { kind: 'string'; value: Item }
    : Item extends number
      ? { kind: 'number'; value: Item }
      : { kind: 'other'; value: Item }
  : never;

type Shapes = [string, string] | [number, number] | [];
type Sorted = Categorise<Shapes>;

declare const sorted: Sorted;
const allowed:
  | { kind: 'other'; value: [string, string] }
  | { kind: 'other'; value: [number, number] }
  | { kind: 'other'; value: [] } = sorted;
"#,
    );
}

#[test]
fn non_distributive_tuple_wrapper_unions_all_inferred_heads() {
    no_assignability_errors(
        r#"
type WrappedHead<T> = [T] extends [readonly [infer H, ...unknown[]]] ? H : never;

type Both = [string, number] | [boolean];
type Combined = WrappedHead<Both>;

declare const combined: Combined;
const allowed: string | boolean = combined;
"#,
    );
}

#[test]
fn non_distributive_tuple_wrapper_fails_when_any_variant_misses() {
    no_assignability_errors(
        r#"
type WrappedHead<T> = [T] extends [readonly [infer H, ...unknown[]]] ? H : never;

type WithEmpty = [string, number] | [];
type Maybe = WrappedHead<WithEmpty>;

declare const maybe: Maybe;
const allowed: never = maybe;
"#,
    );
}

#[test]
fn distributive_mixed_readonly_and_mutable_tuple_union() {
    no_assignability_errors(
        r#"
type FrontElt<Tup> = Tup extends readonly [infer Hd, ...unknown[]] ? Hd : never;

type Mixed = readonly [string, number] | [boolean, string];
type Picked = FrontElt<Mixed>;

declare const picked: Picked;
const allowed: string | boolean = picked;
"#,
    );
}

#[test]
fn distributive_recursive_reverse_over_tuple_union() {
    no_assignability_errors(
        r#"
type Flip<Tup extends readonly unknown[]> =
  Tup extends readonly [infer Hd, ...infer Tl] ? [...Flip<Tl>, Hd] : [];

type Triplets = [1, 2, 3] | [10, 20] | [];
type Flipped = Flip<Triplets>;

declare const flipped: Flipped;
const allowed: [3, 2, 1] | [20, 10] | [] = flipped;
"#,
    );
}

#[test]
fn distributive_over_tuple_union_with_no_infer_wrapper_round_trips() {
    no_assignability_errors(
        r#"
type Pick0<Tup> = Tup extends [infer Hd, ...unknown[]] ? Hd : never;

declare function take<Args>(input: Args, fallback: NoInfer<Pick0<Args>>): Pick0<Args>;

const value = take([] as [string, number] | [boolean], 'fb');
const allowed: string | boolean = value;
"#,
    );
}

#[test]
fn distributive_head_with_class_subtype_variants() {
    no_assignability_errors(
        r#"
class Base { tag = 'b' as const; }
class Derived extends Base { mark = 1; }

type FirstClass<Tup> = Tup extends readonly [infer Hd, ...unknown[]] ? Hd : never;

type Cases = [Base, number] | [Derived, string];
type Heads = FirstClass<Cases>;

declare const heads: Heads;
const allowed: Base | Derived = heads;
"#,
    );
}

#[test]
fn distributive_zip_pair_of_tuples_over_union() {
    no_assignability_errors(
        r#"
type ZipPair<Tup> =
  Tup extends readonly [readonly [infer Fa, infer Fb], readonly [infer Sa, infer Sb]]
    ? [Fa, Fb, Sa, Sb]
    : never;

type Inputs = [["a", true], ["b", false]] | [[1, 2], [3, 4]];
type Flat = ZipPair<Inputs>;

declare const flat: Flat;
const allowed: ["a", true, "b", false] | [1, 2, 3, 4] = flat;
"#,
    );
}

#[test]
fn distributive_function_args_over_signature_union_preserves_param_lists() {
    no_assignability_errors(
        r#"
type ParamsOf<F> = F extends (...args: infer Args) => unknown ? Args : never;

type Handlers = ((path: string) => void) | ((code: number, ok: boolean) => void);
type AllArgs = ParamsOf<Handlers>;

declare const argv: AllArgs;
const allowed: [path: string] | [code: number, ok: boolean] = argv;
"#,
    );
}

#[test]
fn distributive_constrained_head_picks_only_passing_variant() {
    no_assignability_errors(
        r#"
type FilterStringHead<Tup> = Tup extends [infer Hd, ...unknown[]]
  ? Hd extends string
    ? Hd
    : never
  : never;

type Variants = [string, 1] | [number, 2] | ["literal"];
type Heads = FilterStringHead<Variants>;

declare const heads: Heads;
const allowed: string = heads;
"#,
    );
}

#[test]
fn distributive_tuple_length_distribute() {
    no_assignability_errors(
        r#"
type LengthOf<Tup extends readonly unknown[]> = Tup['length'];

type Tuples = [1] | [1, 2] | [1, 2, 3];
type Sizes = LengthOf<Tuples>;

declare const sizes: Sizes;
const allowed: 1 | 2 | 3 = sizes;
"#,
    );
}

// ---------------------------------------------------------------------------
// Object-valued distributive branches over a *deferred* union check side
// (issue #10864). When the conditional's check type is a wrapping alias /
// application (`Wrap<U>`, an inline `Id<U>` argument) rather than a literal
// union, the per-member rewrite runs through `substitute_exact_type`. If that
// rewrite does not reach into object property types, every union member
// collapses to one widened object (`{ value: A | B }`) and the conditional
// becomes over-constrained. Binder names are varied so no fixture-name path
// can match.
// ---------------------------------------------------------------------------

#[test]
fn distributive_object_branch_through_inline_application_arg() {
    no_assignability_errors(
        r#"
type Tag<Elem> = Elem extends unknown ? { value: Elem } : never;
type Members = [string] | [number];

// `Tag<Members>` is passed inline as an application argument; the outer
// conditional's check side is therefore a deferred application, not a literal
// union. tsc keeps per-member precision: { value: [string] } | { value: [number] }.
type Same<Left, Right> = [Left] extends [Right]
  ? ([Right] extends [Left] ? true : false)
  : false;

const exact: Same<
  Tag<Members>,
  { value: [string] } | { value: [number] }
> = true;
"#,
    );
}

#[test]
fn distributive_object_branch_through_wrapping_alias() {
    no_assignability_errors(
        r#"
type Wrap<Inner> = Classify<Inner>;
type Classify<Item> = Item extends unknown
  ? Item extends string
    ? { kind: "string"; value: Item }
    : { kind: "other"; value: Item }
  : never;

type Inputs = [string, string] | [number, number] | [];

type Equal<Lhs, Rhs> = [Lhs] extends [Rhs]
  ? ([Rhs] extends [Lhs] ? true : false)
  : false;

// V's body is Application(Wrap, [Inputs]) — a deferred check side. Each tuple
// arm classifies to { kind: "other"; value: <arm> }; precision must survive.
const ok: Equal<
  Wrap<Inputs>,
  | { kind: "other"; value: [string, string] }
  | { kind: "other"; value: [number, number] }
  | { kind: "other"; value: [] }
> = true;
"#,
    );
}

#[test]
fn distributive_object_branch_inline_application_rejects_widening() {
    // The widened collapse `{ value: [string] | [number] }` must NOT be
    // accepted as the result, which would be the pre-fix (over-constrained)
    // behavior. Assigning a widened-shaped value to the per-member union fails.
    let source = r#"
type Mark<Slot> = Slot extends unknown ? { value: Slot } : never;
type Slots = [string] | [number];
type Result = Mark<Slots>;

declare const widened: { value: [string] | [number] };
const back: Result = widened;
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        has_error(&diagnostics, 2322),
        "widened single object must not satisfy the per-member distributed union. diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn distributive_nested_object_branch_preserves_inner_variable() {
    no_assignability_errors(
        r#"
type Deep<Elem> = Elem extends unknown ? { outer: { inner: Elem } } : never;
type Arms = [string] | [number];
type Pass<X> = Deep<X>;

type Same<Left, Right> = [Left] extends [Right]
  ? ([Right] extends [Left] ? true : false)
  : false;

const ok: Same<
  Pass<Arms>,
  { outer: { inner: [string] } } | { outer: { inner: [number] } }
> = true;
"#,
    );
}
