//! TS1293: `module: "preserve"` + `isolatedModules` (without
//! `verbatimModuleSyntax`) forbids ESM import/export syntax that carries a
//! binding in a CommonJS file.
//!
//! Structural rule: under `preserve`, a file's CJS-ness is *always*
//! extension-locked (`.cts`/`.cjs`) — `preserve` cannot pair with the
//! `node16`/`nodenext` `moduleResolution` that `package.json`
//! `"type"`-based detection requires, so there is no adjustable variant of
//! this diagnostic the way `verbatimModuleSyntax` has TS1286 vs TS1295.
//! `tsc` fires it only for clauses that need a runtime require()/exports
//! rewrite — default/namespace/named imports, `export { ... }` (local or
//! `from`), and `export * as ns from` — and exempts type-only forms
//! (`import type`, `export type { ... }`), bare side-effect forms (`import
//! "./m"`, `export * from "./m"`), and `export` on a value declaration
//! (`export const x = 1`, which stays legal even in this mode). When
//! `verbatimModuleSyntax` is also on, it takes over entirely (TS1286/TS1295)
//! and TS1293 never fires. Oracle-verified against `typescript@7.0.2`.
//!
//! Owner: parser-adjacent checker gate in
//! `crates/tsz-checker/src/declarations/import/verbatim.rs` (imports) and
//! `crates/tsz-checker/src/declarations/module_checker/verbatim_module_syntax.rs`
//! (named/re-exports) — the same CJS-forbidden-ESM-syntax mechanism
//! `verbatimModuleSyntax` already uses for TS1286/TS1295, gated on
//! `preserve_isolated_modules_cjs_check_active` instead.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS1293: u32 = 1293;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::Preserve,
            isolated_modules: true,
            ..CheckerOptions::default()
        },
    )
}

fn check_vms(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::Preserve,
            isolated_modules: true,
            verbatim_module_syntax: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

// ---------------------------------------------------------------------
// Imports
// ---------------------------------------------------------------------

#[test]
fn named_import_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import { y } from \"./b\";\ny;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

#[test]
fn default_import_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export default 3;\n"),
            ("/main.cts", "import def from \"./b\";\ndef;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

#[test]
fn namespace_import_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import * as ns from \"./b\";\nns;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

/// Renamed binder: the diagnostic must not key off a fixed identifier name.
#[test]
fn renamed_named_import_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const somethingElse = 2;\n"),
            (
                "/main.cts",
                "import { somethingElse as renamed } from \"./b\";\nrenamed;\n",
            ),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

/// `import type` is exempt — it is erased entirely, no runtime rewrite needed.
#[test]
fn type_only_import_in_cts_is_clean() {
    let diagnostics = check(
        &[
            ("/b.ts", "export interface Y {}\n"),
            ("/main.cts", "import type { Y } from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Bare side-effect import has no binding to rewrite — exempt.
#[test]
fn side_effect_import_in_cts_is_clean() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// `.mts` is ESM-locked, never CJS — TS1293 cannot apply.
#[test]
fn named_import_in_mts_is_clean() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.mts", "import { y } from \"./b\";\ny;\n"),
        ],
        "/main.mts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Without `isolatedModules`, `module: preserve` alone does not gate TS1293.
#[test]
fn named_import_in_cts_without_isolated_modules_is_clean() {
    let diagnostics = check_multi_file(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import { y } from \"./b\";\ny;\n"),
        ],
        "/main.cts",
        CheckerOptions {
            module: ModuleKind::Preserve,
            isolated_modules: false,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// `verbatimModuleSyntax` takes over entirely; TS1293 never fires alongside it.
#[test]
fn named_import_in_cts_under_vms_reports_ts1286_not_ts1293() {
    const TS1286: u32 = 1286;
    let diagnostics = check_vms(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import { y } from \"./b\";\ny;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1286], "{diagnostics:?}");
}

// ---------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------

#[test]
fn named_reexport_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "export { y } from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

/// Local named export (no `from`) is the same clause shape.
#[test]
fn local_named_export_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[("/main.cts", "const z = 1;\nexport { z as w };\n")],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

/// Renamed export alias — must not key off a fixed identifier name.
#[test]
fn renamed_named_reexport_in_cts_reports_ts1293() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const somethingElse = 2;\n"),
            (
                "/main.cts",
                "export { somethingElse as renamedOut } from \"./b\";\n",
            ),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}

/// `export type { ... } from` is erased entirely — exempt.
#[test]
fn type_only_named_reexport_in_cts_is_clean() {
    let diagnostics = check(
        &[
            ("/b.ts", "export interface Y {}\n"),
            ("/main.cts", "export type { Y } from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// A value declaration carrying the `export` modifier keeps working —
/// `preserve`'s CJS emit can still translate this without ESM clause syntax.
#[test]
fn exported_value_declaration_in_cts_is_clean() {
    let diagnostics = check(&[("/main.cts", "export const z = 5;\n")], "/main.cts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Bare `export * from "./m"` has no clause (no local binding at all) —
/// exempt, unlike `export * as ns from`.
#[test]
fn bare_star_reexport_in_cts_is_clean() {
    let diagnostics = check(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "export * from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}
