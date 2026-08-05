//! Regression probe for #16486: `export default <namespace>` / `export
//! default <enum>` lost their namespace meaning after #16480.
//!
//! Structural rule: `resolve_qualified_name`'s TS2702/TS2713 gate
//! (`crates/tsz-checker/src/state/type_analysis/qualified_names.rs`) asks a
//! default import's *target* for namespace meaning through
//! `resolve_import_target_has_namespace_meaning`. For `export default m;`
//! re-exporting a namespace/enum `m`, the binder's synthetic "default" ALIAS
//! symbol carries none of the referenced declaration's flags itself -- only
//! its identifier-reference declaration does -- so the gate must chase one
//! more hop (`default_export_identifier_target`) before concluding "no
//! namespace meaning". #16480 introduced the target-namespace-meaning check
//! without that hop, so a default-exported namespace/enum was wrongly
//! treated as namespace-less and every qualified access through it became a
//! spurious TS2702.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    let libs = load_lib_files(&["lib.es5.d.ts"]);
    check_multi_file_with_libs_stamped(files, entry, strict_options(), &libs)
}

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check(files, entry).into_iter().map(|d| d.code).collect()
}

/// The reported regression: a present member through a default-exported
/// namespace must resolve cleanly, never TS2702.
#[test]
fn export_default_namespace_keeps_namespace_meaning() {
    assert_eq!(
        codes(
            &[
                (
                    "/dep.ts",
                    "namespace m { export interface foo { a: number } }\nexport default m;\n"
                ),
                ("/main.ts", "import D from \"./dep\";\nvar q: D.foo;\n"),
            ],
            "/main.ts",
        ),
        Vec::<u32>::new(),
        "tsc resolves D.foo cleanly"
    );
}

/// Renamed-binder / different-declaration-kind adjacent case: a
/// default-exported *enum* (not namespace) member access through a renamed
/// import binding.
///
/// Known residual, tracked in #16499: the member-lookup path
/// (`resolve_symbol_export_for`/`left_sym_for_missing` in
/// `resolve_qualified_name`) reads the *un-followed* default-import alias
/// symbol for its own namespace flags, which is exactly the gap this PR's
/// TS2702 gate fix chases through -- but that chase lives only in the gate,
/// not in the member-lookup path, so an enum member access (present or
/// missing) through a default import still misfires here. This pins today's
/// behavior rather than leaving it to drift while #16499 is open; the point
/// of this test is that it is NOT TS2702 (this PR's actual fix).
#[test]
fn export_default_enum_member_access_through_default_import_is_never_ts2702() {
    let codes = codes(
        &[
            (
                "/e2.ts",
                "enum Color2 { Red, Green }\nexport default Color2;\n",
            ),
            (
                "/main3.ts",
                "import Palette from \"./e2\";\nvar ok: Palette.Red;\n",
            ),
        ],
        "/main3.ts",
    );
    assert!(
        !codes.contains(&2702),
        "a present member of a default-exported enum must not be TS2702, got: {codes:?}"
    );
    assert_eq!(
        codes,
        vec![2503],
        "known residual tracked in #16499, got: {codes:?}"
    );
}

/// A default-exported *class* (no merge partner) still has no namespace
/// meaning -- this must NOT regress to TS2694/clean just because the
/// identifier-hop above now exists. Negative control for the fix's scope.
#[test]
fn export_default_class_keeps_no_namespace_meaning() {
    assert_eq!(
        codes(
            &[
                ("/cls.ts", "export default class Widget {}\n"),
                (
                    "/main4.ts",
                    "import W from \"./cls\";\nvar a: W.NotAMember;\n"
                ),
            ],
            "/main4.ts",
        ),
        vec![2702],
        "a default-exported class keeps reporting TS2702, unaffected by the namespace/enum hop"
    );
}

/// The missing-member half of the regression: a missing member on a
/// default-exported enum must never surface as TS2702 ("used as a
/// namespace") -- the enum *has* namespace meaning, so a miss is a
/// member-lookup failure, not a meaning failure.
///
/// Known residual, tracked in #16499: tsz currently reports TS2503 ("cannot
/// find namespace") here instead of tsc's TS2694 ("no exported member") --
/// `left_sym_for_missing`'s namespace-flags gate in `resolve_qualified_name`
/// reads the *un-followed* default-import alias symbol, which does not itself
/// carry the enum's flags either. This assertion pins today's behavior (never
/// TS2702) without claiming full parity on the exact code.
#[test]
fn valid_namespace_member_through_default_namespace_import_is_never_ts2702() {
    let codes = codes(
        &[
            (
                "/e.ts",
                "enum Color { Red, Green }\nexport default Color;\n",
            ),
            ("/main2.ts", "import C from \"./e\";\nvar bad: C.Nope;\n"),
        ],
        "/main2.ts",
    );
    assert!(
        !codes.contains(&2702),
        "a missing member of a default-exported enum must not be TS2702, got: {codes:?}"
    );
    // Pin the current (not-yet-parity) shape so this doesn't silently drift
    // further from tsc while #16499 is open.
    assert_eq!(
        codes,
        vec![2503],
        "known residual tracked in #16499, got: {codes:?}"
    );
}
