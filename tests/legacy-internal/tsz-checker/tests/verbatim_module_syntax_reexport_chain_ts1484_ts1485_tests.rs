//! TS1484/TS1485 (type-only import) and the commonjs double-report for the
//! `verbatimModuleSyntax` re-export-chain family — tsz-org/tsz#17098.
//!
//! Two distinct defects, one underlying question ("does alias resolution
//! report the type-only boundary it crossed, when that boundary is in a third
//! file?"):
//!
//!  1. **The TS1484-vs-TS1485 picker.** tsc's `checkAliasSymbol` splits on
//!     `isType = !(getSymbolFlags(target) & Value)` over the FULLY resolved
//!     target, following the whole re-export chain: a pure type (no runtime
//!     value anywhere) is TS1484 *even when the chain crossed an explicit
//!     `export type` boundary*; TS1485 is reserved for a target that still
//!     carries a value but was reached across such a boundary. tsz previously
//!     keyed the split off whether this module's *immediate* export was a
//!     re-export alias (`is_import_specifier_alias_reexport`), which mislabels
//!     a pure type reached through a plain re-export hop (`export { Foo }`) or
//!     across an `export type` hop as TS1485.
//!
//!  2. **The commonjs early-return.** In a CommonJS file the ESM-in-CJS syntax
//!     error (TS1295) short-circuited both the import check (skipping
//!     TS1484/TS1485) and the named-export check (skipping TS1205). tsc reports
//!     the type-only diagnostic at the same anchor *alongside* TS1295.
//!
//! Oracle-verified against pinned `typescript@7.0.2`. The picker rows use
//! `module: preserve` (ESM-legal, so no CJS noise) and an import-only entry
//! file so exactly the import diagnostic surfaces; the commonjs rows use
//! `module: commonjs`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_multi_file, check_multi_file_with_global_index};
use tsz_common::common::ModuleKind;

const RE_EXPORTING_A_TYPE: u32 = 1205;
const ESM_IMPORTS_EXPORTS_IN_COMMONJS: u32 = 1295;
const IMPORT_IS_A_TYPE: u32 = 1484;
const IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION: u32 = 1485;

fn check(files: &[(&str, &str)], entry: &str, module: ModuleKind) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module,
            strict: true,
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

// Import-only entry: isolates the type-only-import diagnostic (no `export =`
// noise) so each picker row asserts exactly one code.
const IMPORT_ONLY_MAIN: &str = "import { Foo } from \"./reexport\";\nexport type Y = Foo;\n";

// ===========================================================================
// Picker: TS1484 vs TS1485 across the re-export chain (module: preserve)
// ===========================================================================

/// A pure interface reached through a *plain* (`export { Foo }`) re-export hop
/// is TS1484 — the target carries no value. Previously mislabeled TS1485
/// because the immediate export is a re-export alias.
#[test]
fn plain_reexport_of_interface_import_is_ts1484() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "a pure type through a plain re-export hop must be TS1484, got: {diagnostics:?}"
    );
}

/// A pure interface reached across an explicit `export type { Foo }` boundary
/// is STILL TS1484: the split is on the target's value-ness, not on whether a
/// type-only boundary was crossed. This is the row that proves the boundary
/// alone does not force TS1485.
#[test]
fn export_type_reexport_of_interface_import_is_ts1484() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "a pure type across an `export type` boundary is still TS1484, got: {diagnostics:?}"
    );
}

/// A class (a real value) reached across an explicit `export type { Foo }`
/// boundary is TS1485 — the target keeps its value, so it "resolves to a
/// type-only declaration" rather than "is a type".
#[test]
fn export_type_reexport_of_class_import_is_ts1485() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION],
        "a value across an `export type` boundary must be TS1485, got: {diagnostics:?}"
    );
}

/// Control: a class reached through a *plain* re-export hop (no type-only
/// boundary anywhere) is a legal value import — no TS1484/TS1485.
#[test]
fn plain_reexport_of_class_import_is_clean() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport const x = new Foo();\n",
            ),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == IMPORT_IS_A_TYPE
                || d.code == IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION),
        "a value through a plain re-export must not be flagged type-only, got: {diagnostics:?}"
    );
}

/// The chain follow must not stop after one hop: a two-hop plain re-export of
/// a pure interface is still TS1484.
#[test]
fn two_hop_plain_reexport_of_interface_import_is_ts1484() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/mid.ts", "export { Foo } from \"./impl\";\n"),
            ("/reexport.ts", "export { Foo } from \"./mid\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "a pure type through two plain re-export hops must be TS1484, got: {diagnostics:?}"
    );
}

