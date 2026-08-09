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
//!
//! Also covers TS1289, TS1291's sibling in the same `checkExportAssignment`
//! branch pair: where TS1291 fires when the alias resolves to NO value at
//! all anywhere in its chain, TS1289 fires when the alias resolves to a
//! REAL value overall but the chain crosses an explicit `import type`/
//! `export type` boundary in a file other than this one (oracle-verified:
//! `import { Foo } from "./reexport"; export = Foo;` where
//! `reexport.ts` does `export type { Foo } from "./impl";` and `impl.ts`
//! does `export class Foo {}`). tsc's own gate: `getTypeOnlyAliasDeclarationEx`
//! finds a type-only alias declaration in the chain, and it is not in the
//! current file. Same double-report pattern applies with TS1283.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_EQUALS_MUST_REFERENCE_A_VALUE: u32 = 1282;
const EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE: u32 = 1283;
const IMPORT_MUST_BE_TYPE_ONLY: u32 = 1484;
const IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION: u32 = 1485;
const RESOLVES_TO_A_TYPE: u32 = 1291;
const RESOLVES_TO_A_TYPE_ONLY_DECLARATION: u32 = 1289;

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

/// An import alias whose chain crosses an explicit `export type { ... }`
/// re-export boundary, but whose ultimate declaration is a real value
/// (a class), reports TS1283 + TS1289 under `verbatimModuleSyntax` (plus
/// the pre-existing TS1485 on the local import statement — the chain
/// variant of TS1484, since the type-only marking here comes from the
/// `reexport.ts` hop rather than `Foo` itself being declared type-only).
#[test]
fn import_alias_through_export_type_reexport_to_class_reports_1283_and_1289_verbatim() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE,
            RESOLVES_TO_A_TYPE_ONLY_DECLARATION,
            IMPORT_RESOLVES_TO_TYPE_ONLY_DECLARATION,
        ],
        "expected TS1283 and TS1289 (plus the pre-existing TS1485 on the import \
         statement itself) for an import alias crossing an `export type` \
         re-export boundary under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same shape, but only `isolatedModules` is enabled: tsc reports only
/// TS1289 (TS1283 is verbatimModuleSyntax-only, matching TS1291's own
/// isolatedModules-only sibling test above).
#[test]
fn import_alias_through_export_type_reexport_to_class_reports_only_1289_under_isolated_modules() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE_ONLY_DECLARATION],
        "expected only TS1289 under isolatedModules (no verbatimModuleSyntax), got: {diagnostics:?}"
    );
}

/// Renamed on import (`Foo as Baz`) — the chain lookup must key off the
/// resolved import target/name, not the local binding name.
#[test]
fn renamed_import_alias_through_export_type_reexport_reports_1289() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export type { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./reexport\";\nexport = Baz;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE_ONLY_DECLARATION],
        "expected only TS1289 for a renamed import through an `export type` \
         re-export boundary, got: {diagnostics:?}"
    );
}

/// Negative control: an import alias to a class with NO type-only boundary
/// anywhere in the chain (plain `export { Foo }` re-export) must not
/// trigger TS1289 — this is the already-covered
/// `import_alias_to_class_export_equals_reports_neither_verbatim` shape,
/// just routed through an intermediate re-exporting file to isolate the
/// "boundary exists" condition from "value exists".
#[test]
fn import_alias_through_plain_reexport_to_class_reports_neither() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Foo {}\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        true,
        false,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_EQUALS_MUST_REFERENCE_A_VALUE
                || d.code == EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE
                || d.code == RESOLVES_TO_A_TYPE
                || d.code == RESOLVES_TO_A_TYPE_ONLY_DECLARATION),
        "expected no TS1282/1283/1289/1291 for a value-carrying alias through a \
         plain re-export with no type-only boundary, got: {diagnostics:?}"
    );
}

/// An import alias whose target has NO value anywhere (a pure interface),
/// reached through a *plain* (non-type-only) re-export hop rather than a
/// direct import, must still report TS1282 + TS1291. `lookup_imported_target_flags`
/// previously stopped at the local re-export ALIAS symbol that
/// `resolve_export_in_file`'s exports-table branch returns for
/// `export { Foo } from "./impl"` in `reexport.ts` — that alias carries
/// `EXPORT_VALUE` unconditionally (it just marks "this name is exported",
/// not "this name is a value") and never copies the target's `TYPE` flag, so
/// the multi-hop case silently read `(has_type: false, has_value: false)`
/// and neither the TS1291 nor the TS1289 condition ever fired. tsz-org/tsz#17098.
///
/// tsc additionally reports TS1484 on the import statement itself here
/// (`'Foo' is a type...`, oracle-verified against `typescript@7.0.2`): the
/// TS1484-vs-TS1485 picker in `check_verbatim_module_syntax_imports` follows
/// the full re-export chain to the pure-type target (no runtime value), so a
/// type reached through a plain re-export hop is TS1484 — not TS1485, which
/// is reserved for a *value* target reached across an explicit type-only
/// boundary. tsz-org/tsz#17098.
#[test]
fn import_alias_through_plain_reexport_to_interface_reports_1282_1291_and_1484_verbatim() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo { x: number }\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
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
        "expected TS1282, TS1291 and TS1484 for an import alias resolving to a \
         pure type through a plain (non-type-only) re-export hop under \
         verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same shape, but only `isolatedModules` is enabled: tsc reports only
/// TS1291 (TS1282 is verbatimModuleSyntax-only), matching the direct-import
/// sibling test above.
#[test]
fn import_alias_through_plain_reexport_to_interface_reports_only_1291_under_isolated_modules() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo { x: number }\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE],
        "expected only TS1291 under isolatedModules through a plain re-export \
         hop, got: {diagnostics:?}"
    );
}

/// Renamed on import (`Foo as Baz`) through the plain re-export hop — the
/// alias-chain follow must key off the resolved import target/name at each
/// hop, not the local binding name.
#[test]
fn renamed_import_alias_through_plain_reexport_to_interface_reports_1291() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo { x: number }\n"),
            ("/reexport.ts", "export { Foo } from \"./impl\";\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./reexport\";\nexport = Baz;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE],
        "expected only TS1291 for a renamed import through a plain re-export \
         hop, got: {diagnostics:?}"
    );
}

/// A THREE-hop chain (`impl.ts` -> `mid.ts` -> `reexport.ts` -> `main.ts`),
/// all plain re-exports, still reaches the pure-type target — the alias
/// chain follow must not stop after one extra hop.
#[test]
fn import_alias_through_two_plain_reexport_hops_to_interface_reports_1291() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export interface Foo { x: number }\n"),
            ("/mid.ts", "export { Foo } from \"./impl\";\n"),
            ("/reexport.ts", "export { Foo } from \"./mid\";\n"),
            (
                "/main.ts",
                "import { Foo } from \"./reexport\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
        false,
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE],
        "expected only TS1291 through a two-hop plain re-export chain, got: {diagnostics:?}"
    );
}
