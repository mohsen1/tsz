//! Regression tests for issue #16915: `globalThis.<prop>` false-positive
//! (TS2551/TS2339) when `globalThis` is augmented via
//! `declare global { namespace globalThis { ... } } }`.
//!
//! `tsc` merges a reopened `namespace globalThis { ... }` into the same
//! Symbol as the ambient `globalThis`, so a missing member routes through
//! the dedicated globalThis-missing-member path (any / TS7017 under
//! `noImplicitAny` / TS2339 for a block-scoped global) and never the
//! generic "Did you mean" suggestion (TS2551). tsz's binder gives the
//! augmenting namespace its own `SymbolId` instead of unifying identity
//! with the lib var, so `is_global_this_expression` (the receiver-detection
//! guard in `crates/tsz-checker/src/types/queries/core.rs`) treated the
//! augmented receiver as an ordinary shadow and fell back to generic
//! property resolution, which does suggest — wrongly. All expectations
//! below are oracle-verified byte-for-byte against `typescript@7.0.2`
//! (bare compiler options: neither `strict` nor `noImplicitAny` is set,
//! which tsc's own `getStrictOptionValue` resolves to `noImplicitAny`
//! effectively `true`, hence TS7017 rather than a silent `any`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs_stamped, check_source_with_libs, load_default_lib_files,
};

fn same_file_codes(source: &str) -> Vec<(u32, u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs(source, "index.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| (d.code, d.start, d.message_text))
        .collect()
}

fn cross_file_codes(files: &[(&str, &str)], entry: &str) -> Vec<(u32, u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_multi_file_with_libs_stamped(files, entry, CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| (d.code, d.start, d.message_text))
        .collect()
}

#[test]
fn declare_global_namespace_augmentation_same_file_no_ts2551() {
    // tsc: TS7017 on the typo'd `tests` write (missing-member path under
    // effective noImplicitAny), never TS2551. The correctly spelled read on
    // the next line resolves through the augmentation and stays clean.
    let codes = same_file_codes(
        r#"
declare global {
    namespace globalThis {
        var test: string;
    }
}
export {};
globalThis.tests = "a-b";
console.log(globalThis.test.split("-"));
"#,
    );
    assert!(
        !codes.iter().any(|(code, ..)| *code == 2551),
        "TS2551 must not fire when globalThis is augmented via declare global namespace globalThis; got: {codes:?}"
    );
    assert!(
        codes.iter().any(|(code, ..)| *code == 7017),
        "TS7017 (missing-member path) must fire for the genuinely-missing 'tests' property; got: {codes:?}"
    );
}

// NOTE: the conformance corpus repro (`extendGlobalThis.ts`) declares the
// `declare global { namespace globalThis { ... } }` augmentation in a
// *separate* declaration file from its use, imported by the checking file.
// `check_multi_file_with_libs_stamped` does not replicate the production
// driver's cross-file `global_augmentations` merge (`SharedBinderData` /
// `create_binder_from_bound_file_with_augmentations` in
// `crates/tsz-cli/src/driver/check_utils.rs`), so a checking file's own
// `binder.global_augmentations` never sees an augmentation declared in a
// different file under this harness — the guard added to
// `is_global_this_expression` correctly no-ops and this specific
// harness/production gap is not exercised by an in-process test. The
// cross-file shape is instead verified against a built `tsz` CLI byte-for-
// byte matching the pinned `typescript@7.0.2` oracle (see PR verification).
// `declare_global_namespace_augmentation_read_only_use_is_clean` below still
// exercises the cross-file import path end to end for the clean case.

#[test]
fn declare_global_namespace_augmentation_read_only_use_is_clean() {
    // Isolate the correctly-spelled read: no diagnostics at all, matching
    // the oracle exactly (`'test'` resolves to `string` through the merged
    // augmentation, so `.split` type-checks).
    let files: &[(&str, &str)] = &[
        (
            "extension.d.ts",
            r#"
declare global {
    namespace globalThis {
        var test: string;
    }
}
export {};
"#,
        ),
        (
            "index.ts",
            r#"
import "./extension";
console.log(globalThis.test.split("-"));
"#,
        ),
    ];
    let codes = cross_file_codes(files, "index.ts");
    assert!(
        codes.is_empty(),
        "declare-global-augmented globalThis.test access must be fully clean; got: {codes:?}"
    );
}

#[test]
fn plain_module_local_namespace_globalthis_still_shadows() {
    // Adjacent negative: WITHOUT `declare global`, a module-local
    // `namespace globalThis { ... }` is a plain (conflicting) local
    // declaration, not a global augmentation — it must keep shadowing
    // rather than being treated as globalThis-like by the new guard.
    //
    // Oracle-confirmed (typescript@7.0.2, re-verified for #17203): tsc does
    // NOT report TS2551 here — declaring `namespace globalThis` at all is
    // itself an error (TS2397, the declaration conflicts with the built-in
    // global identifier), and the property write then goes through the
    // globalThis-missing-member path (TS7017 under noImplicitAny), never the
    // generic "did you mean" suggestion. This test previously pinned TS2551,
    // which tsc never emits for this shape; that was a stale expectation,
    // not a regression — tsz now matches tsc exactly.
    let codes = same_file_codes(
        r#"
namespace globalThis {
    export const test = 1;
}
globalThis.tests = "a-b";
"#,
    );
    assert!(
        codes.iter().any(|(code, ..)| *code == 2397),
        "declaring `namespace globalThis` must still report TS2397 (conflicts with the \
         built-in global identifier); got: {codes:?}"
    );
    assert!(
        codes.iter().any(|(code, ..)| *code == 7017),
        "the property write must still go through the globalThis-missing-member path \
         (TS7017), not a generic TS2551 suggestion; got: {codes:?}"
    );
    assert!(
        !codes.iter().any(|(code, ..)| *code == 2551),
        "TS2551 must not fire for a module-local `namespace globalThis`; got: {codes:?}"
    );
}

#[test]
fn value_shadow_const_globalthis_still_shadows() {
    // Adjacent negative: a genuine VALUE shadow (`const globalThis = ...`)
    // has no MODULE flag and must be unaffected by the new guard.
    let codes = same_file_codes(
        r#"
const globalThis = { test: "x" };
globalThis.tests = "a-b";
"#,
    );
    assert!(
        codes.iter().any(|(code, ..)| *code == 2551),
        "a local const globalThis value shadow must still shadow and report a \
         property-not-found diagnostic; got: {codes:?}"
    );
}
