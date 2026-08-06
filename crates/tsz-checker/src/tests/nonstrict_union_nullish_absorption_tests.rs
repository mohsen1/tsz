//! Regression tests for #16580: with `strictNullChecks` off, `null`/`undefined`
//! are absorbed out of *every* union, not just array-literal element unions.
//!
//! Structural rule: when `strictNullChecks` is off, `null` and `undefined` are
//! subtypes of every type, so tsc's `addTypeToUnion` never adds either to a
//! union's member set — it records only that one was seen, and `getUnionType`
//! falls back to `null` (preferred over `undefined`) when *nothing else*
//! remains. tsz applied that reduction only on the array-literal element path
//! (#16578, a checker-side post-pass); it now happens in the solver's union
//! construction itself, so annotations, aliases, return types, property types
//! and inferred unions all get it.
//!
//! Two constituents that look nullish but are not: `void` is not
//! `TypeFlags.Nullable` in tsc and must survive, and an *all-nullish* union has
//! no non-nullish sibling to absorb into, so it stays nullish rather than
//! collapsing to `never`.
//!
//! Each row reads the rendered source type out of a `TS2322` produced by
//! assigning the value to an incompatible type — the same methodology the issue
//! used against a pinned `typescript@7.0.2` oracle
//! (`--strict false --strictNullChecks false --target es2015`). Binder names are
//! varied across rows so no row can be satisfied by a name-shaped predicate.

use crate::test_utils::{check_with_options_code_messages, non_strict_checker_options};

fn nonstrict_messages(source: &str) -> Vec<(u32, String)> {
    check_with_options_code_messages(source, non_strict_checker_options())
}

/// Assign to `string` and read the rendered source type out of the `TS2322`.
fn assert_renders_as(source: &str, expected_type: &str, context: &str) {
    assert_renders_as_against(source, expected_type, "string", context);
}

/// The same, for rows whose own type is `string` and so need a different
/// incompatible target to provoke the `TS2322`.
fn assert_renders_as_against(source: &str, expected_type: &str, target: &str, context: &str) {
    let messages = nonstrict_messages(source);
    assert_eq!(
        messages,
        vec![(
            2322,
            format!("Type '{expected_type}' is not assignable to type '{target}'.")
        )],
        "{context}: {messages:?}"
    );
}

fn assert_clean(source: &str, context: &str) {
    let messages = nonstrict_messages(source);
    assert!(messages.is_empty(), "{context}: {messages:?}");
}

// ── The reported witness: a declared return type ──

/// #16580's repro. `declare function f(): number | null` returns `number` with
/// the flag off, so the `TS2322` names `number`, not `number | null`.
#[test]
fn declared_return_union_absorbs_null() {
    assert_renders_as(
        "\
declare function supply(): number | null;
var probe: string = supply();
",
        "number",
        "a declared return type absorbs its null constituent",
    );
}

#[test]
fn declared_return_union_absorbs_undefined() {
    assert_renders_as(
        "\
declare function fetchValue(): number | undefined;
var sink: string = fetchValue();
",
        "number",
        "a declared return type absorbs its undefined constituent",
    );
}

// ── Annotations, including the array-element form the issue measured as a1 ──

#[test]
fn variable_annotation_absorbs_null() {
    assert_renders_as(
        "\
declare var quantity: number | null;
var probe: string = quantity;
",
        "number",
        "a variable annotation absorbs its null constituent",
    );
}

/// The issue's a1: the annotation `(number | null)[]` renders `number[]`, so the
/// reduction has to happen inside the array's *element* type as constructed from
/// the annotation, not only on the array-literal inference path.
#[test]
fn array_annotation_element_union_absorbs_null() {
    assert_renders_as(
        "\
var readings: (number | null)[] = [1, null];
var probe: string = readings;
",
        "number[]",
        "an array annotation's element union absorbs its null constituent",
    );
}

/// A nullish constituent buried under an object type's property annotation.
#[test]
fn property_annotation_absorbs_undefined() {
    assert_renders_as(
        "\
declare var record: { tally: number | undefined };
var probe: string = record;
",
        "{ tally: number; }",
        "a property annotation absorbs its undefined constituent",
    );
}

// ── Aliases: the same root cause surfacing as a naming difference ──

/// The issue's a3. Once the union reduces to a single member there is no alias
/// left to print, so tsc renders `number`. This must come from the reduction,
/// not from teaching the printer to expand aliases.
#[test]
fn alias_of_union_with_undefined_renders_as_its_survivor() {
    assert_renders_as(
        "\
type Tally = number | undefined;
declare var counted: Tally;
var probe: string = counted;
",
        "number",
        "an alias whose union reduces to one member prints that member",
    );
}

