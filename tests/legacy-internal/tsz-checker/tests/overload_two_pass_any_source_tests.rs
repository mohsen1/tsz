//! Two-pass overload resolution with `any` arguments (issue #13042).
//!
//! Structural rule: tsc's `chooseOverload` runs twice — first with the
//! subtype relation, where an `any` SOURCE is not related to concrete
//! targets (at every nesting level), then with the assignable relation in
//! declaration order. With an `any` argument and a mixed
//! non-generic/generic overload set, the non-generic candidate is skipped
//! in pass 1 and the generic one wins with `U = any`; when every candidate
//! fails pass 1, the first assignable candidate wins in declaration order.
//!
//! tsz routes pass 1 through the solver's `AnySourceNotRelated` relation
//! mode via `resolve_call_with_checker_adapter_subtype_pass`.
//!
//! The inferred result types are pinned through probe assignments:
//! a probe against an incompatible concrete target errors (TS2322) when the
//! call result is concrete and stays silent when the result is `any`.

use crate::test_utils::{check_source_diagnostics, diagnostic_count};

fn count(source: &str, code: u32) -> usize {
    diagnostic_count(&check_source_diagnostics(source), code)
}

/// Mixed overload set, `any` argument: the generic candidate wins pass 1
/// with `U = any`, so the call result is `any` (matrix case `e`).
#[test]
fn any_argument_prefers_generic_candidate_over_first_nongeneric() {
    let source = r#"
declare function grabItem(key: string): "s";
declare function grabItem<TPick>(key: TPick, alt?: TPick): TPick;
declare const opaque: any;
const picked = grabItem(opaque);
const probeNum: number = picked;
const probeObj: { marker: number } = picked;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "result should be `any` (generic candidate with TPick = any), not the literal \"s\""
    );
}

/// Declaration order with the generic candidate first: pass 1 selects it
/// directly, so the result is still `any`.
#[test]
fn any_argument_generic_first_declaration_order_still_any() {
    let source = r#"
declare function fetchSlot<TBox>(handle: TBox, fallback?: TBox): TBox;
declare function fetchSlot(handle: string): "s";
declare const blob: any;
const slot = fetchSlot(blob);
const probeNum: number = slot;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "generic-first declaration order should also produce `any`"
    );
}

/// All-non-generic overloads with an `any` argument: every candidate fails
/// pass 1, and pass 2 picks the FIRST candidate in declaration order
/// (matrix case `e2`).
#[test]
fn any_argument_all_nongeneric_falls_back_in_declaration_order() {
    let source = r#"
declare function stamp(value: string): "s";
declare function stamp(value: number): "n";
declare const fuzzy: any;
const tag = stamp(fuzzy);
const probeWrong: "n" = tag;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "pass 2 must select the first overload in declaration order (result \"s\", not `any`/\"n\")"
    );

    let ok_source = r#"
declare function stamp(value: string): "s";
declare function stamp(value: number): "n";
declare const fuzzy: any;
const tag = stamp(fuzzy);
const probeRight: "s" = tag;
"#;
    assert_eq!(count(ok_source, 2322), 0, "result should be exactly \"s\"");
}

/// When the generic candidate cannot match the call arity, the non-generic
/// candidate still wins through the pass-2 fallback.
#[test]
fn any_argument_generic_arity_mismatch_keeps_nongeneric_winner() {
    let source = r#"
declare function routeKey(name: string): "s";
declare function routeKey<TPair>(name: TPair, partner: TPair): TPair;
declare const loose: any;
const route = routeKey(loose);
const probeWrong: "x" = route;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "generic candidate fails arity, so the non-generic \"s\" result must win (not `any`)"
    );
}

/// Non-`any` arguments are unaffected: the first matching overload wins as
/// before, in both literal and numeric forms.
#[test]
fn concrete_arguments_keep_existing_overload_selection() {
    let source = r#"
declare function render(value: string): "s";
declare function render(value: number): "n";
const a = render("title");
const b = render(42);
const probeA: "s" = a;
const probeB: "n" = b;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "concrete arguments must keep today's overload selection"
    );
}

