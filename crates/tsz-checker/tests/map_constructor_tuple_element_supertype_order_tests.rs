//! Regression tests for issue #17364: the covariant common-supertype winner
//! for a type parameter fed from a *tuple element* position of an array
//! literal — the `V` in `new Map([["", true], ["", 0]])`.
//!
//! Structural rule: when *every* inference candidate for a type parameter came
//! from an array-literal element position and the candidates are genuinely
//! disjoint bare primitive intrinsics, `tsc`'s `getCommonSupertype` runs its
//! reduceLeft tournament over `getUnionType(candidates)`. The candidates are
//! therefore visited in ascending intrinsic type-id order — `string < number <
//! bigint < symbol < boolean` (`boolean` is the `false | true` union, minted
//! after the simple intrinsics) — and, because none is a subtype of another,
//! the lowest-id member wins. The winner is thus *order-independent*: `string`
//! beats `number` and `boolean`; `number` beats `boolean`; regardless of the
//! source order of the array elements.
//!
//! tsz previously kept the *source-first* candidate, so
//! `new Map([["", true], ["", 0]])` collapsed `V` to `boolean` and anchored the
//! TS2769 last-overload chain at the `0` element instead of tsc's `V = number`
//! anchored at the `true` element (the `for-of39.ts` conformance witness).
//!
//! These tests observe the inferred `V` through a plain generic function
//! `firstOf<K, V>(pairs: [K, V][]): V` — no lib or constructor overload needed,
//! so the winner is visible directly as the return type and its downstream
//! assignability. Each fixture is oracle-verified against `typescript@7.0.2`
//! under `--strict` (byte-identical codes, locations, and messages).
//!
//! Owner layer: `crates/tsz-solver/src/inference/infer_bct.rs`
//! (`get_common_supertype_for_inference`, the all-from-array-element ranked
//! winner). The mixed array+naked case (#9667) keeps the leftmost-array
//! first-wins and is guarded by `mixed_array_and_naked_candidate_keeps_array_leftmost`.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_line_column, strict_checker_options,
};

/// Diagnostics for `source` under strict mode with no libs (these fixtures are
/// self-contained `declare`d generics, so no lib types are required).
fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_with_libs(source, "test.ts", strict_checker_options(), &[])
}

/// `(code, line, column)` triples for every diagnostic, 1-indexed and sorted by
/// `(line, column, code)` so assertions do not depend on emission order.
fn coded_anchors(source: &str) -> Vec<(u32, u32, u32)> {
    let mut anchors: Vec<(u32, u32, u32)> = diagnostics(source)
        .iter()
        .map(|d| {
            let (line, column) = diagnostic_line_column(source, d);
            (d.code, line, column)
        })
        .collect();
    anchors.sort_by_key(|&(code, line, column)| (line, column, code));
    anchors
}

const DECL: &str = "declare function firstOf<K, V>(pairs: [K, V][]): V;\n";

// --- number beats boolean, order-independent (the for-of39 family) ---------

#[test]
fn boolean_then_number_pins_v_to_number() {
    // `V = number`, so assigning the result to `number` is sound; the array
    // argument's `true` leg is what fails (`boolean` not assignable to
    // `number`) at col 31 — matching tsc's `V = number` selection.
    let source = format!("{DECL}var r: number = firstOf([[\"\", true], [\"\", 0]]);\n");
    assert_eq!(
        coded_anchors(&source),
        vec![(2322, 2, 31)],
        "V must be number (not source-first boolean): only the `true` array leg fails"
    );
}

#[test]
fn number_then_boolean_still_pins_v_to_number_order_independent() {
    // Swapped source order — winner unchanged (number < boolean by intrinsic id).
    let source = format!("{DECL}var r: number = firstOf([[\"\", 0], [\"\", true]]);\n");
    assert_eq!(
        coded_anchors(&source),
        vec![(2322, 2, 40)],
        "boolean loses to number regardless of source order"
    );
}

