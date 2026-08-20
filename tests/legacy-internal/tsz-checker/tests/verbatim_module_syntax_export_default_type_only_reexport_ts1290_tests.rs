//! `export default <name>` under `isolatedModules` (or `verbatimModuleSyntax`),
//! where `<name>` is an import alias whose resolution chain crosses an
//! explicit `export type { ... }` re-export boundary in another file, but
//! whose ultimate declaration is a real value (a class).
//!
//! Sibling of TS1289 (`export =`, see
//! `verbatim_module_syntax_export_equals_ts1291_tests.rs`) for `export
//! default`: tsc's `checkExportAssignment` picks TS1290 (not TS1292) when
//! `getTypeOnlyAliasDeclarationEx` finds an explicit type-only alias
//! declaration somewhere in the chain, in a file other than this one, even
//! though the target overall still resolves to a value. `check_verbatim_module_syntax_export_default`'s
//! early TS1284/TS1285 branch only inspects the LOCAL symbol's own
//! `is_type_only` flag, so it cannot see this either — both TS1285 (VMS) and
//! TS1290 (isolatedModules-like) were unreported for this shape before this
//! change.
//!
//! Oracle-confirmed against `typescript@7.0.2`: `import { Foo } from
//! "./reexport"; export default Foo;` where `reexport.ts` does `export type
//! { Foo } from "./impl";` and `impl.ts` does `export class Foo {}` reports
//! **both** TS1285 and TS1290 under `verbatimModuleSyntax`, and only TS1290
//! under `isolatedModules` alone.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_DEFAULT_REAL_VALUE: u32 = 1285;
const IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION: u32 = 1485;
const RESOLVES_TO_A_TYPE_ONLY_DECLARATION: u32 = 1290;
const RESOLVES_TO_A_TYPE: u32 = 1292;

fn check(files: &[(&str, &str)], entry: &str, verbatim_module_syntax: bool) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            verbatim_module_syntax,
            isolated_modules: !verbatim_module_syntax,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// An import alias whose chain crosses an explicit `export type { ... }`
/// re-export boundary, but whose ultimate declaration is a real value (a
/// class), reports TS1285 + TS1290 under `verbatimModuleSyntax` (plus the
/// pre-existing TS1485 on the local import statement).
#[test]
fn import_alias_through_export_type_reexport_to_class_reports_1285_and_1290_verbatim() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_DEFAULT_REAL_VALUE,
            RESOLVES_TO_A_TYPE_ONLY_DECLARATION,
            IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION,
        ],
        "expected TS1285 and TS1290 (plus the pre-existing TS1485 on the import \
         statement itself) for an import alias crossing an `export type` \
         re-export boundary under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same shape, but only `isolatedModules` is enabled: tsc reports only
/// TS1290 (TS1285 is verbatimModuleSyntax-only, matching TS1292's own
/// isolatedModules-only sibling test).
#[test]
fn import_alias_through_export_type_reexport_to_class_reports_only_1290_under_isolated_modules() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE_ONLY_DECLARATION],
        "expected only TS1290 under isolatedModules (no verbatimModuleSyntax), got: {diagnostics:?}"
    );
}

/// Renamed on import (`Foo as Baz`) — the chain lookup must key off the
/// resolved import target/name, not the local binding name.
#[test]
fn renamed_import_alias_through_export_type_reexport_reports_1290() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./reexport\";\nexport default Baz;\n",
            ),
        ],
        "/main.ts",
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE_ONLY_DECLARATION],
        "expected only TS1290 for a renamed import through an `export type` \
         re-export boundary, got: {diagnostics:?}"
    );
}

/// Negative control: an import alias to a class with NO type-only boundary
/// anywhere in the chain (plain `export { Foo }` re-export) must not
/// trigger TS1290 or TS1285 — same "boundary exists" vs "value exists"
/// isolation as the TS1289 negative control.
#[test]
fn import_alias_through_plain_reexport_to_class_reports_neither() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
        true,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_DEFAULT_REAL_VALUE
                || d.code == RESOLVES_TO_A_TYPE_ONLY_DECLARATION
                || d.code == RESOLVES_TO_A_TYPE),
        "expected no TS1285/1290/1292 for a value-carrying alias through a \
         plain re-export with no type-only boundary, got: {diagnostics:?}"
    );
}
