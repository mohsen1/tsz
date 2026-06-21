//! Regression tests for `export { local as Name }` where `Name` collides with a
//! distinct local declaration.
//!
//! Structural rule: a renamed export specifier (`export { orig as exp }`)
//! contributes only to the module's public export surface. tsc keeps any
//! in-module declaration named `exp` (for example a same-named `class`) intact
//! for value and type references and re-aliases solely at the export boundary.
//! tsz previously overwrote the scope / file-local slot for `exp` with the alias
//! source, producing spurious `TS2749` at in-module type sites and `TS2552` at
//! value sites, while cross-file `import { exp }` still had to resolve to the
//! alias target. Mined from purify-ts (`Either.ts` / `Maybe.ts`).

use super::super::core::*;

/// `export { box as Box }` next to a local `class Box`: in-module value/type
/// references to `Box` must keep resolving to the class, not the lowercased
/// alias source. No `TS2749` / `TS2552`.
#[test]
fn test_export_alias_does_not_clobber_same_named_local_class() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Box { constructor(public value: number) {} }
const useType = (b: Box): number => b.value;
const useValue = (): Box => new Box(1);
const box = (n: number) => new Box(n);
export { box as Box };
"#,
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2749 || *code == 2552),
        "Expected no TS2749/TS2552 from an export-alias colliding with a local class.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics.\nActual: {diagnostics:#?}"
    );
}

/// Renamed binders: the same collision over a local `function`, with a different
/// identifier, must behave identically (anti-hardcoding: not keyed on `Box`).
#[test]
fn test_export_alias_does_not_clobber_same_named_local_function() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function Widget(input: number): number { return input; }
const useValue = (): number => Widget(1);
const make = (n: number) => n;
export { make as Widget };
"#,
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    // `Widget` in `Widget(1)` resolves to the local function (returns number),
    // never to the lowercased `make`. No TS2552 / TS2304.
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == 2552 || *code == 2304),
        "Expected no TS2552/TS2304 from an export-alias colliding with a local function.\nActual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics.\nActual: {diagnostics:#?}"
    );
}

/// The module's public export surface must map the *alias name* (`Box`) to the
/// alias source, never the local declaration name. Cross-file:
/// - `import { Box }` resolves (the renamed public export);
/// - `import { box }` is `TS2460` (the local is exported only under `Box`);
/// - `import { Nonexistent }` is `TS2305`.
///
/// This proves the fix seeds `module_exports[Box] -> box` without leaking `box`
/// itself into the public surface or clobbering the producer's `class Box`.
#[test]
fn test_export_alias_cross_file_resolves_to_alias_target() {
    let files = [
        (
            "/m.ts",
            r#"
class Box { constructor(public value: number) {} }
const useType = (b: Box): number => b.value;
const useValue = (): Box => new Box(1);
const box = (n: number) => new Box(n);
export { box as Box };
"#,
        ),
        (
            "/consumer.ts",
            r#"
import { Box } from "./m";
import { box } from "./m";
import { Nonexistent } from "./m";
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options_and_import_reporting(
        &files,
        "/consumer.ts",
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
        true,
    );

    // `import { box }` -> TS2460: the local is exported only under the alias `Box`.
    assert!(
        diagnostics
            .iter()
            .any(|(code, msg)| *code == 2460 && msg.contains("'box'") && msg.contains("'Box'")),
        "Expected TS2460 for `import {{ box }}` (exported only as `Box`).\nActual: {diagnostics:#?}"
    );
    // `import { Nonexistent }` -> TS2305.
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2305),
        "Expected TS2305 for a name that is not exported.\nActual: {diagnostics:#?}"
    );
    // `import { Box }` must resolve cleanly: no TS2305/TS2460 mentioning `Box`
    // as the *missing/renamed* member.
    assert!(
        !diagnostics
            .iter()
            .any(|(code, msg)| *code == 2305 && msg.contains("'Box'")),
        "Expected `import {{ Box }}` to resolve to the alias target.\nActual: {diagnostics:#?}"
    );
}

/// A renamed export with no colliding local must keep resolving cross-file
/// (guards against the fix dropping the public mapping entirely).
#[test]
fn test_renamed_export_without_collision_still_resolves_cross_file() {
    let files = [
        (
            "/m.ts",
            r#"
const internalName = (n: number) => n + 1;
export { internalName as publicName };
"#,
        ),
        (
            "/consumer.ts",
            r#"
import { publicName } from "./m";
const r: number = publicName(3);
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/consumer.ts",
        CheckerOptions {
            strict: true,
            module: ModuleKind::ES2020,
            target: ScriptTarget::ES2020,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for a non-colliding renamed export.\nActual: {diagnostics:#?}"
    );
}
