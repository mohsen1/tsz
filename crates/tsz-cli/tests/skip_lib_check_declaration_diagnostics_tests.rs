//! `skipLibCheck` declaration-preparation pass diagnostic equivalence
//! (issue #13250, Fast workstream B).
//!
//! With `skipLibCheck: true` the driver still runs the checker over each
//! declaration file to populate shared caches, but every checker diagnostic
//! from that pass is dropped. The pass now runs with
//! `diagnostics_discarded` set so the checker skips diagnostic presentation
//! work (spelling-suggestion candidate scans, failure elaboration) instead of
//! formatting diagnostics destined for the bin. These tests pin the
//! observable contract around that change:
//!
//! * `skipLibCheck: true` — semantic errors inside a local `.d.ts` stay
//!   unreported, while errors in regular `.ts` files (including suggestion
//!   diagnostics) are unaffected.
//! * `skipLibCheck: false` — the same `.d.ts` errors surface, with the
//!   "did you mean" suggestion machinery still intact.
//!
//! Binder names are varied between the two configurations so nothing keys on
//! identifier text.

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

/// Run `tsz -p tsconfig.json` over `files` with the given `skipLibCheck`
/// setting, returning combined stdout+stderr.
fn run_tsz(files: &[(&str, &str)], skip_lib_check: bool) -> String {
    let Some(tsz_bin) = find_tsz_binary() else {
        return String::from("__SKIP__");
    };
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, contents).expect("write file");
    }
    std::fs::write(
        dir.path().join("tsconfig.json"),
        format!(
            r#"{{ "compilerOptions": {{ "noEmit": true, "strict": true, "target": "esnext", "module": "esnext", "lib": ["esnext"], "moduleResolution": "bundler", "skipLibCheck": {skip_lib_check} }} }}"#
        ),
    )
    .expect("write tsconfig.json");

    let output = Command::new(tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// With `skipLibCheck: true`, a semantic error inside a local `.d.ts`
/// (an unresolved type name that would produce TS2552/TS2304) must stay
/// unreported, and user-file diagnostics — including spelling suggestions —
/// must be unaffected by the discarded-diagnostics preparation pass.
#[test]
fn skip_lib_check_drops_declaration_diagnostics_keeps_user_diagnostics() {
    let out = run_tsz(
        &[
            (
                "shapes.d.ts",
                "declare interface WideShape { edge: number }\ndeclare const fallback: WideShap;\n",
            ),
            (
                "main.ts",
                "const rounded = 1;\nexport const r = roundd;\n",
            ),
        ],
        true,
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("WideShap'"),
        "declaration-file diagnostics must be dropped under skipLibCheck, got:\n{out}"
    );
    assert!(
        !out.contains("shapes.d.ts"),
        "no diagnostic may anchor into the skipped declaration file, got:\n{out}"
    );
    assert!(
        out.contains("2552") && out.contains("rounded"),
        "user-file typo must still produce a TS2552 suggestion, got:\n{out}"
    );
}

/// With `skipLibCheck: false`, the same shape of `.d.ts` error must surface,
/// with the suggestion machinery intact — proving the discarded-diagnostics
/// gate does not leak into retained-diagnostics checking of declaration
/// files.
#[test]
fn checked_declaration_files_keep_suggestion_diagnostics() {
    let out = run_tsz(
        &[
            (
                "geometry.d.ts",
                "declare interface NarrowBox { side: number }\ndeclare const primary: NarrowBo;\n",
            ),
            ("entry.ts", "export const ok: number = 1;\n"),
        ],
        false,
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("2552") && out.contains("NarrowBox"),
        "without skipLibCheck the declaration-file typo must surface as TS2552 \
         with a suggestion, got:\n{out}"
    );
}
