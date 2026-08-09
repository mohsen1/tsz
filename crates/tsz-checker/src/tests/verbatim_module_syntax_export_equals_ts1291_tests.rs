//! `export = <name>` under `isolatedModules` (or `verbatimModuleSyntax`),
//! where `<name>` is an import alias resolving purely to a type.
//!
//! `check_vms_export_equals` already reports TS1282/TS1283 for `export =`
//! under `verbatimModuleSyntax` when the *local* symbol's own flags mark it
//! as a pure type or type-only import. It never covered tsc's second,
//! independent check in `checkExportAssignment`: for `export =` specifically
//! (the `isExportEquals` branch), when the identifier is an *alias* whose
//! resolved import target carries Type but not Value, and the alias was not
//! declared `import type` in this file, tsc additionally reports TS1291 —
//! gated on `isolatedModules`-like (either flag), not `verbatimModuleSyntax`
//! alone. `export default`'s mirror of this (TS1292) was already wired in
//! `check_verbatim_module_syntax_export_default`; TS1291 was its unwired
//! `export =` sibling (tsz-org/tsz#16291's unwired-diagnostic-codes sweep).
//!
//! Oracle-confirmed against `typescript@7.0.2` (`--module preserve`, which
//! keeps both `export =` and ESM import syntax legal so the matrix isolates
//! TS1291 from the unrelated CJS-import-under-VMS diagnostics TS1286/TS1295).

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_EQUALS_MUST_REFERENCE_A_VALUE: u32 = 1282;
const IMPORT_MUST_BE_TYPE_ONLY: u32 = 1484;
const RESOLVES_TO_A_TYPE: u32 = 1291;

fn check(
    files: &[(&str, &str)],
    entry: &str,
    verbatim_module_syntax: bool,
    isolated_modules: bool,
) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::Preserve,
            strict: true,
            verbatim_module_syntax,
            isolated_modules,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// A plain import alias to an interface-only module member, referenced by
/// `export =`, reports both TS1282 and TS1291 under `verbatimModuleSyntax`
/// (plus the pre-existing TS1484 on the import statement itself).
#[test]
fn import_alias_to_interface_export_equals_reports_both_verbatim() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_EQUALS_MUST_REFERENCE_A_VALUE,
            RESOLVES_TO_A_TYPE,
            IMPORT_MUST_BE_TYPE_ONLY,
        ],
        "expected TS1282 and TS1291 (plus the pre-existing TS1484 on the import \
         statement itself) for an import-alias export= resolving to a pure type \
         under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same shape, renamed on import (`Foo as Baz`) — the alias-resolution path
/// must key off the resolved import target, not the local binding name.
#[test]
fn renamed_import_alias_to_interface_export_equals_reports_both_verbatim() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./types\";\nexport = Baz;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_EQUALS_MUST_REFERENCE_A_VALUE,
            RESOLVES_TO_A_TYPE,
            IMPORT_MUST_BE_TYPE_ONLY,
        ],
        "expected TS1282 and TS1291 (plus the pre-existing TS1484 on the import \
         statement itself) for a renamed import-alias export= resolving to a pure \
         type under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same alias shape, but only `isolatedModules` is enabled: tsc reports only
/// TS1291 (TS1282 is verbatimModuleSyntax-only, and the import itself is not
/// separately flagged under plain isolatedModules). Negative control against
/// the same code path so the fix stays gated correctly.
#[test]
fn import_alias_to_interface_export_equals_reports_only_1291_under_isolated_modules() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE],
        "expected only TS1291 under isolatedModules (no verbatimModuleSyntax), got: {diagnostics:?}"
    );
}

/// A LOCAL interface exported via `export =` directly (no import alias in
/// between) must keep going through the pre-existing direct `check_vms_export_equals`
/// TS1282 branch only — not the alias-resolution TS1291 path, and not a
/// double report.
#[test]
fn local_interface_export_equals_reports_only_1282_verbatim() {
    let diagnostics = check(
        &[("/main.ts", "interface Foo { x: number }\nexport = Foo;\n")],
        "/main.ts",
        true,
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_VALUE],
        "expected only TS1282 for a directly-declared local interface export=, got: {diagnostics:?}"
    );
}

/// An import alias whose target has BOTH a type and a value meaning (a
/// class) must not trigger either TS1282 or TS1291.
#[test]
fn import_alias_to_class_export_equals_reports_neither_verbatim() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Bar { x = 1; }\n"),
            (
                "/main.ts",
                "import { Bar } from \"./impl\";\nexport = Bar;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_EQUALS_MUST_REFERENCE_A_VALUE || d.code == RESOLVES_TO_A_TYPE),
        "expected neither TS1282 nor TS1291 for a value-carrying import alias export=, got: {diagnostics:?}"
    );
}

/// An import alias already marked `import type` in this file suppresses
/// TS1291 (the type-only declaration is local, matching tsc's
/// `typeOnlyDeclaration` same-file check) — TS1282 still fires from the
/// pre-existing direct branch since the local symbol itself is a pure type.
#[test]
fn import_type_alias_to_interface_export_equals_suppresses_1291() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import type { Foo } from \"./types\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_VALUE],
        "expected only TS1282 (TS1291 suppressed by the local `import type`), got: {diagnostics:?}"
    );
}

/// Negative control: with neither `verbatimModuleSyntax` nor
/// `isolatedModules` enabled, `export =` of a type-only alias is legal.
#[test]
fn import_alias_to_interface_export_equals_clean_without_either_flag() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        false,
        false,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics without isolatedModules/verbatimModuleSyntax, got: {diagnostics:?}"
    );
}