/// Renamed on import (`Foo as Baz`) through a plain hop — the chain follow
/// keys off the resolved target, not the local binding name — still TS1484.
#[test]
fn renamed_plain_reexport_of_interface_import_is_ts1484() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./reexport\";\nexport type Y = Baz;\n",
            ),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "a renamed pure-type import through a plain hop must be TS1484, got: {diagnostics:?}"
    );
}

/// Control (single-file boundary): a directly imported `export type` alias is
/// a pure type — TS1484. Guards the picker's common single-hop path.
#[test]
fn direct_type_alias_import_is_ts1484() {
    let diagnostics = check(
        &[
            ("/m.ts", "export type T = number;\n"),
            (
                "/main.ts",
                "import { T } from \"./m\";\nexport type Y = T;\n",
            ),
        ],
        "/main.ts",
        ModuleKind::Preserve,
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "a direct `export type` alias import must be TS1484, got: {diagnostics:?}"
    );
}

/// Regression guard: an uninstantiated (type-only) namespace import carries no
/// runtime value, so it is TS1484 — even though `lookup_imported_target_flags`
/// treats the module declaration as value-bearing, the absence of a crossed
/// `export type` boundary keeps the split on TS1484. Uses the global-index
/// harness (like the real driver) because the cross-file namespace-member
/// resolution behind the type-only gate needs the declaring-file index.
#[test]
fn type_only_namespace_import_is_ts1484() {
    let diagnostics = check_multi_file_with_global_index(
        &[
            ("/m.ts", "export namespace NS { export type X = number; }\n"),
            (
                "/main.ts",
                "import { NS } from \"./m\";\nexport type Y = NS.X;\n",
            ),
        ],
        "/main.ts",
        CheckerOptions {
            module: ModuleKind::Preserve,
            strict: true,
            verbatim_module_syntax: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        codes(&diagnostics),
        vec![IMPORT_IS_A_TYPE],
        "an uninstantiated namespace import must be TS1484, got: {diagnostics:?}"
    );
}

// ===========================================================================
// CommonJS double-report: TS1295 alongside the type-only diagnostic
// ===========================================================================

/// In a CommonJS file, a type-only named import reports BOTH the ESM-in-CJS
/// syntax error (TS1295) and the type-only-import diagnostic (TS1484) at the
/// same anchor — the early-return after TS1295 previously dropped the latter.
#[test]
fn commonjs_type_import_reports_ts1295_and_ts1484() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::CommonJS,
    );
    let cs = codes(&diagnostics);
    assert!(
        cs.contains(&ESM_IMPORTS_EXPORTS_IN_COMMONJS) && cs.contains(&IMPORT_IS_A_TYPE),
        "commonjs type import must report both TS1295 and TS1484, got: {diagnostics:?}"
    );
}

/// The commonjs double-report keeps the picker's TS1485 branch too: a value
/// reached across an `export type` boundary reports TS1295 + TS1485.
#[test]
fn commonjs_type_import_through_export_type_to_class_reports_ts1295_and_ts1485() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            ("/main.ts", IMPORT_ONLY_MAIN),
        ],
        "/main.ts",
        ModuleKind::CommonJS,
    );
    let cs = codes(&diagnostics);
    assert!(
        cs.contains(&ESM_IMPORTS_EXPORTS_IN_COMMONJS)
            && cs.contains(&IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION),
        "commonjs value-through-export-type import must report TS1295 and TS1485, got: {diagnostics:?}"
    );
}

/// The export side mirrors it: a plain re-export of a type in a CommonJS file
/// reports BOTH TS1205 (re-exporting a type) and TS1295 at the same anchor.
#[test]
fn commonjs_plain_reexport_of_type_reports_ts1205_and_ts1295() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
        ],
        "/reexport.ts",
        ModuleKind::CommonJS,
    );
    let cs = codes(&diagnostics);
    assert!(
        cs.contains(&RE_EXPORTING_A_TYPE) && cs.contains(&ESM_IMPORTS_EXPORTS_IN_COMMONJS),
        "commonjs plain re-export of a type must report both TS1205 and TS1295, got: {diagnostics:?}"
    );
}

/// Control: an explicit `export type { Foo }` re-export in a CommonJS file is
/// fully type-only (erased), so it reports neither TS1205 nor TS1295 — the
/// fall-through must not manufacture a diagnostic where tsc stays silent.
#[test]
fn commonjs_export_type_reexport_is_clean() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
        ],
        "/reexport.ts",
        ModuleKind::CommonJS,
    );
    let cs = codes(&diagnostics);
    assert!(
        !cs.contains(&RE_EXPORTING_A_TYPE) && !cs.contains(&ESM_IMPORTS_EXPORTS_IN_COMMONJS),
        "an `export type` re-export in commonjs must stay clean, got: {diagnostics:?}"
    );
}