/// Reduce-style overload pair with an `any` seed: the union-typed callback
/// result flows from the generic candidate with `U = any` (matrix cases
/// `a`/`b`).
#[test]
fn reduce_like_any_seed_selects_generic_candidate() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
declare const opaqueSeed: any;
const merged = segments.fold((acc, item) => acc[item], opaqueSeed);
const probeNum: number = merged;
const constant = segments.fold(() => "x", opaqueSeed);
const probeNum2: number = constant;
"#;
    assert_eq!(
        count(source, 2322),
        0,
        "any seed must select the generic fold (TAcc = any); result is `any`, not string"
    );
}

/// Reduce-style call with a concrete seed keeps the string overload
/// (matrix case `d`).
#[test]
fn reduce_like_concrete_seed_keeps_string_result() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
const collapsed = segments.fold(() => "x", "start");
const probeNum: number = collapsed;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "concrete seed keeps the string overload; string is not assignable to number"
    );
}

/// An explicit callback parameter annotation makes the generic candidate
/// fail pass 1 too — the rejection applies inside the nested callback
/// parameter comparison, not just at the top-level argument (matrix case
/// `c`). Declaration order then selects the string overload in pass 2.
#[test]
fn annotated_callback_param_rejects_generic_candidate_at_nested_level() {
    let source = r#"
interface SegmentList {
    fold(merge: (acc: string, item: string) => string): string;
    fold(merge: (acc: string, item: string) => string, seed: string): string;
    fold<TAcc>(merge: (acc: TAcc, item: string) => TAcc, seed: TAcc): TAcc;
}
declare const segments: SegmentList;
declare const opaqueSeed: any;
const joined = segments.fold((acc: string, item) => acc + item, opaqueSeed);
const probeNum: number = joined;
"#;
    assert_eq!(
        count(source, 2322),
        1,
        "annotated callback param must reject the generic candidate in pass 1 (nested any source); result is string"
    );
}

/// The same nested-`any` rule must survive the generic call evaluator's
/// aggregate variadic-rest relation. That relation uses a provisional-rest
/// policy, but it must still compose with pass 1's `AnySourceNotRelated`
/// policy rather than falling back to ordinary `any` propagation.
#[test]
fn generic_aggregate_rest_preserves_nested_any_rejection_during_subtype_pass() {
    let declarations = r#"
declare function select(value: string, callback: (value: string) => void): "fixed";
declare function select<TValues extends readonly unknown[]>(
    ...args: [...TValues, (...values: TValues) => void]
): "generic";

declare const opaque: any;
const selected = select(opaque, (value: string) => {});
"#;
    assert_eq!(
        count(
            &format!("{declarations}\nconst fixedProbe: \"fixed\" = selected;"),
            2322,
        ),
        0,
        "both candidates must fail pass 1, so pass 2 selects the first fixed overload"
    );
    assert_eq!(
        count(
            &format!("{declarations}\nconst genericProbe: \"generic\" = selected;"),
            2322,
        ),
        1,
        "the generic probe must fail when the fixed overload wins pass 2"
    );
}

#[test]
fn provisional_aggregate_rest_does_not_relax_nested_fixed_callback_slots() {
    let source = r#"
declare function take<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, (x: Outer) => void]
): void;
function f<Outer extends unknown[]>(source: (...args: Outer) => void) {
    take<Outer>(source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "the provisional aggregate policy must not leak into the nested callback relation"
    );
}

#[test]
fn provisional_aggregate_rest_matches_tsc_during_context_instantiation_retry() {
    let source = r#"
type Deferred<Value> = Value extends unknown ? Value : never;
type Target<Outer extends unknown[]> =
    <Inner>(callback: (...args: [] | [...Outer]) => void) => Deferred<Inner>;
declare function take<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, Target<Outer>]
): void;
function f<Outer extends unknown[]>(
    source: <Inner>(callback: (...args: Outer) => void) => Inner,
) {
    take<Outer>(source);
}
"#;
    assert_eq!(
        count(source, 2345),
        0,
        "the nested generic context-instantiation shape is accepted by `tsc`"
    );
}