/// The issue's a5: both nullish constituents at once.
#[test]
fn alias_of_union_with_both_nullish_renders_as_its_survivor() {
    assert_renders_as_against(
        "\
type Zqq = string | null | undefined;
declare var widget: Zqq;
var probe: number = widget;
",
        "string",
        "number",
        "an alias absorbs both nullish constituents",
    );
}

/// An alias that still has two survivors keeps its alias name — the reduction
/// removes the nullish member without disturbing alias display.
#[test]
fn alias_with_two_survivors_keeps_its_name() {
    assert_renders_as(
        "\
type Payload = number | boolean | null;
declare var carried: Payload;
var probe: string = carried;
",
        "Payload",
        "an alias with two survivors still prints as the alias",
    );
}

/// A generic alias instantiated with a concrete argument: the reduction happens
/// after instantiation, on the instantiated member list.
#[test]
fn generic_alias_instantiation_absorbs_null() {
    assert_renders_as(
        "\
type Slot<TValue> = TValue | null;
declare var held: Slot<number>;
var probe: string = held;
",
        "number",
        "a generic alias absorbs null after instantiation",
    );
}

// ── Negative controls: what must NOT be absorbed ──

/// The issue's a6, and the important negative: an all-nullish union has no
/// non-nullish sibling to absorb into. It must stay nullish — which in non-strict
/// mode is assignable to `string`, so the row is clean rather than a `TS2322`
/// naming `never`.
#[test]
fn all_nullish_union_does_not_collapse() {
    assert_clean(
        "\
declare var vacant: null | undefined;
var probe: string = vacant;
",
        "an all-nullish union keeps a nullish result",
    );
}

/// The alias form of the same negative.
#[test]
fn all_nullish_alias_does_not_collapse() {
    assert_clean(
        "\
type Absent = undefined | null;
declare var missing: Absent;
var probe: string = missing;
",
        "an all-nullish alias keeps a nullish result",
    );
}

// ── What the all-nullish union actually reduces *to* ──
//
// The rows above only establish that it stays nullish. They cannot tell a
// surviving `null | undefined` union from the scalar `null`, because both are
// assignable to everything with the flag off. The non-strict non-null arm keys
// on `type.flags & Nullable`, and a `Union`'s flags are `Union`, so it is the
// one observable that discriminates them.
//
// Oracle, `typescript@7.0.2`, `--strict false --strictNullChecks false`:
//   declare var a: null | undefined; a.foo;  -> TS18047 'a' is possibly 'null'.
//   declare var b: undefined | null; b.foo;  -> TS18047 'b' is possibly 'null'.
//   declare var d: undefined;        d.foo;  -> TS18048 'd' is possibly 'undefined'.
//
// Non-vacuity, stated honestly: these four rows pass *with or without* the
// solver-side collapse, because a directly-written annotation is also handled by
// #16593's syntactic union-type-node resolver. They are here to pin that the two
// paths agree. The rows that actually isolate the solver seam are
// `interface_index_signature_value_reports_the_reduced_single_cause` (the
// index-signature value is lowered by `tsz-lowering`, which no syntactic
// resolver sees) and the interner-level
// `nonstrict_all_nullish_union_collapses_to_the_scalar_null`; both fail with
// `crates/tsz-solver/src/intern/normalize.rs` reverted to its pre-fix state.

