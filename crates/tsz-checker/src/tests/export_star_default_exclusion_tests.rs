//! `export * from "./m"` never forwards `m`'s `default` export.
//!
//! Structural rule: ECMAScript's `ExportStarAsNamedExports` resolution
//! explicitly drops the local name `default` from the set of names a wildcard
//! export re-exports (only `export { default } from "./m"` or
//! `export { x as default } from "./m"` forward a default, and both are
//! *named* re-exports tracked separately from the wildcard chain). `tsc`
//! matches this in `visitExportedUnnamedExportBindings`, which is called only
//! when `specifier.name.escapedText !== InternalSymbolName.Default`.
//!
//! Before this fix, `resolve_export_in_file_uncached`'s wildcard-reexport
//! branch and `collect_reexported_symbols`'s namespace-member collector both
//! walked every name in a wildcard source's export table, `default` included,
//! so a barrel's default import silently resolved to whichever re-exported
//! module happened to declare one — the exact opposite of `tsc`, which
//! reports `TS1192`/`TS2305`/`TS2339` depending on the import form.

use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// `import d from "./barrel"` where `barrel.ts` is `export * from "./impl"`
/// and only `impl.ts` has a default: `tsc` reports `TS1192`, the barrel
/// itself never gains a default just by wildcard-re-exporting one.
#[test]
fn default_import_through_wildcard_barrel_reports_ts1192() {
    let diags = check(
        &[
            (
                "./impl.ts",
                r#"export default 42;
export const named = 1;
"#,
            ),
            ("./barrel.ts", r#"export * from "./impl";"#),
            (
                "./consumer.ts",
                r#"import d from "./barrel";
const y = d;
"#,
            ),
        ],
        "./consumer.ts",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "expected TS1192, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// `import * as ns from "./barrel"; ns.default` — the wildcard source's
/// default must not leak onto the barrel's namespace object either.
#[test]
fn namespace_import_through_wildcard_barrel_has_no_default_member() {
    let diags = check(
        &[
            (
                "./impl.ts",
                r#"export default 42;
export const named = 1;
"#,
            ),
            ("./barrel.ts", r#"export * from "./impl";"#),
            (
                "./consumer.ts",
                r#"import * as ns from "./barrel";
const y = ns.default;
"#,
            ),
        ],
        "./consumer.ts",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected TS2339 for ns.default, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Positive control: a barrel that names its wildcard source's default
/// explicitly (`export { default } from "./impl"`) still forwards it —
/// only the *wildcard* path drops `default`, not an explicit named re-export.
#[test]
fn named_reexport_of_default_still_forwards_through_barrel() {
    let diags = check(
        &[
            ("./impl.ts", r#"export default 42;"#),
            (
                "./barrel.ts",
                r#"export * from "./impl";
export { default } from "./impl";
"#,
            ),
            (
                "./consumer.ts",
                r#"import d from "./barrel";
const y: string = d;
"#,
            ),
        ],
        "./consumer.ts",
    );
    // `d` resolves to `number` (impl's real default), so assigning it to a
    // `string`-typed binding is the one and only error — not TS1192.
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "an explicit `export {{ default }}` must still forward the default: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "expected the number-to-string mismatch once d resolves, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Positive control: a barrel that declares its own `export default` keeps
/// it — the fix must only suppress a *wildcard-forwarded* default, not a
/// default declared directly in the consuming file.
#[test]
fn own_default_export_is_unaffected_by_wildcard_exclusion() {
    let diags = check(
        &[
            ("./impl.ts", r#"export const named = 1;"#),
            (
                "./barrel.ts",
                r#"export * from "./impl";
export default "own default";
"#,
            ),
            (
                "./consumer.ts",
                r#"import d from "./barrel";
const y: number = d;
"#,
            ),
        ],
        "./consumer.ts",
    );
    assert!(
        !codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "barrel's own default export must still resolve: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "d should resolve to the barrel's own string default, mismatching `number`: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// Renamed bindings and a chained (two-hop) wildcard barrel: the exclusion
/// must survive an extra `export *` hop and does not depend on any
/// particular identifier spelling.
#[test]
fn default_exclusion_survives_a_chained_wildcard_barrel() {
    let diags = check(
        &[
            (
                "./core.ts",
                r#"export default { version: 7 };
export const flavor = "vanilla";
"#,
            ),
            ("./mid.ts", r#"export * from "./core";"#),
            ("./outer.ts", r#"export * from "./mid";"#),
            (
                "./consumer.ts",
                r#"import wrapped from "./outer";
const y = wrapped;
"#,
            ),
        ],
        "./consumer.ts",
    );
    assert!(
        codes(&diags).contains(&diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT),
        "expected TS1192 through a two-hop wildcard chain, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}