#[test]
fn context_instantiation_retry_preserves_rigid_nested_rest_failures() {
    let source = r#"
type Deferred<Value> = Value extends unknown ? Value : never;

type DirectSource<Outer extends unknown[]> =
    <Inner>(callback: (...args: [] | [...Outer]) => void) => Inner;
type DirectTarget<Outer extends unknown[]> =
    <Inner>(callback: (...args: Outer) => void) => Deferred<Inner>;
declare function takeDirect<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, DirectTarget<Outer>]
): void;
function direct<Outer extends unknown[]>(source: DirectSource<Outer>) {
    takeDirect<Outer>(source);
}

type NullableSource<Outer extends unknown[]> =
    <Inner>(callback: ((...args: [] | [...Outer]) => void) | undefined) => Inner;
type NullableTarget<Outer extends unknown[]> =
    <Inner>(callback: ((...args: Outer) => void) | undefined) => Deferred<Inner>;
declare function takeNullable<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, NullableTarget<Outer>]
): void;
function nullable<Outer extends unknown[]>(source: NullableSource<Outer>) {
    takeNullable<Outer>(source);
}

type TupleSource<Outer extends unknown[]> =
    <Inner>(...args: [callback: (...args: [] | [...Outer]) => void]) => Inner;
type TupleTarget<Outer extends unknown[]> =
    <Inner>(...args: [callback: (...args: Outer) => void]) => Deferred<Inner>;
declare function takeTuple<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, TupleTarget<Outer>]
): void;
function tupled<Outer extends unknown[]>(source: TupleSource<Outer>) {
    takeTuple<Outer>(source);
}

type MethodSource<Outer extends unknown[]> = {
    method<Inner>(callback: (...args: [] | [...Outer]) => void): Inner;
}["method"];
type MethodTarget<Outer extends unknown[]> = {
    method<Inner>(callback: (...args: Outer) => void): Deferred<Inner>;
}["method"];
declare function takeMethod<Outer extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, MethodTarget<Outer>]
): void;
function method<Outer extends unknown[]>(source: MethodSource<Outer>) {
    takeMethod<Outer>(source);
}
"#;
    assert_eq!(
        count(source, 2345),
        4,
        "a contextual retry must preserve rigid nested rest failures through aliases, nullish wrappers, tuple-rest slots, and extracted methods"
    );
}

