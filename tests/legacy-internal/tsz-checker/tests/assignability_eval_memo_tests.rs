//! Tests for the stamp-guarded `evaluate_type_for_assignability` result memo
//! (issue #8356).
//!
//! The memo must be semantically invisible: repeated constraint validation of
//! the same types may skip recomputation, but diagnostics (including repeated
//! TS2344 at distinct sites) and recursive-alias evaluation results must be
//! byte-identical to the unmemoized pipeline. Entries are dropped whenever the
//! session stamp (type-environment generations + symbol-type cache versions)
//! moves, so a hit can never observe a stale environment.

use crate::context::{AssignabilityEvalMemo, SymbolTypeCache};
use crate::test_utils::check_source_codes;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

// ---------------------------------------------------------------------------
// Memo container semantics
// ---------------------------------------------------------------------------

#[test]
fn memo_serves_entries_under_unchanged_stamp() {
    let mut memo = AssignabilityEvalMemo::default();
    let stamp = (1, 1, 0, 0);
    memo.insert(stamp, TypeId::STRING, TypeId::NUMBER);
    assert_eq!(memo.get(stamp, TypeId::STRING), Some(TypeId::NUMBER));
}

#[test]
fn memo_drops_entries_when_any_stamp_component_moves() {
    for moved in [(2, 1, 0, 0), (1, 2, 0, 0), (1, 1, 1, 0), (1, 1, 0, 1)] {
        let mut memo = AssignabilityEvalMemo::default();
        memo.insert((1, 1, 0, 0), TypeId::STRING, TypeId::NUMBER);
        assert_eq!(
            memo.get(moved, TypeId::STRING),
            None,
            "stamp {moved:?} must invalidate"
        );
        // The memo re-stamps on the miss; fresh entries are valid again.
        memo.insert(moved, TypeId::STRING, TypeId::BOOLEAN);
        assert_eq!(memo.get(moved, TypeId::STRING), Some(TypeId::BOOLEAN));
    }
}

#[test]
fn symbol_type_cache_version_tracks_mutations_not_reads() {
    let cache = SymbolTypeCache::new();
    let v0 = cache.version();
    assert!(cache.get(&SymbolId(7)).is_none());
    assert_eq!(cache.version(), v0, "reads must not bump the version");

    cache.insert(SymbolId(7), TypeId::STRING);
    let v1 = cache.version();
    assert!(v1 > v0, "insert must bump the version");

    // entry_or_insert on an existing key changes nothing observable.
    cache.entry_or_insert(SymbolId(7), TypeId::NUMBER);
    assert_eq!(cache.version(), v1);

    cache.entry_or_insert(SymbolId(8), TypeId::NUMBER);
    assert!(cache.version() > v1, "new-key entry_or_insert must bump");

    let v2 = cache.version();
    assert!(cache.remove(&SymbolId(99)).is_none());
    assert_eq!(cache.version(), v2, "absent remove must not bump");
    cache.remove(&SymbolId(7));
    assert!(cache.version() > v2, "present remove must bump");
}

// ---------------------------------------------------------------------------
// Recursive type-level iteration (the ts-toolbelt loop idiom) stays clean
// under heavy repeated constraint validation.
// ---------------------------------------------------------------------------

#[test]
fn recursive_indexed_object_map_loop_alias_stays_clean() {
    let codes = check_source_codes(
        r#"
type Next<I extends number[]> = [...I, 0];
type Loop<N extends number, I extends number[] = []> = {
    0: Loop<N, Next<I>>;
    1: I["length"];
}[I["length"] extends N ? 1 : 0];
type Three = Loop<3>;
const ok: 3 = null as unknown as Three;
"#,
    );
    assert!(codes.is_empty(), "expected clean check, got: {codes:?}");
}

#[test]
fn recursive_indexed_object_map_loop_alias_stays_clean_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Step<Acc extends number[]> = [...Acc, 0];
type Iterate<Target extends number, Acc extends number[] = []> = {
    0: Iterate<Target, Step<Acc>>;
    1: Acc["length"];
}[Acc["length"] extends Target ? 1 : 0];
type Result = Iterate<2>;
const ok: 2 = null as unknown as Result;
"#,
    );
    assert!(codes.is_empty(), "expected clean check, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Repeated violations must each still report: a memoized evaluation result
// must not swallow per-site TS2344 diagnostics.
// ---------------------------------------------------------------------------

#[test]
fn repeated_constraint_violations_report_at_every_site() {
    let codes = check_source_codes(
        r#"
type OnlyString<S extends string> = S;
type BadA = OnlyString<number>;
type BadB = OnlyString<number>;
type BadC = OnlyString<{ value: number }>;
"#,
    );
    let ts2344 = codes.iter().filter(|&&code| code == 2344).count();
    assert_eq!(
        ts2344, 3,
        "expected TS2344 at all three sites, got: {codes:?}"
    );
}

#[test]
fn repeated_constraint_violations_report_at_every_site_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Keyish<Name extends string> = Name;
type FirstUse = Keyish<42>;
type SecondUse = Keyish<42>;
"#,
    );
    let ts2344 = codes.iter().filter(|&&code| code == 2344).count();
    assert_eq!(ts2344, 2, "expected TS2344 at both sites, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Mixed valid/invalid repeats of the same generic target: the memoized result
// for the valid argument must not leak onto the invalid one or vice versa.
// ---------------------------------------------------------------------------

#[test]
fn valid_and_invalid_arguments_keep_independent_outcomes() {
    let codes = check_source_codes(
        r#"
type Pick1<S extends string> = S;
type Good = Pick1<"a">;
type Bad = Pick1<1>;
type GoodAgain = Pick1<"a">;
type BadAgain = Pick1<1>;
"#,
    );
    let ts2344 = codes.iter().filter(|&&code| code == 2344).count();
    assert_eq!(
        ts2344, 2,
        "expected TS2344 exactly at the two invalid sites, got: {codes:?}"
    );
}
