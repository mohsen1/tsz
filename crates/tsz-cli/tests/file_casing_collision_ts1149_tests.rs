//! Regression tests for TS1149
//! (`FILE_NAME_DIFFERS_FROM_ALREADY_INCLUDED_FILE_NAME_ONLY_IN_CASING`).
//!
//! Structural rule: when two root files specified for compilation resolve to
//! distinct real on-disk paths that are identical except for casing (e.g.
//! `foo.ts` and `Foo.ts` both existing and both listed as roots), `tsc`
//! reports TS1149 unconditionally — this is a portability warning, not a
//! `useCaseSensitiveFileNames` host check, so it fires the same way on a
//! case-sensitive Linux filesystem as anywhere else. The diagnostic carries
//! no source location (root files aren't reached via an import specifier to
//! anchor on) and a two-line "file is in the program because" chain, one line
//! per colliding file. Oracle-pinned against `typescript@7.0.2`.
//!
//! Owner: `crates/tsz-cli/src/driver/core_diagnostics.rs`, the root-file-list
//! casing scan that runs before source reading begins.
//!
//! Scope: this only covers casing collisions among the explicit root file
//! list. A collision introduced through an *import* specifier (tsc's
//! `Imported via "..." from file '...'` chain link) is a distinct, unclaimed
//! follow-up — it needs the module-resolution loop's specifier/importer
//! attribution, not just the root file list, and is deliberately not
//! addressed here.

use super::args::CliArgs;
use super::driver::compile;
use clap::Parser;
use std::path::Path;

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn parse_args(args: &[&str]) -> CliArgs {
    CliArgs::try_parse_from(args).expect("test args should parse")
}

/// Whether `dir`'s filesystem distinguishes file names by case.
///
/// The two collision fixtures below need `foo.ts` and `Foo.ts` to exist as two
/// distinct files. On a case-insensitive filesystem — APFS's default on macOS,
/// and Windows — the second `write_file` overwrites the first, leaving a single
/// root, so there is no casing collision and `TS1149` correctly does not fire.
/// The compiler is right and the scenario is simply unrepresentable there, so
/// those cases skip rather than assert.
///
/// Deliberately a runtime probe and not a `known-failures.txt` entry: on a
/// case-sensitive filesystem (Linux CI) these must run and pass, and a blanket
/// known-failure would mask a genuine regression there.
fn fs_is_case_sensitive(dir: &Path) -> bool {
    let lower = dir.join("tsz_case_probe.tmp");
    let upper = dir.join("TSZ_CASE_PROBE.tmp");
    if std::fs::write(&lower, "probe").is_err() {
        // Cannot tell; assume sensitive so the assertions still run rather than
        // silently skipping on an unrelated I/O problem.
        return true;
    }
    let sensitive = !upper.exists();
    let _ = std::fs::remove_file(&lower);
    let _ = std::fs::remove_file(&upper);
    sensitive
}

/// Two real, distinct root files whose names differ only in casing must
/// report TS1149, anchored nowhere (no `file`/`start`/`length`), with the
/// "Root file specified for compilation" reason repeated once per file.
#[test]
fn two_root_files_differing_only_in_casing_report_ts1149() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    if !fs_is_case_sensitive(base) {
        // See `fs_is_case_sensitive`: the two-distinct-files fixture cannot
        // exist here, so there is no collision to detect.
        return;
    }

    write_file(&base.join("foo.ts"), "export const a = 1;\n");
    write_file(&base.join("Foo.ts"), "export const b = 2;\n");

    let args = parse_args(&["tsz", "--noEmit", "foo.ts", "Foo.ts"]);
    let result = compile(&args, base).expect("compile should run");

    let casing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1149)
        .collect();
    assert_eq!(
        casing.len(),
        1,
        "expected exactly one TS1149, got: {:#?}",
        result.diagnostics
    );
    let diag = casing[0];
    assert!(
        diag.file.is_empty() && diag.start == 0 && diag.length == 0,
        "TS1149 on root files has no anchor location, got file={:?} start={} length={}",
        diag.file,
        diag.start,
        diag.length
    );
    assert!(
        diag.message_text.contains("Foo.ts") && diag.message_text.contains("foo.ts"),
        "message should name both colliding files: {}",
        diag.message_text
    );

    let reason_lines: Vec<_> = diag
        .related_information
        .iter()
        .map(|r| r.message_text.as_str())
        .collect();
    assert_eq!(
        reason_lines,
        vec![
            "The file is in the program because:",
            "Root file specified for compilation",
            "Root file specified for compilation",
        ],
        "reason chain should have one header plus one line per colliding root file"
    );
}

