//! Regression tests for a false-positive `TS7015`/`TS7053` when a well-known
//! symbol (`Symbol.iterator`, etc.) is reached through an alias binding
//! (`const S = Symbol;` or `const S = globalThis.Symbol;`) rather than
//! spelled directly off the global `Symbol`.
//!
//! `tsc` resolves `S.iterator` to the same well-known symbol regardless of
//! how `S` was obtained, because the identity lives in the TYPE
//! (`SymbolConstructor.iterator`'s declared `unique symbol`), not in the
//! access syntax. tsz's element-access path only recognized the well-known
//! shape through a purely-syntactic check requiring the base identifier's
//! literal text to be `"Symbol"` (`computed_names::well_known_symbol_access_shape`);
//! an alias bypassed that shortcut and fell into a numeric fallback that
//! decoded the key's `SymbolRef` as a binder `SymbolId` — but a well-known
//! symbol's `SymbolRef` is minted by the lowering layer from the `unique
//! symbol` type-operator's NODE INDEX
//! (`tsz_lowering::lower::advanced::lower_type_operator`), not a `SymbolId`
//! at all, so the raw-id lookup silently resolved to an unrelated symbol
//! that happened to share the same per-binder-local number.
//!
//! Fix: identify the well-known-symbol member NAME by TYPE IDENTITY against
//! the lib's own `SymbolConstructor` interface
//! (`well_known_symbol_name_by_type_identity`) instead of decoding the
//! `SymbolRef` as an id.
//!
//! Every read of `Symbol.<name>` — direct or aliased — resolves through
//! ordinary property-type computation against the same merged
//! `SymbolConstructor` type, so the fix works uniformly for both spellings.
//!
//! Oracle-verified against `typescript@7.0.2`, `--target es6`.
//!
//! Issue: <https://github.com/tsz-org/tsz/issues/16961>

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn check_with_libs(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

#[track_caller]
fn assert_fully_clean(src: &str) {
    let diags = check_with_libs(src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

// ---------------------------------------------------------------------------
// Direct spelling stays clean (control — must not regress).
// ---------------------------------------------------------------------------

#[test]
fn direct_symbol_iterator_element_access_stays_clean() {
    assert_fully_clean("[][Symbol.iterator];");
}

// ---------------------------------------------------------------------------
// Alias forms — the false-positive family.
// ---------------------------------------------------------------------------

#[test]
fn simple_const_alias_of_symbol() {
    assert_fully_clean(
        "
        const S = Symbol;
        [][S.iterator];
        ",
    );
}

#[test]
fn shadowed_symbol_via_global_this() {
    assert_fully_clean(
        "
        export {};
        const Symbol = globalThis.Symbol;
        [][Symbol.iterator];
        ",
    );
}

#[test]
fn aliased_global_this_symbol() {
    assert_fully_clean(
        "
        const S = globalThis.Symbol;
        [][S.iterator];
        ",
    );
}

#[test]
fn shadowed_symbol_used_as_object_literal_computed_key_and_alias_element_access() {
    assert_fully_clean(
        "
        export {};
        const Symbol = globalThis.Symbol;
        const o = { [Symbol.iterator]: 1 };
        [][Symbol.iterator];
        ",
    );
}

/// Binder-name independence: the alias must work under an arbitrary
/// identifier, not just `S`/`Symbol`.
#[test]
fn arbitrary_alias_identifier_name() {
    assert_fully_clean(
        "
        const zzTop = Symbol;
        [][zzTop.iterator];
        ",
    );
}

/// The pre-existing (non-array) receiver case: same missed lookup, reported
/// as `TS7053` rather than `TS7015` since there is no element-type fallback
/// to hide it. Receiver-independent — this predates the array-specific fix
/// in #16958 and must be fixed by the same identity-based lookup.
#[test]
fn aliased_symbol_on_string_receiver() {
    assert_fully_clean(
        r#"
        const S = globalThis.Symbol;
        "abc"[S.iterator];
        "#,
    );
}

// ---------------------------------------------------------------------------
// Negative controls — must still error; the fix must not over-suppress.
// ---------------------------------------------------------------------------

#[test]
fn unrelated_unique_symbol_still_reports_ts7015() {
    let diags = check_with_libs(
        "
        declare const s: unique symbol;
        [][s];
        ",
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7015),
        "expected TS7015 for an unrelated unique symbol key, got: {diags:?}"
    );
}

#[test]
fn plain_wide_symbol_still_reports_ts7015() {
    let diags = check_with_libs(
        "
        declare const s: symbol;
        [][s];
        ",
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7015),
        "expected TS7015 for a plain `symbol`-typed key, got: {diags:?}"
    );
}

#[test]
fn well_known_symbol_hasinstance_still_resolves_through_alias() {
    assert_fully_clean(
        "
        class C {
            static [Symbol.hasInstance](x: unknown): boolean {
                return true;
            }
        }
        const S = Symbol;
        (C as any)[S.hasInstance];
        ",
    );
}
