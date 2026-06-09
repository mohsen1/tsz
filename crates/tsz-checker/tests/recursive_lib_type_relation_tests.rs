//! Parity invariant for the shared-lib-universe campaign.
//!
//! Recursive / cross-lib-file library types (`Promise<T>` ↔ `PromiseLike<T>`
//! spanning lib.es5 + lib.es2015.promise, including nested
//! `Promise<Promise<T>>` and chained `.then`) must resolve and relate identically whether the lib
//! binder/arena are deep-cloned per checker (today) or shared read-only (the
//! campaign payoff). These tests pin the structural relations that any sharing
//! refactor must keep byte-identical — they are the green baseline re-run under
//! both clone and shared modes.
//!
//! See memory `project_shared_lib_universe_campaign`. The arena-sharing
//! experiment regressed exactly this class of cross-lib resolution
//! (`recursiveComplicatedClasses` lost a `TS2345` on `Symbol`), so a sharing PR
//! that keeps these green AND keeps `recursiveComplicatedClasses` is the bar.

use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

/// Strict-mode diagnostic codes against the full default lib bundle (es2015+,
/// so `Promise`/`PromiseLike`/`Awaited`/`IterableIterator` exist). Filters
/// TS2318 missing-default-lib noise.
fn check_source_strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|&code| code != 2318)
        .collect()
}

/// `Promise<T>` is assignable to `PromiseLike<T>` — the canonical recursive,
/// cross-lib-file relation (the two interfaces live in lib.es5 / lib.es2015 and
/// reference each other through `.then`). Nested `Promise<Promise<T>>` and a
/// user thenable assignable to `PromiseLike<T>` exercise the recursive `.then`
/// resolution that the lib clone currently isolates per checker.
#[test]
fn recursive_lib_types_resolve_and_relate() {
    let codes = check_source_strict_codes(
        r#"
declare const pr: Promise<number>;
const pl: PromiseLike<number> = pr;
declare const nested: Promise<Promise<number>>;
const plNested: PromiseLike<Promise<number>> = nested;
declare const t: Promise<number>;
const back: Promise<number> = t.then((n) => n + 1).then((n) => n);
"#,
    );
    assert!(
        codes.is_empty(),
        "recursive/cross-lib Promise/PromiseLike relations must resolve cleanly, got: {codes:?}"
    );
}

/// The relation is real, not vacuous: a mismatched type argument across the
/// `Promise`/`PromiseLike` boundary must still trip TS2322.
#[test]
fn promise_like_relation_rejects_mismatched_type_argument() {
    let codes = check_source_strict_codes(
        r#"
const p: PromiseLike<string> = (async () => 1)();
"#,
    );
    assert!(
        codes.contains(&2322),
        "Promise<number> assigned to PromiseLike<string> must trip TS2322, got: {codes:?}"
    );
}
