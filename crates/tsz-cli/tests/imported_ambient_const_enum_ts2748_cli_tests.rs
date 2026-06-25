//! TS2748 ("Cannot access ambient const enums when `isolatedModules` is
//! enabled") for *imported* ambient const enums (#14811).
//!
//! Structural rule: ambient-ness of a const enum is a property of the file that
//! *declares* it, not the file that *accesses* it. `is_const_enum_ambient`
//! previously resolved each declaration's ambient context against the importing
//! file's arena, so a cross-file imported const enum looked non-ambient and the
//! access-site TS2748 gate was skipped (false negative). The check now resolves
//! the declaration against the arena of the file that owns the symbol.
//!
//! tsc's `checkConstEnumAccess` gates the access-site diagnostic on
//! `rawIsolatedModules || (verbatimModuleSyntax && firstId-is-not-an-alias)`.
//! Under `verbatimModuleSyntax` alone an imported const enum is reported once at
//! the import statement instead, so the access site stays silent for imports; a
//! locally-declared const enum (no alias) is still reported at the access site.
//! Raw `isolatedModules` always reports at the access site.
//!
//! These run through the real multi-file driver (`crate::driver::compile`) so
//! cross-file const enum member resolution and the production option fan-out
//! (`verbatimModuleSyntax -> isolatedModules`) are exercised on the real path.
//! Binder and file names are varied so the behaviour follows structure, not
//! identifier text.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

const TS2748: u32 = 2748;

/// Compile `files` (written into one temp dir) with the given extra flags and
/// root-file order.
fn compile(files: &[(&str, &str)], extra_flags: &[&str], roots: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "--lib",
        "es2022",
    ];
    argv.extend_from_slice(extra_flags);
    argv.extend_from_slice(roots);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn count_ts2748(diagnostics: &[Diagnostic]) -> usize {
    diagnostics.iter().filter(|d| d.code == TS2748).count()
}

/// 1-based line numbers of every TS2748 anchored in the file named `basename`,
/// derived from the diagnostic `start` offset against the known `content`.
fn ts2748_lines_in(diagnostics: &[Diagnostic], basename: &str, content: &str) -> Vec<u32> {
    let mut lines: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == TS2748 && d.file.ends_with(basename))
        .map(|d| {
            content[..d.start as usize]
                .bytes()
                .filter(|&b| b == b'\n')
                .count() as u32
                + 1
        })
        .collect();
    lines.sort_unstable();
    lines
}

/// The consuming module used by the verbatim/both-flag placement cases: the
/// import is on line 1, the const-enum access on line 2.
const MAIN_IMPORTS_COLOR: &str = "import { Color } from './en';\nconst c = Color.Red;\n";

// ---------------------------------------------------------------------------
// isolatedModules: access-site diagnostic, imported or local.
// ---------------------------------------------------------------------------

