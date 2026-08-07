//! Regression tests for the TS2322/TS2345 target-**intersection** elaboration.
//!
//! Structural rule (verified against `tsc` 6.0.2): when a source is related to a
//! target intersection `C1 & C2 & …`, `tsc` (`typeRelatedToEachType`) relates it
//! to each constituent in written order and elaborates the **first** failing
//! constituent — the top-level `Type 'S' is not assignable to type 'C1 & C2 &
//! …'.` headline is followed by `Type 'S' is not assignable to type 'Ci'.` one
//! level deeper, then that constituent's own (path-compressed) drill.
//!
//! tsz previously evaluated the intersection target into a single merged object
//! before building the failure reason, so the chain skipped straight to the
//! merged property mismatch and dropped the constituent frame that explains
//! which member of the intersection requires the failing shape. The fix
//! reconstructs the constituent frame at the assignability gateway
//! (`analyze_assignability_failure` -> `IntersectionTargetMismatch`) from the
//! original (pre-evaluation) intersection, so it applies regardless of how the
//! intersection is spelled. See the diagnostics family tracker (#12179); this is
//! the dual of the intersection-*source* fix in #10962.

use crate::test_utils::check_source_diagnostics;

/// Collect a single diagnostic's full elaboration text (main message plus all
/// related-information lines, joined by newlines) for the given code.
fn elaboration(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS{code}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    let mut lines = vec![matching[0].message_text.clone()];
    lines.extend(
        matching[0]
            .related_information
            .iter()
            .map(|info| info.message_text.clone()),
    );
    lines.join("\n")
}

/// A two-object intersection target with a failing first constituent emits the
/// constituent frame (`Type 'S' is not assignable to type '{ x: number; }'.`)
/// between the intersection headline and the property drill.
#[test]
fn anonymous_object_intersection_emits_first_constituent_frame() {
    let text = elaboration(
        r#"
declare let a: { x: number } & { y: string };
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; } & { y: string; }'"),
        "Expected the intersection headline. Got: {text:?}"
    );
    assert!(
        text.contains(
            "Type '{ x: string; y: string; }' is not assignable to type '{ x: number; }'."
        ),
        "Expected the failing constituent frame. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the constituent's property drill. Got: {text:?}"
    );
}

/// The elaboration reports the **first** failing constituent in written order:
/// when the second constituent is the one that fails, its frame is emitted, not
/// the first's.
#[test]
fn reports_failing_constituent_in_written_order() {
    let text = elaboration(
        r#"
declare let a: { x: number } & { y: string };
declare let b: { x: number; y: number };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ y: string; }'."),
        "Expected the second constituent frame (the one that fails). Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'y' are incompatible."),
        "Expected the failing property to be 'y'. Got: {text:?}"
    );
}

/// Anti-hardcoding cover: the rule is structural, not tied to a spelling. An
/// interface intersection (`P & Q`) — which stays an intersection rather than
/// being merged at construction — produces the same constituent frame, and the
/// frame names the interface (`P`), not the merged shape.
#[test]
fn interface_intersection_names_the_constituent_interface() {
    let text = elaboration(
        r#"
interface Alpha { x: number }
interface Beta { y: string }
declare let a: Alpha & Beta;
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Alpha'."),
        "Expected the constituent frame to name the interface 'Alpha'. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the property drill beneath the constituent frame. Got: {text:?}"
    );
}

/// A non-generic type alias for the intersection keeps its alias spelling in the
/// headline (`T`) while the constituent frame renders the structural
/// constituent — matching tsc's `aliasSymbol` policy.
#[test]
fn aliased_intersection_keeps_alias_in_headline_structural_constituent() {
    let text = elaboration(
        r#"
type Combined = { x: number } & { y: string };
declare let a: Combined;
declare let b: { x: string; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Combined'."),
        "Expected the alias name in the headline. Got: {text:?}"
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; }'."),
        "Expected the structural constituent in the frame. Got: {text:?}"
    );
}

