//! End-to-end regression for issue #14855 — TS6053 ("File '...' not found.")
//! for an unresolved `/// <reference path="..." />` must report the reference
//! path exactly as written in the source, matching `tsc` 7.0.2 (which dropped
//! the 6.x cwd-relative/resolved-path display). tsz previously interpolated
//! the resolved *absolute* path into the message.
//!
//! These tests run the real binary in a subprocess with `current_dir` set to a
//! temp project, exercising the full driver → checker wiring that supplies the
//! diagnostic's display path.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_ref_not_found_rel_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, contents).expect("write repro file");
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Run the entry file through the real binary from `cwd`, returning combined
/// stdout+stderr.
fn run_from(cwd: &Path, entry: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(tsz_bin)
        .args([
            entry, "--noEmit", "--strict", "--target", "es2022", "--lib", "es2022", "--pretty",
            "false",
        ])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run tsz");
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn run_project_from(cwd: &Path) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let output = Command::new(tsz_bin)
        .args(["--project", ".", "--noEmit", "--pretty", "false"])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run tsz project");
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Sibling / parent / subdirectory reference paths each resolve relative to the
/// current directory — never an absolute path.
#[test]
fn unresolved_reference_paths_are_reported_relative_to_cwd() {
    let temp = TempDir::new("siblings").expect("temp dir");
    // Distinct, non-trivial file names so a name-specific shortcut could not pass.
    write_file(
        &temp.path.join("entry.ts"),
        "/// <reference path=\"absent-sibling.d.ts\" />\n\
         /// <reference path=\"../escaped-parent.d.ts\" />\n\
         /// <reference path=\"vendor/absent-deep.d.ts\" />\n\
         export const x = 1;\n",
    );

    let Some(out) = run_from(&temp.path, "entry.ts") else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        out.contains("File 'absent-sibling.d.ts' not found."),
        "sibling reference must be cwd-relative; got:\n{out}"
    );
    assert!(
        out.contains("File '../escaped-parent.d.ts' not found."),
        "parent-escaping reference must keep a single `../`; got:\n{out}"
    );
    assert!(
        out.contains("File 'vendor/absent-deep.d.ts' not found."),
        "subdirectory reference must be cwd-relative; got:\n{out}"
    );
    // The defining property: no absolute path leaks into the message.
    assert!(
        !out.contains(&format!("File '{}", temp.path.display())),
        "TS6053 must not contain an absolute path; got:\n{out}"
    );
}

/// `tsc --project` prints the same as-written reference path as explicit-file
/// mode: tsc 7.0.2 no longer substitutes the resolved (absolute) path that
/// 6.x project mode used.
#[test]
fn project_mode_reports_reference_path_as_written() {
    let temp = TempDir::new("project").expect("temp dir");
    write_file(
        &temp.path.join("pkg/entry.ts"),
        "/// <reference path=\"../escaped-parent.d.ts\" />\n\
         export const z = 3;\n",
    );
    write_file(
        &temp.path.join("tsconfig.json"),
        r#"{ "compilerOptions": { "target": "es2022", "lib": ["es2022"] }, "include": ["**/*.ts"] }"#,
    );

    let Some(out) = run_project_from(&temp.path) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        out.contains("File '../escaped-parent.d.ts' not found."),
        "project-mode reference must print the path as written in the source; got:\n{out}"
    );
    assert!(
        !out.contains(&format!("File '{}", temp.path.display())),
        "TS6053 must not contain an absolute path; got:\n{out}"
    );
}

/// When the entry file sits in a subdirectory, tsc 7.0.2 still prints the
/// sibling reference exactly as written — no subdirectory prefix is added.
#[test]
fn reference_from_nested_entry_prints_path_as_written() {
    let temp = TempDir::new("nested").expect("temp dir");
    write_file(
        &temp.path.join("pkg/entry.ts"),
        "/// <reference path=\"absent-neighbor.d.ts\" />\n\
         export const y = 2;\n",
    );

    let Some(out) = run_from(&temp.path, "pkg/entry.ts") else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        out.contains("File 'absent-neighbor.d.ts' not found."),
        "reference from a nested entry must print the path as written; got:\n{out}"
    );
    assert!(
        !out.contains("File 'pkg/absent-neighbor.d.ts' not found."),
        "the 6.x subdir-prefixed display must not come back; got:\n{out}"
    );
    assert!(
        !out.contains(&format!("File '{}", temp.path.display())),
        "TS6053 must not contain an absolute path; got:\n{out}"
    );
}

#[test]
fn explicit_file_absolute_reference_literal_stays_absolute() {
    let temp = TempDir::new("absolute_literal").expect("temp dir");
    let missing = temp.path.join("absent-absolute.d.ts");
    let missing_display = missing.to_string_lossy().replace('\\', "/");
    write_file(
        &temp.path.join("entry.ts"),
        &format!("/// <reference path=\"{missing_display}\" />\nexport const q = 4;\n"),
    );

    let Some(out) = run_from(&temp.path, "entry.ts") else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        out.contains(&format!("File '{missing_display}' not found.")),
        "absolute reference literals must remain absolute; got:\n{out}"
    );
}
