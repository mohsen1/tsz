//! Cross-file symbol-identity guards for **imported string-literal consts used
//! as computed property keys**.
//!
//! Per-file binders number their `SymbolId`s independently (each file starts at
//! `0`), so an import-alias symbol in the consuming file can share a raw
//! `SymbolId` with an unrelated `export const` in the providing file. When a
//! computed property key `[K]` references such an imported const, the checker
//! must resolve `K` to *its own* target export's literal value. If the raw-id
//! collision leaks, `[K]` resolves to a *different* export that merely shares
//! the alias's raw id — so several distinct keys collapse onto one name and the
//! object literal draws a spurious **TS1117** ("An object literal cannot have
//! multiple properties with the same name"), or the wrong literal name surfaces
//! as a spurious **TS2353** (excess property) / **TS2339** (missing property).
//!
//! The real multi-file driver keeps the two declarations distinct (per-file
//! checker contexts plus a `(SymbolId, file_idx)`-keyed cross-file cache), so
//! `tsc` and the production `tsz` driver are clean on every positive case here.
//! The in-crate `check_multi_file_with_libs` checker harness resolves every file
//! through a single context whose `symbol_types` cache is keyed by the raw
//! `SymbolId`, so it conflates the colliding ids and cannot host this guard —
//! hence the real multi-module driver test (`crate::driver::compile`), matching
//! the sibling `cross_file_local_callee_symbol_identity_tests`.
//!
//! This is a regression floor for the cross-arena identity work (#14344) and the
//! `green`/`hold` immer row (#13942 FP#1: an imported string-literal const used
//! as a computed key not resolving to its literal name across files). The
//! collision only ever surfaced inside a function body, where flow analysis
//! re-resolves the computed-key reference through the raw-id path, so each guard
//! places the object literal inside a function.
//!
//! Binder and file names are varied across cases so the behaviour follows
//! structure, not identifier text. Each positive case is checked in both
//! root-file orders so the result cannot depend on which file the driver happens
//! to check first.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) with the given root-file order.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
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
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

/// Diagnostic codes that signal the computed-key identity collision leaking: a
/// spurious duplicate (TS1117), or the wrong resolved literal name surfacing as
/// excess (TS2353) / missing (TS2339) against a contextual type.
fn computed_key_collision_codes(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| matches!(d.code, 1117 | 2353 | 2339))
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Assert the repro is clean (no collision diagnostics) in both root-file orders
/// (consumer-first is the cross-file regression direction).
fn assert_clean_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = computed_key_collision_codes(&compile_in_order(files, &names));
    assert!(
        forward.is_empty(),
        "expected no computed-key identity-collision diagnostics in order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = computed_key_collision_codes(&compile_in_order(files, &reversed));
    assert!(
        backward.is_empty(),
        "expected no computed-key identity-collision diagnostics in order {reversed:?}, got: {backward:?}"
    );
}

/// Three distinct imported string-literal consts used as computed keys inside a
/// function body must each resolve to their own literal name — no spurious
/// TS1117 from a raw-id collision collapsing them onto one name.
#[test]
fn distinct_imported_const_computed_keys_stay_distinct() {
    assert_clean_both_orders(&[
        (
            "keys.ts",
            "export const RED = 'red';\n\
             export const GREEN = 'green';\n\
             export const BLUE = 'blue';\n",
        ),
        (
            "palette.ts",
            "import { RED, GREEN, BLUE } from './keys';\n\
             function build() {\n\
             \x20 return { [RED]: 1, [GREEN]: 2, [BLUE]: 3 };\n\
             }\n\
             export {};\n",
        ),
    ]);
}

/// Same shape, different binder and file names — proves the rule follows
/// structure, not identifier text.
#[test]
fn distinct_imported_const_computed_keys_renamed_binders() {
    assert_clean_both_orders(&[
        (
            "tokens.ts",
            "export const ALPHA = 'alpha';\n\
             export const BETA = 'beta';\n\
             export const GAMMA = 'gamma';\n",
        ),
        (
            "registry.ts",
            "import { ALPHA, BETA, GAMMA } from './tokens';\n\
             function assemble() {\n\
             \x20 return { [ALPHA]: true, [BETA]: false, [GAMMA]: true };\n\
             }\n\
             export {};\n",
        ),
    ]);
}

/// The collision originally surfaced when the object literal was contextually
/// typed (e.g. a function return type whose properties match the resolved key
/// names). The literal names must resolve correctly so they neither collapse
/// (TS1117) nor read as excess/missing against the contextual interface.
#[test]
fn imported_const_computed_keys_under_contextual_return_type() {
    assert_clean_both_orders(&[
        (
            "names.ts",
            "export const WIDTH = 'width';\n\
             export const HEIGHT = 'height';\n\
             export const DEPTH = 'depth';\n",
        ),
        (
            "shape.ts",
            "import { WIDTH, HEIGHT, DEPTH } from './names';\n\
             interface Dimensions { width?: number; height?: number; depth?: number; }\n\
             function dims(): Dimensions {\n\
             \x20 return { [WIDTH]: 1, [HEIGHT]: 2, [DEPTH]: 3 };\n\
             }\n\
             export {};\n",
        ),
    ]);
}

/// A re-export hop between the const declarations and the consuming file must
/// not change the result: each computed key still resolves to its own literal.
#[test]
fn imported_const_computed_keys_through_reexport_hop() {
    assert_clean_both_orders(&[
        (
            "origin.ts",
            "export const FIRST = 'first';\n\
             export const SECOND = 'second';\n\
             export const THIRD = 'third';\n",
        ),
        ("barrel.ts", "export * from './origin';\n"),
        (
            "site.ts",
            "import { FIRST, SECOND, THIRD } from './barrel';\n\
             function rec() {\n\
             \x20 return { [FIRST]: 'a', [SECOND]: 'b', [THIRD]: 'c' };\n\
             }\n\
             export {};\n",
        ),
    ]);
}

/// Negative control: a genuine duplicate (plain string-literal keys) must still
/// report TS1117. The cross-file guard must not blanket-suppress the real check.
#[test]
fn genuine_plain_string_duplicate_still_reports_ts1117() {
    let files = &[("dup.ts", "const o = { 'k': 1, 'k': 2 };\nexport {};\n")];
    let diagnostics = compile_in_order(files, &["dup.ts"]);
    assert!(
        diagnostics.iter().any(|d| d.code == 1117),
        "a genuine duplicate property name must still report TS1117; got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

/// Negative control: the *same* imported const used twice as a computed key is a
/// genuine duplicate (both resolve to the same literal name) and must still
/// report TS1117 — proving the duplicate check stays live for computed keys.
#[test]
fn same_imported_const_used_twice_still_reports_ts1117() {
    let files = &[
        ("k.ts", "export const KEY = 'key';\n"),
        (
            "twice.ts",
            "import { KEY } from './k';\n\
             const o = { [KEY]: 1, [KEY]: 2 };\n\
             export {};\n",
        ),
    ];
    let diagnostics = compile_in_order(files, &["twice.ts", "k.ts"]);
    assert!(
        diagnostics.iter().any(|d| d.code == 1117),
        "duplicate computed keys resolving to the same literal must still report TS1117; got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
