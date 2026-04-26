//! Regression test for TS2862 false positive on generic `keyof T` index writes.
//!
//! `is_broad_index_type` previously evaluated `keyof T` through T's constraint
//! and ended up classifying it as a broad index type. The keyof-inner-type
//! guard meant to keep `keyof T` non-broad never fired because the eager
//! `evaluate_type_with_env` short-circuit returned first. Mirrors
//! `keyofAndIndexedAccessErrors.ts`'s `Repro from #51069` snippet.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn ts2862_not_emitted_for_keyof_t_indexed_compound_assignment() {
    // `T extends Record<string, number>`; `key: keyof T`. tsc emits only
    // TS2322 here ("Type 'number' is not assignable to type 'T[keyof T]'");
    // tsz must not also emit TS2862, which is reserved for the broad-index
    // write classification (e.g. `T[string]`).
    let source = r#"
class Test<T extends Record<string, number>> {
    testy: T;
    constructor(t: T) { this.testy = t; }
    public t(key: keyof T): number {
        this.testy[key] += 1;
        return this.testy[key];
    }
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2862),
        "TS2862 must not fire for `T[keyof T]` compound writes; got {codes:?}"
    );
}