/// A failing constituent whose property mismatch is itself a single-property
/// chain keeps tsc's path-compressed drill (`The types of 'x.p' are
/// incompatible between these types.`) beneath the constituent frame, rather
/// than re-expanding it into nested `Types of property` lines.
#[test]
fn constituent_drill_preserves_dotted_path_compression() {
    let text = elaboration(
        r#"
declare let a: { x: { p: number } } & { y: string };
declare let b: { x: { p: string }; y: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ x: { p: number; }; }'."),
        "Expected the constituent frame. Got: {text:?}"
    );
    assert!(
        text.contains("The types of 'x.p' are incompatible between these types."),
        "Expected the path-compressed drill. Got: {text:?}"
    );
    assert!(
        !text.contains("Types of property 'x' are incompatible."),
        "Compressed chain must not re-expand the leading property. Got: {text:?}"
    );
}

/// The elaboration applies in argument position (TS2345) as well, since both
/// flow through the shared assignability gateway. A declared-variable argument
/// (not an inline object literal, which is checked property-wise) exercises the
/// whole-argument TS2345 path.
#[test]
fn argument_position_intersection_emits_constituent_frame() {
    let text = elaboration(
        r#"
declare function consume(p: { x: number } & { y: string }): void;
declare const arg: { x: string; y: string };
consume(arg);
"#,
        2345,
    );
    assert!(
        text.contains("is not assignable to type '{ x: number; }'."),
        "Expected the constituent frame in the argument elaboration. Got: {text:?}"
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the property drill in the argument elaboration. Got: {text:?}"
    );
}

/// A branded primitive intersection (`string & { __brand }`) collapses to the
/// constituent frame alone: the source fails the object constituent, and there
/// is no deeper structural drill.
#[test]
fn branded_primitive_intersection_frame_stands_alone() {
    let text = elaboration(
        r#"
type Tagged = string & { __tag: 1 };
declare let a: Tagged;
declare let b: string;
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Tagged'."),
        "Expected the alias headline. Got: {text:?}"
    );
    assert!(
        text.contains("is not assignable to type '{ __tag: 1; }'."),
        "Expected the object-constituent frame. Got: {text:?}"
    );
}

/// A **primitive** source (`number`) failing an object-only intersection target
/// still gets the first-constituent frame. tsz merges `{ … } & { … }` into a
/// single object before building the reason, and a primitive source against that
/// merged object yields a missing/no-common-property reason — which the object
/// source path (`TS2739`/`TS2741`) owns. But a primitive has no properties to
/// enumerate, so `tsc` never emits that missing-property line; it falls back to
/// `typeRelatedToEachType`'s per-constituent frame (`Type 'number' is not
/// assignable to type '{ alpha: number; }'.`). Previously tsz dropped it and
/// left only the headline.
#[test]
fn primitive_source_to_object_intersection_emits_first_constituent_frame() {
    let text = elaboration(
        r#"
declare let a: { alpha: number } & { beta: string };
declare let b: number;
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type '{ alpha: number; } & { beta: string; }'."),
        "Expected the intersection headline. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ alpha: number; }'."),
        "Expected the first-constituent frame for the primitive source. Got: {text:?}"
    );
}

/// The same fallback holds when the constituents are named interfaces: the frame
/// names the failing interface (`Left`), matching `tsc`. Binder names are varied
/// so the outcome tracks the structural shape, not a fixed spelling.
#[test]
fn primitive_source_names_first_interface_constituent() {
    let text = elaboration(
        r#"
interface Left { alpha: number }
interface Right { beta: string }
declare let a: Left & Right;
declare let b: string;
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'Left & Right'."),
        "Expected the interface-intersection headline. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'Left'."),
        "Expected the first-constituent frame naming the interface. Got: {text:?}"
    );
}

/// A `unique`-free literal source is a primitive too; the frame reports the
/// widened operand exactly as `tsc` does inside the constituent line.
#[test]
fn literal_source_to_object_intersection_emits_constituent_frame() {
    let text = elaboration(
        r#"
declare let a: { alpha: number } & { beta: string };
declare const b: 5;
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ alpha: number; }'."),
        "Expected the widened constituent frame for the literal source. Got: {text:?}"
    );
}