#[test]
fn context_instantiation_retry_preserves_rigid_overloaded_callback_failures() {
    let source = r#"
type DeferredOverload<Value> = Value extends unknown ? Value : never;
type OverloadedSource<Outer extends unknown[]> =
    <Inner>(callback: {
        (...args: [] | [...Outer]): void;
        (...args: [number]): void;
    }) => Inner;
type OverloadedTarget<Outer extends unknown[]> =
    <Inner>(callback: {
        (...args: Outer): void;
        (...args: [number]): void;
    }) => DeferredOverload<Inner>;
declare function takeOverloaded<
    Outer extends unknown[],
    Prefix extends unknown[] = [],
>(...args: [...Prefix, OverloadedTarget<Outer>]): void;
function overloaded<Outer extends unknown[]>(source: OverloadedSource<Outer>) {
    takeOverloaded<Outer>(source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "an overload sibling must not let contextual instantiation erase a rigid bare-rest failure"
    );
}

#[test]
fn context_instantiation_retry_normalizes_inner_tuple_rest_surfaces() {
    let source = r#"
type Deferred<Value> = Value extends infer Inner ? Inner : never;
type Target<Pack extends unknown[]> =
    <Result>(callback: (...args: Pack) => void) => Deferred<Result>;
declare function take<Pack extends unknown[], Prefix extends unknown[] = []>(
    ...args: [...Prefix, Target<Pack>]
): void;

type FixedSource<Pack extends unknown[]> =
    <Result>(callback: (...args: [Pack]) => void) => Result;
function fixed<Pack extends unknown[]>(source: FixedSource<Pack>) {
    take<Pack>(source);
}

type SpreadSource<Pack extends unknown[]> =
    <Result>(callback: (...args: [...Pack]) => void) => Result;
function spread<Pack extends unknown[]>(source: SpreadSource<Pack>) {
    take<Pack>(source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "a fixed one-tuple rest is not the generic pack, while a variadic tuple spread is"
    );
}

#[test]
fn provisional_aggregate_rest_preserves_resolver_aware_source_aliases() {
    let source = r#"
type Identity<Pack extends unknown[]> = Pack;
type ConditionalIdentity<Pack extends unknown[]> =
    Pack extends unknown[] ? Pack : never;
declare function take<Values extends readonly unknown[]>(
    ...args: [...Values, (...args: Values) => void]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    direct: (...args: Identity<Outer>) => void,
    conditional: (...args: ConditionalIdentity<Outer>) => void,
) {
    take(...prefix, direct);
    take(...prefix, conditional);
}
"#;
    assert_eq!(
        count(source, 2345),
        0,
        "transparent source aliases must retain the call-owned bare-rest provenance"
    );
}

#[test]
fn provisional_aggregate_rest_supports_callable_interface_slots() {
    let source = r#"
interface Callback<Pack extends unknown[]> {
    (...args: Pack): void;
}
declare function take<Values extends readonly unknown[]>(
    ...args: [...Values, Callback<Values>]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    callback: Callback<Outer>,
) {
    take(...prefix, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        0,
        "call-signature interfaces participate in the direct aggregate callback slot"
    );
}

#[test]
fn provisional_aggregate_rest_crosses_no_infer_callable_wrappers() {
    let source = r#"
interface Callback<Pack extends unknown[]> {
    (...args: NoInfer<Pack>): void;
}
declare function take<Values extends unknown[]>(
    ...args: [...Values, NoInfer<Callback<Values>>]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    callback: Callback<Outer>,
) {
    take(...prefix, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        0,
        "transparent `NoInfer` wrappers around the slot and its rest binder retain direct call-owned provenance"
    );
}

#[test]
fn provisional_aggregate_rest_rejects_mixed_user_union_overloads() {
    let source = r#"
interface Target<Pack extends unknown[]> {
    (...args: Pack): void;
    (...args: [] | [...Pack]): void;
}
interface Source<Pack extends unknown[]> {
    (...args: Pack): void;
}
declare function take<Values extends unknown[]>(
    ...args: [...Values, Target<Values>]
): void;
function f<Outer extends unknown[]>(prefix: Outer, source: Source<Outer>) {
    take(...prefix, source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "a call-owned overload must not authorize a sibling user-written union-rest overload"
    );
}

#[test]
fn provisional_aggregate_rest_is_scoped_to_the_logical_argument_slot() {
    let source = r#"
declare function take<Values extends unknown[]>(
    ...args: [
        ...Values,
        provisional: (...args: Values) => void,
        rigid: (...args: [] | [...Values]) => void,
    ]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    source: (...args: Outer) => void,
) {
    take(...prefix, source, source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "an inferred aggregate slot must not transfer provenance to an identical user-union sibling"
    );
}

#[test]
fn ordinary_union_inference_is_not_provisional() {
    let source = r#"
declare function inferred<Values extends unknown[]>(
    value: Values,
    callback: (...args: NoInfer<Values>) => void,
): void;
function f<Outer extends unknown[]>(
    value: [] | [...Outer],
    callback: (...args: Outer) => void,
) {
    inferred(value, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "ordinary inference of a union must not acquire aggregate provenance"
    );
}

#[test]
fn explicit_union_type_argument_is_not_provisional() {
    let source = r#"
declare function explicit<Values extends unknown[]>(
    callback: (...args: Values) => void,
): void;
function f<Outer extends unknown[]>(callback: (...args: Outer) => void) {
    explicit<[] | [...Outer]>(callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "an explicitly supplied union must not acquire aggregate provenance"
    );
}

#[test]
fn provisional_aggregate_rest_crosses_direct_optional_and_nullable_wrappers() {
    let source = r#"
declare function optional<Values extends unknown[]>(
    ...args: [...Values, callback?: (...args: Values) => void]
): void;
declare function nullable<Values extends unknown[]>(
    ...args: [...Values, callback: ((...args: Values) => void) | undefined]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    callback: (...args: Outer) => void,
) {
    optional(...prefix, callback);
    nullable(...prefix, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        0,
        "optional metadata and a single nullish shell retain direct aggregate provenance"
    );
}

#[test]
fn optional_aggregate_callback_tail_right_aligns_fixed_but_not_open_spreads() {
    let concrete_open = r#"
declare function optional<Values extends unknown[]>(
    ...args: [...Values, callback?: (...args: Values) => void]
): void;
declare const openStrings: string[];

optional(...openStrings);
"#;
    assert_eq!(
        count(concrete_open, 2345),
        0,
        "a concrete open spread stays in the variadic middle"
    );

    let generic_open = r#"
declare function optional<Values extends unknown[]>(
    ...args: [...Values, callback?: (...args: Values) => void]
): void;
function omit<Outer extends unknown[]>(prefix: Outer) {
    optional(...prefix);
}
"#;
    assert_eq!(
        count(generic_open, 2345),
        0,
        "a generic open spread stays in the variadic middle"
    );

    let rejected = r#"
declare function optional<Values extends unknown[]>(
    ...args: [...Values, callback?: (...args: Values) => void]
): void;
declare const fixedPair: [string, boolean];
optional(...fixedPair);
"#;
    assert_eq!(
        count(rejected, 2345),
        1,
        "a fixed tuple's trailing value is reserved for the optional callback"
    );
}

#[test]
fn expanded_spread_mismatch_is_not_refreshed_from_a_later_source_argument() {
    let source = r#"
declare function take<T>(
    ...args: [first: string, second: number, tail: T]
): void;
declare const pair: [string, boolean];

take(...pair, 0);
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "an expanded spread slot keeps its own mismatch when a later source argument shares its numeric index"
    );
}

#[test]
fn alias_application_marks_only_its_variadic_tuple_binder() {
    let source = r#"
type Args<Prefix extends unknown[], Fixed extends unknown[]> =
    [...Prefix, value: Fixed, callback: (...args: NoInfer<Fixed>) => void];
declare function take<Prefix extends unknown[], Fixed extends unknown[]>(
    ...args: Args<Prefix, Fixed>
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    value: [] | [...Outer],
    callback: (...args: Outer) => void,
) {
    take(...prefix, value, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "an alias application must not mark its fixed suffix binder as aggregate-owned"
    );
}

#[test]
fn aggregate_participation_does_not_override_an_ordinary_candidate() {
    let source = r#"
declare function take<Values extends unknown[]>(
    value: Values,
    ...args: [...Values, callback: (...args: NoInfer<Values>) => void]
): void;
function f<Outer extends unknown[]>(
    value: [] | [...Outer],
    callback: (...args: Outer) => void,
) {
    take(value, callback);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "a prior fixed-parameter candidate keeps an aggregate participant rigid"
    );
}

#[test]
fn provisional_aggregate_rest_does_not_cross_object_wrappers() {
    let source = r#"
declare function take<Outer extends unknown[]>(
    ...args: [] | [{ callback: (...args: [] | [...Outer]) => void }]
): void;
function f<Outer extends unknown[]>(source: (...args: Outer) => void) {
    take<Outer>({ callback: source });
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "only a direct aggregate tuple callback slot may arm the provisional relation"
    );
}

#[test]
fn user_declared_direct_union_rest_is_not_provisional() {
    let source = r#"
declare function take<Outer extends unknown[], Prefix extends unknown[]>(
    ...args: [...Prefix, (...args: [] | [...Outer]) => void]
): void;
function f<Outer extends unknown[], Prefix extends unknown[]>(
    prefix: Prefix,
    source: (...args: Outer) => void,
) {
    take<Outer, Prefix>(...prefix, source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "only a union produced by substituting the callee's bare variadic is provisional"
    );
}

#[test]
fn provisional_aggregate_rest_does_not_cross_callable_properties() {
    let source = r#"
type Source<Outer extends unknown[]> = {
    (...args: Outer): void;
    callback: (...args: Outer) => void;
};
type Target<Values extends unknown[]> = {
    (...args: Values): void;
    callback: (...args: [] | [...Values]) => void;
};
declare function take<Values extends unknown[]>(
    ...args: [...Values, Target<Values>]
): void;
function f<Outer extends unknown[]>(
    prefix: Outer,
    source: Source<Outer>,
) {
    take(...prefix, source);
}
"#;
    assert_eq!(
        count(source, 2345),
        1,
        "a direct callable surface must not authorize its function-valued properties"
    );
}