fn nonstrict_codes(source: &str) -> Vec<u32> {
    nonstrict_messages(source)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// `null | undefined` is the scalar `null`, so the non-null arm fires TS18047.
#[test]
fn all_nullish_union_reduces_to_the_scalar_null() {
    assert_eq!(
        nonstrict_codes("declare var vacant: null | undefined;\nvacant.foo;\n"),
        vec![18047],
        "an all-nullish union reduces to the scalar null"
    );
}

/// `null` wins on presence, not position — the reversed order is the same type.
#[test]
fn all_nullish_union_prefers_null_regardless_of_order() {
    assert_eq!(
        nonstrict_codes("declare var hollow: undefined | null;\nhollow.foo;\n"),
        vec![18047],
        "undefined | null still reduces to the scalar null"
    );
}

/// Through an alias, so the reduction is not a property of the annotation site.
#[test]
fn all_nullish_alias_reduces_to_the_scalar_null() {
    assert_eq!(
        nonstrict_codes("type Absent = undefined | null;\ndeclare var gone: Absent;\ngone.foo;\n"),
        vec![18047],
        "an all-nullish alias reduces to the scalar null"
    );
}

/// The discriminating negative: with no `null` among the members, `undefined`
/// is the survivor and the arm reports TS18048 instead.
#[test]
fn bare_undefined_stays_undefined() {
    assert_eq!(
        nonstrict_codes("declare var unset: undefined;\nunset.foo;\n"),
        vec![18048],
        "a bare undefined is not rewritten to null"
    );
}

/// `void` is not `TypeFlags.Nullable` in tsc and survives union construction
/// with the flag off. This is what separates the fix from a blanket
/// "drop anything nullable" rule. The `void | string` member order is tsc's own
/// (oracle-confirmed), not a display choice of this fix.
#[test]
fn void_constituent_survives() {
    assert_renders_as_against(
        "\
declare var outcome: string | void;
var probe: number = outcome;
",
        "void | string",
        "number",
        "void is not nullable and survives the reduction",
    );
}

/// `void` beside a nullish constituent: the nullish one goes, `void` stays, and
/// because two members survive the alias still has something to name.
#[test]
fn void_survives_beside_an_absorbed_null() {
    assert_renders_as_against(
        "\
type WithVoid = string | void | null;
declare var mixedv: WithVoid;
var probe: number = mixedv;
",
        "WithVoid",
        "number",
        "void survives while its null sibling is absorbed",
    );
}

/// A union with no nullish constituent at all is untouched.
#[test]
fn union_without_nullish_is_unchanged() {
    assert_renders_as(
        "\
declare var mixed: number | boolean;
var probe: string = mixed;
",
        "number | boolean",
        "a union with no nullish constituent is unchanged",
    );
}

/// A bare `null` annotation is not a union and is not reduced away.
#[test]
fn bare_null_annotation_is_untouched() {
    assert_clean(
        "\
declare var blank: null;
var probe: string = blank;
",
        "a bare null annotation stays null",
    );
}

// ── The array-literal path #16578 already owned still holds ──

/// Guards against the checker-side post-pass and the new solver-side reduction
/// disagreeing: the inferred element type still widens `"s"` to `string` before
/// the nullish sibling is absorbed.
#[test]
fn array_literal_element_union_still_widens_then_absorbs() {
    assert_renders_as(
        "\
var samples = [\"s\", undefined];
var probe: string = samples;
",
        "string[]",
        "the array-literal element union widens the literal, then absorbs undefined",
    );
}

// ── Unions the solver constructs, never written in source ──
//
// These are the shapes a checker-layer fix over syntactic union type nodes
// (#16593) structurally cannot reach: the union that survives conditional-type
// resolution, a mapped type's instantiated property type, and the union a
// function's return type is *inferred* from. All three are oracle-pinned to
// `number` against `typescript@7.0.2`.

/// The false branch of a resolved conditional still carries its nullish
/// constituent unless the reduction happens where the branch type is built.
#[test]
fn resolved_conditional_branch_absorbs_undefined() {
    assert_renders_as(
        "\
type Verdict<TIn> = TIn extends string ? string | null : number | undefined;
declare var settled: Verdict<number>;
var probe: string = settled;
",
        "number",
        "a resolved conditional branch absorbs its undefined constituent",
    );
}

/// A mapped type's property type is instantiated by the solver, not resolved
/// from the source union node.
#[test]
fn mapped_type_property_absorbs_null() {
    assert_renders_as(
        "\
type Lookup = { [K in \"a\"]: number | null };
declare var table: Lookup;
var probe: string = table.a;
",
        "number",
        "a mapped type's property type absorbs its null constituent",
    );
}

/// Still red, and **not** this seam's bug — pinned here so the distinction is
/// recorded rather than rediscovered.
///
/// tsz reports *nothing* for this row. If the inferred return type were
/// `number | null` the assignment would report `TS2322` naming `number | null`,
/// which the reduction would then fix. It reports nothing because `return null`
/// in non-strict mode widens to `any` before any union is built
/// (`widen_nullish_to_any_deep`, the #16384/#16393/#16396 widening-provenance
/// family), so the BCT over the return statements is `any` and no union with a
/// nullish constituent is ever constructed. There is nothing for union
/// construction to absorb.
///
/// Fixing it means the widening gate must not treat a bare `return null` beside
/// a non-nullish `return` as a widening source — a different owner from this
/// change, and a false negative rather than a display difference.
#[test]
#[ignore = "belongs to the non-strict nullish *widening* family (#16384/#16396), not union construction: `return null` widens to `any` before any union exists"]
fn inferred_return_union_absorbs_null() {
    assert_renders_as(
        "\
function tally() {
    if (1) {
        return 1;
    }
    return null;
}
var probe: string = tally();
",
        "number",
        "an inferred return union absorbs its null constituent",
    );
}