/// When the first failing constituent (in written order) fails because the
/// source is *missing* a property it requires, the elaboration folds to the
/// `Property 'p' is missing in type 'S' but required in type 'Ci'.` line (which
/// already names the constituent) with no extra `Type 'S' is not assignable to
/// type 'Ci'.` frame — even though the merged top-level reason is a different
/// (property-type) mismatch. This exercises the missing-property fold reached
/// via the per-constituent inner reason.
#[test]
fn first_failing_constituent_missing_property_folds() {
    let text = elaboration(
        r#"
declare let a: { y: string } & { x: number };
declare let b: { x: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains(
            "Property 'y' is missing in type '{ x: string; }' but required in type '{ y: string; }'."
        ),
        "Expected the folded missing-property line naming the first constituent. Got: {text:?}"
    );
    assert!(
        !text.contains("is not assignable to type '{ y: string; }'."),
        "Missing-property fold must not also emit a constituent frame. Got: {text:?}"
    );
}

/// Control: a non-intersection target is unaffected — the chain stays the plain
/// `Type 'S' is not assignable to type 'T'.` + property drill with no spurious
/// constituent frame.
#[test]
fn non_intersection_target_is_unchanged() {
    let text = elaboration(
        r#"
declare let a: { x: number };
declare let b: { x: string };
a = b;
"#,
        2322,
    );
    assert!(
        text.contains("Types of property 'x' are incompatible."),
        "Expected the plain property drill. Got: {text:?}"
    );
    // Exactly one `is not assignable to type` line (the headline); no extra
    // constituent frame for a single-object target.
    assert_eq!(
        text.matches("is not assignable to type").count(),
        2,
        "Non-intersection target must not gain a constituent frame. Got: {text:?}"
    );
}

// ===========================================================================
// Object intersections that reach the relation as a *merged* object.
//
// tsz eagerly merges an anonymous object-only intersection (`{ a } & { b }`)
// into a single object for O(1) member lookup. When that intersection is
// produced through a generic instantiation, type alias, conditional, or `infer`
// capture — rather than written inline — the merged object reaches the
// diagnostic with no structural `Intersection` to key on, and tsz previously
// collapsed the missing-property failure to a flat single-object `TS2741`. tsc
// keeps `A & B` an intersection in every spelling and reports the top-level
// `TS2322` with a member-by-member elaboration. The `INTERSECTION_MERGED` shape
// flag plus the `merged_intersection_origin` provenance let the renderer
// recover the intersection regardless of how it was constructed. Binder names
// are varied so the outcome tracks the structural shape, not any spelling.
// (Verified against `tsc` 6.0.2, `--noEmit --strict`.)
// ===========================================================================

/// Returns the sorted set of diagnostic codes emitted for `source`.
fn diagnostic_codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

/// A single missing property against an intersection produced by *instantiating*
/// a generic alias (`Combine<S> = S & { tag }`) is the top-level `TS2322` with
/// the failing-member elaboration, exactly as for a written `S & { tag }`.
#[test]
fn instantiated_generic_object_intersection_target_reports_ts2322() {
    let text = elaboration(
        r#"
type Combine<Slot> = Slot & { secondField: number };
type Combined = Combine<{ firstField: string }>;
const value: Combined = { firstField: "x" };
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type"),
        "Expected the top-level TS2322 headline. Got: {text:?}"
    );
    assert!(
        text.contains(
            "Property 'secondField' is missing in type '{ firstField: string; }' but required in type '{ secondField: number; }'."
        ),
        "Expected the failing-member elaboration naming the second constituent. Got: {text:?}"
    );
}

/// The same intersection routed through an *identity alias* (`Identity<T> = T`)
/// — so the only structural form tsz ever holds is the merged object — still
/// recovers the intersection target. The constituent member order is preserved.
#[test]
fn identity_aliased_object_intersection_target_reports_ts2322() {
    let codes = diagnostic_codes(
        r#"
type Echo<Payload> = Payload;
type Pair = Echo<{ alpha: 1 } & { beta: 2 }>;
const value: Pair = { alpha: 1 };
"#,
    );
    assert_eq!(codes, vec![2322], "Expected only TS2322. Got: {codes:?}");
}

/// A conditional type whose branch is the intersection (`Wrap<T> = T extends
/// object ? T & { brand } : never`) is covered too — the conditional evaluates
/// to the merged object before the relation runs.
#[test]
fn conditional_object_intersection_target_reports_ts2322() {
    let codes = diagnostic_codes(
        r#"
type Decorate<Input> = Input extends object ? Input & { decorated: true } : never;
type Decorated = Decorate<{ base: number }>;
const value: Decorated = { base: 1 };
"#,
    );
    assert_eq!(codes, vec![2322], "Expected only TS2322. Got: {codes:?}");
}

/// An `infer` capture of an intersection from a contravariant position
/// (the `UnionToIntersection` idiom) reaches the relation as the merged object
/// and must still report `TS2322`, not `TS2741`.
#[test]
fn infer_captured_object_intersection_target_reports_ts2322() {
    let codes = diagnostic_codes(
        r#"
type Merge<Variants> =
    (Variants extends unknown ? (arg: Variants) => void : never) extends
        (arg: infer Merged) => void ? Merged : never;
type Combined = Merge<{ left: 1 } | { right: 2 }>;
const value: Combined = { left: 1 };
"#,
    );
    assert_eq!(codes, vec![2322], "Expected only TS2322. Got: {codes:?}");
}

/// Multiple missing properties against an instantiated intersection produce the
/// plural `TS2739` form embedded under the top-level `TS2322`, listing the
/// constituent that requires them.
#[test]
fn instantiated_intersection_multiple_missing_reports_ts2322_plural() {
    let text = elaboration(
        r#"
type Extend<Seed> = Seed & { needB: number; needC: string };
type Extended = Extend<{ haveA: boolean }>;
const value: Extended = { haveA: true };
"#,
        2322,
    );
    assert!(
        text.contains("is missing the following properties from type '{ needB: number; needC: string; }': needB, needC"),
        "Expected the plural missing-properties elaboration. Got: {text:?}"
    );
}

/// The same instantiated intersection in *argument* position keeps `TS2345`
/// (tsc never downgrades an argument intersection mismatch to `TS2322`), proving
/// the recovery is position-aware rather than a blanket rewrite.
#[test]
fn instantiated_intersection_argument_position_stays_ts2345() {
    let codes = diagnostic_codes(
        r#"
type Combine<Slot> = Slot & { extra: number };
declare function take(value: Combine<{ given: string }>): void;
take({ given: "x" });
"#,
    );
    assert_eq!(codes, vec![2345], "Expected only TS2345. Got: {codes:?}");
}

/// Control: a generic alias that instantiates to a *plain* object (no
/// intersection) must keep the single-object `TS2741`. A plain object literal
/// interns to the same shape as a merged intersection of that shape, so this
/// guards that the `INTERSECTION_MERGED` flag — not the shape alone — drives the
/// recovery.
#[test]
fn instantiated_plain_object_target_keeps_ts2741() {
    let codes = diagnostic_codes(
        r#"
type Shape<Value> = { present: Value; alsoNeeded: number };
type Concrete = Shape<string>;
const value: Concrete = { present: "x" };
"#,
    );
    assert_eq!(codes, vec![2741], "Expected only TS2741. Got: {codes:?}");
}

/// Control: a nominal class-instance intersection (`Derived & Base`) is not
/// stamped `INTERSECTION_MERGED` — its members carry binder symbols, so the
/// merge follows nominal subtyping. tsc elaborates the failing member by its
/// nominal name (`required in type 'Dog'`), and tsz matches byte-for-byte.
#[test]
fn nominal_class_instance_intersection_elaborates_by_nominal_name() {
    let text = elaboration(
        r#"
class Animal { species = "a"; }
class Dog extends Animal { breed = "b"; }
type Both = Dog & Animal;
const value: Both = new Animal();
"#,
        2322,
    );
    assert!(
        text.contains("Property 'breed' is missing in type 'Animal' but required in type 'Dog'."),
        "Nominal class intersection must elaborate against the nominal member name 'Dog'. Got: {text:?}"
    );
}

// ===========================================================================
// Object-first mixed intersections whose merged object is not the LAST
// member in source order (`{ z: 1 } & [tuple]`, not `[tuple] & { z: 1 }`).
//
// `normalize_intersection`'s member-list rebuild (`crates/tsz-solver/src/
// intern/intersection.rs`) used to special-case the "no merged callable"
// path: it appended the merged object *after* every other remaining member
// unconditionally, instead of substituting it at its original position like
// the callable-merge path already did. A tuple (or any other non-object,
// non-callable member — primitive, array, ...) that appeared *after* the
// object in source order was silently promoted ahead of it in the interned
// `Intersection`'s member list. The written-order elaboration
// (`IntersectionTargetMismatch` in `crates/tsz-checker/src/assignability/
// assignability_diagnostics.rs`) then walked that reordered list and named
// the wrong "first failing constituent" — the tuple instead of tsc's object.
// Reversing the operands (`[tuple] & { z: 1 }`) happened to keep the correct
// order by coincidence (the object was already last), which is why the bug
// was order-sensitive rather than a blanket display defect. (Verified
// against `tsc` 7.0.2, `--noEmit --strict`.)
// ===========================================================================

/// The exact #16753 repro: an object-first intersection with a tuple member
/// (via a nested spread) elaborates the object, matching tsc.
#[test]
fn object_first_tuple_intersection_names_object_constituent() {
    let text = elaboration(
        r#"
type B = { z: 1 } & [string, ...[number, boolean]];
const b: B = 1;
"#,
        2322,
    );
    assert!(
        text.contains("is not assignable to type 'B'."),
        "Expected the alias headline. Got: {text:?}"
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ z: 1; }'."),
        "Expected the object constituent frame (tsc names the object, not the tuple). Got: {text:?}"
    );
    assert!(
        !text.contains("[string, number, boolean]"),
        "Must not name the tuple constituent when the object is written first. Got: {text:?}"
    );
}