/// Argument order flips which file is "the new one" vs "already included",
/// matching `tsc`'s message-parameter order (whichever file is processed
/// second names itself first, points back at the first).
#[test]
fn casing_collision_message_order_follows_root_file_argument_order() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    if !fs_is_case_sensitive(base) {
        // See `fs_is_case_sensitive`: the two-distinct-files fixture cannot
        // exist here, so there is no collision to detect.
        return;
    }

    write_file(&base.join("foo.ts"), "export const a = 1;\n");
    write_file(&base.join("Foo.ts"), "export const b = 2;\n");

    let args = parse_args(&["tsz", "--noEmit", "Foo.ts", "foo.ts"]);
    let result = compile(&args, base).expect("compile should run");

    let casing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1149)
        .collect();
    assert_eq!(casing.len(), 1, "got: {:#?}", result.diagnostics);
    let message = &casing[0].message_text;
    let foo_lower_pos = message.find("foo.ts").expect("names lowercase foo.ts");
    let foo_upper_pos = message.find("Foo.ts").expect("names uppercase Foo.ts");
    assert!(
        foo_lower_pos < foo_upper_pos,
        "the second-listed root file ('foo.ts') is the one newly processed and \
         is named first ('File name X'); the first-listed ('Foo.ts') is 'already \
         included' and is named second: {message}"
    );
}

/// The exact same path listed twice (not a casing collision — it's the
/// identical file) must not report TS1149; the existing root-file dedup
/// already handles this and the casing scan must not double-count it.
#[test]
fn identical_root_path_listed_twice_does_not_report_ts1149() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(&base.join("foo.ts"), "export const a = 1;\n");

    let args = parse_args(&["tsz", "--noEmit", "foo.ts", "foo.ts"]);
    let result = compile(&args, base).expect("compile should run");

    let casing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1149)
        .collect();
    assert!(
        casing.is_empty(),
        "an exact duplicate path is not a casing collision, got: {casing:#?}"
    );
}

/// Root files with unrelated names never report TS1149 (no false positives
/// on the common case).
#[test]
fn unrelated_root_file_names_do_not_report_ts1149() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(&base.join("a.ts"), "export const a = 1;\n");
    write_file(&base.join("b.ts"), "export const b = 2;\n");

    let args = parse_args(&["tsz", "--noEmit", "a.ts", "b.ts"]);
    let result = compile(&args, base).expect("compile should run");

    let casing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1149)
        .collect();
    assert!(casing.is_empty(), "got: {casing:#?}");
}

/// A casing collision reached only through an import specifier (neither
/// colliding file is itself a root) is out of this slice's scope and must
/// stay silent rather than partially/incorrectly report — this pins the
/// documented scope boundary so a future session doesn't assume it's covered.
#[test]
fn import_discovered_casing_collision_is_out_of_scope_and_stays_silent() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path();

    write_file(&base.join("foo.ts"), "export const a = 1;\n");
    write_file(&base.join("Foo.ts"), "export const b = 2;\n");
    write_file(
        &base.join("main.ts"),
        "import { a } from './foo';\nimport { b } from './Foo';\n",
    );

    let args = parse_args(&["tsz", "--noEmit", "main.ts"]);
    let result = compile(&args, base).expect("compile should run");

    let casing: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 1149)
        .collect();
    assert!(
        casing.is_empty(),
        "import-discovered casing collisions are an unclaimed follow-up, got: {casing:#?}"
    );
}