#[test]
fn boolean_first_result_is_not_assignable_to_boolean() {
    // The complementary witness: `V = number`, so the whole result is *not*
    // assignable to `boolean` — a TS2322 on the binding in addition to the
    // failing array leg. If tsz had kept `V = boolean` this binding error would
    // vanish, so it pins the winner from the other side.
    let source = format!("{DECL}var r: boolean = firstOf([[\"\", true], [\"\", 0]]);\n");
    assert_eq!(
        coded_anchors(&source),
        vec![(2322, 2, 5), (2322, 2, 32)],
        "V = number: the binding `r: boolean` fails and the `true` leg fails"
    );
}

// --- string beats both number and boolean ----------------------------------

#[test]
fn string_then_number_pins_v_to_string() {
    let source = format!("{DECL}var r: string = firstOf([[\"\", \"x\"], [\"\", 0]]);\n");
    assert_eq!(
        coded_anchors(&source),
        vec![(2322, 2, 42)],
        "string beats number: only the `0` leg fails"
    );
}

#[test]
fn number_then_string_pins_v_to_string_order_independent() {
    let source = format!("{DECL}var r: number = firstOf([[\"\", 0], [\"\", \"x\"]]);\n");
    assert_eq!(
        coded_anchors(&source),
        vec![(2322, 2, 5), (2322, 2, 31)],
        "string wins regardless of source order: binding `r: number` fails and the `0` leg fails"
    );
}

// --- Negative controls: the fix must stay scoped ---------------------------

#[test]
fn homogeneous_entries_infer_cleanly() {
    // No disjoint candidates → no tournament → the return type is exactly the
    // element type, assignable to its own annotation. Distinct binder name to
    // keep the rule structural, not name-keyed.
    let source = format!("{DECL}var homogeneous: number = firstOf([[\"\", 0], [\"\", 1]]);\n");
    assert_eq!(
        coded_anchors(&source),
        Vec::<(u32, u32, u32)>::new(),
        "a homogeneous number column must infer V = number and assign cleanly"
    );
}

#[test]
fn single_array_param_still_unions_disjoint_primitives() {
    // A single `T[]` parameter (naked element, not a tuple leg) uses tsc's
    // union inference, not the common-supertype tournament: `T = number |
    // boolean`. Guards that the ranked path stays scoped to the tuple-element
    // supertype case and does not collapse this union to a single winner.
    let source = "declare function only<T>(xs: T[]): T;\nvar u: boolean = only([true, 0]);\n";
    assert_eq!(
        coded_anchors(source),
        vec![(2322, 2, 5)],
        "T must stay `number | boolean` (union), failing to assign to boolean as a whole"
    );
}

#[test]
fn mixed_array_and_naked_candidate_keeps_array_leftmost() {
    // #9667: one candidate from the `T[]` array position (`boolean`), one from a
    // naked `T` argument (`number`). tsc keeps the leftmost *array* candidate
    // (`boolean`) rather than id-sorting, so the naked `0` argument is what
    // fails (TS2345). The all-from-array ranked path must NOT fire here.
    let source = "declare function pick<T>(xs: T[], y: T): T;\nvar r = pick([true], 0);\n";
    assert_eq!(
        coded_anchors(source),
        vec![(2345, 2, 22)],
        "mixed array+naked keeps the array candidate (boolean); the naked `0` fails"
    );
}

#[test]
fn mixed_number_array_string_naked_keeps_number_array_candidate() {
    // Complementary mixed witness where the *array* candidate has the higher
    // TS7 rank than the naked one (`number`-array vs `string`-naked). An
    // `any`-from-array id-sort would flip `T` to `string` and reject the array
    // elements with TS2322; tsc keeps `T = number` (the array candidate) and
    // reports only the naked `"a"` argument with TS2345.
    let source = "declare function f<T>(a: T[], b: T): void;\nf([1, 2], \"a\");\n";
    assert_eq!(
        coded_anchors(source),
        vec![(2345, 2, 11)],
        "array candidate `number` must win; only the naked string argument fails"
    );
}
