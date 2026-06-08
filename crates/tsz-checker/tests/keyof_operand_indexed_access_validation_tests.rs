//! Regression coverage: errors inside a `keyof` / `readonly` / `unique`
//! type-operator operand must be reported.
//!
//! The checker's recursive type-node validation (`check_type_node`) had no arm
//! for `TYPE_OPERATOR`, so the operand of `keyof <T>` (and `readonly`/`unique`)
//! fell through the catch-all and was never validated. Any diagnostic that lives
//! inside that operand — most visibly `TS2536` for an invalid indexed access
//! (`keyof A[K]` where `K` cannot index `A`) but also `TS2304` for an unresolved
//! name — was silently dropped. `tsc` validates the operand like any other nested
//! type node, so tsz must too.
//!
//! Structural rule: when a type node contains an indexed access `A[I]` where `I`
//! cannot index `A`, `tsc` reports `TS2536` at that node regardless of the
//! surrounding syntactic position (bare, under `keyof`, under another indexed
//! access, in a type-parameter constraint, or inside an array element). The bare
//! position already worked; this exercises the previously-dropped operator
//! positions.
//!
//! Witnessed while reducing the `intersectionsOfLargeUnions.ts` accepted
//! regression (the `HTMLElementTagNameMap[T][P]` / `keyof ElementTagNameMap[T]`
//! shapes), and verified against `tsc` 6.0.2.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_codes};

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

fn count(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|&c| c == code).count()
}

const MAPS: &str = "interface MapA { a: 1; b: 2; }\ninterface MapB { a: 1; }\n";

#[test]
fn keyof_invalid_indexed_access_in_alias_body_reports_ts2536() {
    // `K` ranges over `keyof MapA` ("a" | "b") but "b" is not a key of MapB,
    // so `MapB[K]` — and therefore `keyof MapB[K]` — is invalid.
    let src = format!("{MAPS}type T<K extends keyof MapA> = keyof MapB[K];");
    assert_eq!(
        count(&src, 2536),
        1,
        "keyof of an invalid indexed access must report exactly one TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn keyof_invalid_indexed_access_in_type_parameter_constraint_reports_ts2536() {
    // The invalid indexed access is inside a type-parameter constraint
    // (`P extends keyof MapB[K]`), a `keyof` operand position.
    let src = format!("{MAPS}type T<K extends keyof MapA, P extends keyof MapB[K]> = [K, P];");
    assert_eq!(
        count(&src, 2536),
        1,
        "keyof operand in a constraint must report TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn keyof_unresolved_name_in_operand_reports_ts2304() {
    // The operand recursion also surfaces an unresolved name that previously
    // hid inside the `keyof` operand.
    let src = "type T = keyof DoesNotExistAnywhere;";
    assert!(
        codes(src).contains(&2304),
        "unresolved name inside a keyof operand must report TS2304: {:?}",
        codes(src)
    );
}

#[test]
fn keyof_array_of_invalid_indexed_access_reports_ts2536() {
    // The operand is an array type whose element is the invalid indexed access:
    // the walker must recurse keyof -> array -> indexed access.
    let src = format!("{MAPS}type T<K extends keyof MapA> = keyof (MapB[K])[];");
    assert_eq!(
        count(&src, 2536),
        1,
        "keyof of an array of an invalid indexed access must report TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn bare_invalid_indexed_access_still_reports_single_ts2536() {
    // Guard: the previously-working bare position is unchanged (no double report).
    let src = format!("{MAPS}type T<K extends keyof MapA> = MapB[K];");
    assert_eq!(
        count(&src, 2536),
        1,
        "bare invalid indexed access keeps exactly one TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn keyof_of_valid_indexed_access_reports_nothing() {
    // Negative control: when `K` is constrained to `keyof MapB`, the index is
    // valid and no TS2536 must appear.
    let src = format!("{MAPS}type T<K extends keyof MapB> = keyof MapB[K];");
    assert!(
        !codes(&src).contains(&2536),
        "valid keyof operand must not report TS2536: {:?}",
        codes(&src)
    );
}

#[test]
fn keyof_operand_validation_is_binder_name_independent() {
    // Anti-hardcoding: the same structural error with renamed binders/params
    // must still report TS2536 — nothing keys off specific identifiers.
    let src = "interface Lookup { foo: 1; bar: 2; }\n\
               interface Narrow { foo: 1; }\n\
               type Pick<Key extends keyof Lookup> = keyof Narrow[Key];";
    assert_eq!(
        count(src, 2536),
        1,
        "renamed binders must still surface the keyof-operand TS2536: {:?}",
        codes(src)
    );
}