#[test]
fn iso_imported_declare_const_enum_from_dts_flags_access_site() {
    // The reported repro.
    let diagnostics = compile(
        &[
            (
                "en.d.ts",
                "export declare const enum Color { Red, Green, Blue }\n",
            ),
            (
                "main.ts",
                "import { Color } from './en';\nconst c = Color.Red;\n",
            ),
        ],
        &["--isolatedModules"],
        &["en.d.ts", "main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        1,
        "expected one TS2748 for imported ambient const enum access, got: {diagnostics:?}"
    );
}

#[test]
fn iso_imported_declare_const_enum_from_regular_ts_flags_access_site() {
    // `declare const enum` is ambient even in a regular `.ts` file; the ambient
    // check must walk the source arena, not just the `.d.ts` shortcut.
    let diagnostics = compile(
        &[
            (
                "shades.ts",
                "export declare const enum Shade { Dark, Light }\n",
            ),
            (
                "main.ts",
                "import { Shade } from './shades';\nconst s = Shade.Dark;\n",
            ),
        ],
        &["--isolatedModules"],
        &["shades.ts", "main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        1,
        "expected one TS2748 for imported `declare const enum` from a regular .ts, got: {diagnostics:?}"
    );
}

#[test]
fn iso_imported_plain_const_enum_from_dts_flags_access_site() {
    // Inside a `.d.ts` every declaration is implicitly ambient, so a `const enum`
    // without an explicit `declare` is still ambient when imported.
    let diagnostics = compile(
        &[
            ("hue.d.ts", "export const enum Hue { Warm, Cool }\n"),
            (
                "main.ts",
                "import { Hue } from './hue';\nconst h = Hue.Warm;\n",
            ),
        ],
        &["--isolatedModules"],
        &["hue.d.ts", "main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        1,
        "expected one TS2748 for imported implicit-ambient const enum, got: {diagnostics:?}"
    );
}

#[test]
fn iso_local_declare_const_enum_still_flags_access_site() {
    // Control: the local case already worked; the cross-file fix must not regress it.
    let diagnostics = compile(
        &[(
            "main.ts",
            "export {};\ndeclare const enum Tone { Soft, Loud }\nconst t = Tone.Soft;\n",
        )],
        &["--isolatedModules"],
        &["main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        1,
        "expected one TS2748 for local ambient const enum access, got: {diagnostics:?}"
    );
}

#[test]
fn imported_const_enum_without_flags_is_clean() {
    // No over-correction: without isolatedModules / verbatimModuleSyntax the
    // access is allowed.
    let diagnostics = compile(
        &[
            (
                "en.d.ts",
                "export declare const enum Color { Red, Green, Blue }\n",
            ),
            (
                "main.ts",
                "import { Color } from './en';\nconst c = Color.Red;\n",
            ),
        ],
        &[],
        &["en.d.ts", "main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        0,
        "expected no TS2748 without isolatedModules, got: {diagnostics:?}"
    );
}

#[test]
fn imported_non_const_ambient_enum_is_not_flagged() {
    // No over-correction: TS2748 is specific to *const* enums.
    let diagnostics = compile(
        &[
            (
                "dir.d.ts",
                "export declare enum Direction { North, South }\n",
            ),
            (
                "main.ts",
                "import { Direction } from './dir';\nconst d = Direction.North;\n",
            ),
        ],
        &["--isolatedModules"],
        &["dir.d.ts", "main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        0,
        "expected no TS2748 for a non-const ambient enum, got: {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// verbatimModuleSyntax: imported -> import site only; local -> access site.
// ---------------------------------------------------------------------------

#[test]
fn verbatim_imported_const_enum_reports_only_at_import_site() {
    // tsc reports the imported const enum once, at the import statement (line 1),
    // and stays silent at the access expression (line 2).
    let diagnostics = compile(
        &[
            (
                "en.d.ts",
                "export declare const enum Color { Red, Green, Blue }\n",
            ),
            ("main.ts", MAIN_IMPORTS_COLOR),
        ],
        &["--verbatimModuleSyntax"],
        &["en.d.ts", "main.ts"],
    );
    assert_eq!(
        ts2748_lines_in(&diagnostics, "main.ts", MAIN_IMPORTS_COLOR),
        vec![1],
        "expected a single import-site TS2748 (line 1) under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

#[test]
fn verbatim_local_const_enum_reports_at_access_site() {
    // A local const enum has no import alias, so it is reported at the access site.
    let diagnostics = compile(
        &[(
            "main.ts",
            "export {};\ndeclare const enum Tone { Soft, Loud }\nconst t = Tone.Soft;\n",
        )],
        &["--verbatimModuleSyntax"],
        &["main.ts"],
    );
    assert_eq!(
        count_ts2748(&diagnostics),
        1,
        "expected one access-site TS2748 for a local const enum under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

#[test]
fn both_flags_imported_reports_at_both_import_and_access_sites() {
    // With raw `isolatedModules` *and* `verbatimModuleSyntax`, tsc reports the
    // imported const enum at both the import statement (line 1, verbatim) and the
    // access expression (line 2, raw isolatedModules).
    let diagnostics = compile(
        &[
            (
                "en.d.ts",
                "export declare const enum Color { Red, Green, Blue }\n",
            ),
            ("main.ts", MAIN_IMPORTS_COLOR),
        ],
        &["--isolatedModules", "--verbatimModuleSyntax"],
        &["en.d.ts", "main.ts"],
    );
    assert_eq!(
        ts2748_lines_in(&diagnostics, "main.ts", MAIN_IMPORTS_COLOR),
        vec![1, 2],
        "expected import-site (line 1) and access-site (line 2) TS2748 with both flags, got: {diagnostics:?}"
    );
}