/// Control: with the tuple written first, tsc (and tsz, unaffected by this
/// bug) names the tuple — both compilers already agreed here.
#[test]
fn tuple_first_object_intersection_names_tuple_constituent() {
    let text = elaboration(
        r#"
type B = [string, ...[number, boolean]] & { z: 1 };
const b: B = 1;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '[string, number, boolean]'."),
        "Expected the tuple constituent frame. Got: {text:?}"
    );
}

/// The bug is not specific to a spread-flattened tuple: a plain
/// no-spread tuple reproduces the same object-first reordering.
#[test]
fn object_first_plain_tuple_intersection_names_object_constituent() {
    let text = elaboration(
        r#"
type B = { z: 1 } & [string, number];
const b: B = 1;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ z: 1; }'."),
        "Expected the object constituent frame. Got: {text:?}"
    );
}

/// Three-way intersection: the first-written object constituent still wins
/// over a trailing tuple, not just a two-member intersection.
#[test]
fn three_way_object_first_intersection_names_first_object_constituent() {
    let text = elaboration(
        r#"
type B = { z: 1 } & { w: 2 } & [string, number];
const b: B = 1;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ z: 1; }'."),
        "Expected the first-written object constituent frame. Got: {text:?}"
    );
}

/// The same reordering bug applies to any non-callable, non-object member —
/// not just tuples. An array member after the object in source order must
/// not be promoted ahead of it either.
#[test]
fn object_first_array_intersection_names_object_constituent() {
    let text = elaboration(
        r#"
type B = { z: 1 } & string[];
const b: B = 1;
"#,
        2322,
    );
    assert!(
        text.contains("Type 'number' is not assignable to type '{ z: 1; }'."),
        "Expected the object constituent frame. Got: {text:?}"
    );
}

/// Control: a bare primitive first (no object member at all — nothing for
/// the merge step to reorder) already kept source order before this fix and
/// must remain unaffected by it.
#[test]
fn primitive_first_tuple_intersection_names_primitive_constituent() {
    let text = elaboration(
        r#"
type B = number & [string, number];
const b: B = "x";
"#,
        2322,
    );
    assert!(
        text.contains("Type 'string' is not assignable to type 'number'."),
        "Expected the first-written primitive constituent frame. Got: {text:?}"
    );
}

/// Renamed-binder control: the rule is structural, not keyed on the
/// property/tuple-element spelling.
#[test]
fn renamed_object_first_tuple_intersection_names_object_constituent() {
    let text = elaboration(
        r#"
type Widget = { kind: "circle" } & [boolean, ...[bigint]];
const w: Widget = 1;
"#,
        2322,
    );
    assert!(
        text.contains(r#"Type 'number' is not assignable to type '{ kind: "circle"; }'."#),
        "Expected the renamed object constituent frame. Got: {text:?}"
    );
}
