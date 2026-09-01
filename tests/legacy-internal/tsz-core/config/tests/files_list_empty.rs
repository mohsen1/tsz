//! TS18002 (`The 'files' list in config file '{0}' is empty.`) — the entry
//! config's own `"files": []` diagnostic from `commandLineParser.ts`'s
//! `getConfigFileSpecs`, distinct from the CLI-driver-owned TS18003 ("no
//! inputs were found") which reacts to file-discovery results instead of the
//! raw JSON shape.
//!
//! Split from `config/mod.rs` to keep each file under the 2000-line limit
//! (§19; ratchet tracked by #8280).

use super::super::*;
use tempfile::tempdir;

fn write_tsconfig(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write tsconfig");
    path
}

#[test]
fn empty_files_alone_reports_ts18002() {
    let temp = tempdir().expect("create temp dir");
    let path = write_tsconfig(temp.path(), "tsconfig.json", r#"{"files": []}"#);

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 18002),
        "expected TS18002 for bare empty files list, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_files_alongside_include_still_reports_ts18002() {
    // tsc's check is purely on the raw `files` shape; it does not consult
    // `include`, even though `include` here would find real sources.
    let temp = tempdir().expect("create temp dir");
    std::fs::write(temp.path().join("a.ts"), "export const a = 1;").expect("write a.ts");
    let path = write_tsconfig(
        temp.path(),
        "tsconfig.json",
        r#"{"files": [], "include": ["*.ts"]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 18002),
        "expected TS18002 even with a satisfying include, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_files_with_nonempty_references_does_not_report_ts18002() {
    let temp = tempdir().expect("create temp dir");
    std::fs::create_dir(temp.path().join("other")).expect("mkdir other");
    write_tsconfig(
        &temp.path().join("other"),
        "tsconfig.json",
        r#"{"files": ["b.ts"]}"#,
    );
    std::fs::write(
        temp.path().join("other").join("b.ts"),
        "export const b = 1;",
    )
    .expect("write b.ts");
    let path = write_tsconfig(
        temp.path(),
        "tsconfig.json",
        r#"{"files": [], "references": [{"path": "./other"}]}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 18002),
        "a solution-style root with non-empty references must not report TS18002, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_files_with_empty_references_still_reports_ts18002() {
    // `references: []` is the same as omitting it entirely for this check
    // (tsc: `referencesOfRaw === "no-prop" || referencesOfRaw.length === 0`).
    let temp = tempdir().expect("create temp dir");
    let path = write_tsconfig(
        temp.path(),
        "tsconfig.json",
        r#"{"files": [], "references": []}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == 18002),
        "expected TS18002 when references is an explicit empty array, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_files_with_extends_does_not_report_ts18002() {
    // The entry has its own `extends`, so tsc treats `files` as potentially
    // completed by the base and never reports TS18002 here — independent of
    // whether the base actually supplies any files.
    let temp = tempdir().expect("create temp dir");
    write_tsconfig(
        temp.path(),
        "base.json",
        r#"{"compilerOptions": {"strict": true}}"#,
    );
    let path = write_tsconfig(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./base.json", "files": []}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 18002),
        "an entry with its own extends must not report TS18002, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn nonempty_files_does_not_report_ts18002() {
    let temp = tempdir().expect("create temp dir");
    std::fs::write(temp.path().join("a.ts"), "export const a = 1;").expect("write a.ts");
    let path = write_tsconfig(temp.path(), "tsconfig.json", r#"{"files": ["a.ts"]}"#);

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 18002),
        "a non-empty files list must not report TS18002, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn absent_files_does_not_report_ts18002() {
    let temp = tempdir().expect("create temp dir");
    std::fs::write(temp.path().join("a.ts"), "export const a = 1;").expect("write a.ts");
    let path = write_tsconfig(temp.path(), "tsconfig.json", r#"{}"#);

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 18002),
        "an absent files key (undefined filesSpecs) must not report TS18002, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn empty_files_inherited_from_extends_base_when_entry_omits_files_does_not_report_ts18002() {
    // Entry omits `files` itself (so the check would look at the inherited,
    // empty value from the base) but the entry's own `extends` presence
    // alone already suppresses TS18002 — this must not regress into firing
    // once inheritance is taken into account.
    let temp = tempdir().expect("create temp dir");
    write_tsconfig(temp.path(), "base.json", r#"{"files": []}"#);
    let path = write_tsconfig(
        temp.path(),
        "tsconfig.json",
        r#"{"extends": "./base.json"}"#,
    );

    let parsed = load_tsconfig_with_diagnostics(&path).expect("load config");
    assert!(
        !parsed.diagnostics.iter().any(|d| d.code == 18002),
        "TS18002 is entry-only per tsc, even when the base's own files is empty, got: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}
