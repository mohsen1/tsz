//! Regression (#16702): a string-literal export **local** name without a `from`
//! clause (`export { "q" as y }`) is TS1003, but `tsc` reports it from the
//! *checker* (`checkExportSpecifier` -> `checkModuleExportName`), so it is a
//! semantic diagnostic. `tsc`'s batch compiler collects semantic diagnostics
//! only when the whole program produced no *syntactic* diagnostics, so a real
//! parse error anywhere in the program suppresses every one of these export-side
//! TS1003s program-wide.
//!
//! tsz used to emit this diagnostic from the parser too
//! (`report_export_specifier_string_local_names`), which mis-classified it as
//! syntactic: it survived the program-wide suppression, and — being a parse
//! diagnostic — it also made an otherwise-clean file count as having a "real
//! syntax error". The witness is `conformance/es2022/`
//! `arbitraryModuleNamespaceIdentifiers/arbitraryModuleNamespaceIdentifiers_syntax.ts`,
//! where the import-side binding errors (genuinely syntactic) suppressed the
//! export-side ones in `tsc` (9 TS1003) but not in tsz (12 TS1003). These tests
//! pin the **count**, since the expected and actual code *sets* are identical.
//!
//! Binder and string names are varied across the rows so the fix cannot be a
//! name-scoped path.

use std::path::PathBuf;
use std::process::Command;

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

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "module": "esnext",
    "target": "es2022",
    "lib": ["es2022"],
    "types": [],
    "noEmit": true
  }
}
"#;

/// Run `tsz -p tsconfig.json` over an in-memory project, returning the combined
/// stdout+stderr. Returns `None` (after the caller's `let else`) when the `tsz`
/// binary is unavailable.
fn run_project(files: &[(&str, &str)]) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write file");
    }
    std::fs::write(dir.path().join("tsconfig.json"), TSCONFIG).expect("write tsconfig");
    let output = Command::new(tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(combined)
}

fn count_code(out: &str, code: &str) -> usize {
    let needle = format!("error {code}:");
    out.matches(&needle).count()
}

/// The reduced witness: three import-side binding errors (syntactic) plus one
/// export-side string-local name (semantic). `tsc` reports exactly the three
/// import-side ones; the export-side one is suppressed program-wide by the
/// syntactic errors. This is the `values-*` slice of the conformance fixture.
#[test]
fn export_side_ts1003_is_suppressed_by_a_sibling_parse_error() {
    let Some(out) = run_project(&[
        (
            "values-valid.ts",
            "export const foo = 123;\nexport { foo as \"valid 1\" };\n",
        ),
        (
            "values-bad-import.ts",
            "import { foo as \"invalid 2\" } from \"./values-valid\";\n",
        ),
        ("values-bad-export.ts", "export { \"invalid 3\" as baz };\n"),
        (
            "values-no-as.ts",
            "import { \"invalid 1\" } from \"./values-valid\";\n",
        ),
        (
            "values-type-as.ts",
            "import { type as \"invalid 4\" } from \"./values-valid\";\n",
        ),
    ]) else {
        return;
    };
    assert_eq!(
        count_code(&out, "TS1003"),
        3,
        "only the three import-side (syntactic) TS1003s survive; the export-side \
         (semantic) one is suppressed program-wide.\noutput:\n{out}"
    );
}

/// Isolation control: the very same export-side construct, alone, still reports.
/// With no syntactic error anywhere in the program the semantic check fires.
#[test]
fn export_side_ts1003_still_reports_in_isolation() {
    let Some(out) = run_project(&[("only.ts", "export { \"lonelyName\" as reExported };\n")])
    else {
        return;
    };
    assert_eq!(
        count_code(&out, "TS1003"),
        1,
        "with nothing syntactic to suppress it, the export-side TS1003 must report.\noutput:\n{out}"
    );
}

/// A file whose *only* content is the export-side construct must NOT be treated
/// as having a real syntax error: an unrelated semantic error in a sibling file
/// must still surface. Before the fix the parser-emitted TS1003 made this file
/// count as syntactically broken and silenced the sibling's TS2322 program-wide.
#[test]
fn an_export_side_construct_does_not_suppress_a_sibling_semantic_error() {
    let Some(out) = run_project(&[
        ("names.ts", "export { \"anExportName\" as local };\n"),
        ("other.ts", "const n: number = \"not a number\";\n"),
    ]) else {
        return;
    };
    assert_eq!(
        count_code(&out, "TS1003"),
        1,
        "the export-side TS1003 must still report on its own file.\noutput:\n{out}"
    );
    assert_eq!(
        count_code(&out, "TS2322"),
        1,
        "the export-side construct is not a syntax error, so a sibling's semantic \
         diagnostic must not be suppressed.\noutput:\n{out}"
    );
}

/// Renamed-binder / anti-hardcoding twin of the suppression row: the same
/// structural shape with entirely different identifiers behaves identically.
#[test]
fn export_side_ts1003_suppression_is_structural_not_name_scoped() {
    let Some(out) = run_project(&[
        ("mod-alpha.ts", "export { \"someString\" as reexport };\n"),
        // `let x: = 1;` is a genuine syntactic error (TS1110) in a sibling file.
        ("mod-beta.ts", "let placeholder: = 1;\n"),
    ]) else {
        return;
    };
    assert_eq!(
        count_code(&out, "TS1003"),
        0,
        "a sibling parse error suppresses the export-side TS1003 regardless of the \
         names involved.\noutput:\n{out}"
    );
    assert_eq!(
        count_code(&out, "TS1110"),
        1,
        "the sibling's syntactic error itself must still report.\noutput:\n{out}"
    );
}
