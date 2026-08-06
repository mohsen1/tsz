//! TS2430 ("interface incorrectly extends interface") when the base's own
//! declaration(s) live in a different arena than the interface being
//! checked — the common case of a user interface extending a lib generic
//! such as `Array<T>`.
//!
//! `check_interface_extension_compatibility`'s type-level comparison pass
//! (generic method overrides, overload coverage, and a *derived* interface's
//! own index signature vs an inherited one) resolves its base interface's
//! declaration through `base_iface_indices.first()`, a `NodeIndex` that may
//! belong to a foreign arena (e.g. `lib.es5.d.ts`'s `Array` declaration).
//! Reading it through `self.ctx.arena` instead of the declaration's own
//! arena (via `arena_for_declaration_or`) silently misses — `NodeArena::get`
//! returns `None` for an out-of-range foreign index — which `continue`d past
//! the *entire* type-level comparison, not just the affected member. A
//! derived interface's own conflicting index signature against a lib
//! generic's index signature went unreported entirely.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};

fn lib_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("./main.ts", source)],
        "./main.ts",
        CheckerOptions::default(),
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Reported repro: a user interface's own numeric index signature
/// incompatible with `Array<T>`'s inherited numeric index signature must
/// report TS2430 — tsc: "'number' index signatures are incompatible."
#[test]
fn extends_array_incompatible_own_index_signature_reports_ts2430() {
    let source = r#"
interface MyExt extends Array<string> {
    [n: number]: number;
}
"#;
    assert!(
        lib_codes(source).contains(&2430),
        "a derived interface's own index signature incompatible with Array's inherited one must report TS2430"
    );
}

/// Negative: a compatible own index signature must not report TS2430.
#[test]
fn extends_array_compatible_own_index_signature_no_ts2430() {
    let source = r#"
interface MyExt extends Array<string> {
    [n: number]: string;
}
"#;
    assert!(
        !lib_codes(source).contains(&2430),
        "an own index signature compatible with Array's inherited one must not report TS2430"
    );
}

/// Adjacent: `ReadonlyArray<T>` is a distinct lib generic from `Array<T>`
/// with its own separate index signature; the same rule must apply.
#[test]
fn extends_readonly_array_incompatible_own_index_signature_reports_ts2430() {
    let source = r#"
interface MyExt extends ReadonlyArray<string> {
    [n: number]: number;
}
"#;
    assert!(
        lib_codes(source).contains(&2430),
        "an index signature incompatible with ReadonlyArray's inherited one must report TS2430"
    );
}

/// Adjacent: renaming the derived interface must not change the outcome —
/// the fix must not key off any user-chosen identifier.
#[test]
fn extends_array_incompatible_own_index_signature_renamed_binder_reports_ts2430() {
    let source = r#"
interface CompletelyDifferentName extends Array<string> {
    [n: number]: number;
}
"#;
    assert!(
        lib_codes(source).contains(&2430),
        "renaming the derived interface must not suppress the TS2430 report"
    );
}
