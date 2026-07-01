//! Regression: a generic method on a cross-module **ambient** (`declare class`)
//! class whose type-parameter constraint references a type alias *imported into
//! the declaring module* must resolve that constraint through the declaring
//! module's own imports — not degrade it to the error type.
//!
//! Issue #15256. The cross-arena class-instance delegation skipped
//! `copy_cross_file_state_from` for ambient classes, so the delegated child
//! checker never received the all-arenas/all-binders/resolved-modules state it
//! needs to follow the declaring module's imports. The method's constraint
//! alias (`TE extends AnyTable`, `AnyTable` imported from a third module) then
//! resolved to `Error`, which downstream (a) widened the inferred literal
//! argument — a false-positive `TS2322` cascade — and (b) silently accepted
//! arguments the constraint should have rejected (a false negative). `tsc`
//! resolves the constraint and accepts.
//!
//! Requires the real project-mode driver (`-p tsconfig.json`): the bug only
//! manifests through the `ProgramContext` shared-store cross-arena delegation
//! path, so a command-line file list or the entry-only checker harness never
//! reproduces it. Binder names below are varied from the kysely repro so the
//! fix cannot be a name-scoped path.

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
    "strict": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "target": "es2022",
    "lib": ["es2022"],
    "types": [],
    "noEmit": true
  }
}
"#;

/// Run `tsz -p tsconfig.json` over an in-memory project, returning
/// `(exit_success, combined_stdout+stderr)`. Returns `None` (after printing a
/// skip notice) when the `tsz` binary is unavailable, so each test can bail with
/// a single `let else` instead of repeating the guard.
fn run_project(files: &[(&str, &str)]) -> Option<(bool, String)> {
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
    Some((output.status.success(), combined))
}

/// The #15256 gate: `declare class QC { bareC<TE extends AnyTable>(from: TE): TE }`
/// with `AnyTable` imported into the declaring module. `db.bareC('sys.tables')`
/// must infer `TE = "sys.tables"` (literal preserved), so the assignment to a
/// `"sys.tables"`-typed slot type-checks with no `TS2322`.
#[test]
fn ambient_class_method_imported_alias_constraint_preserves_literal() {
    let Some((ok, out)) = run_project(&[
        ("schema.ts", "export type AnyTable = string;\n"),
        (
            "orm.ts",
            "import type { AnyTable } from './schema';\n\
             export declare class QC {\n\
             \x20 bareC<TE extends AnyTable>(from: TE): TE;\n\
             }\n",
        ),
        (
            "app.ts",
            "import type { QC } from './orm';\n\
             declare const db: QC;\n\
             const a = db.bareC('sys.tables');\n\
             const _c: 'sys.tables' = a;\n",
        ),
    ]) else {
        return;
    };
    assert!(
        ok && !out.contains("TS2322"),
        "imported-alias constraint on an ambient class method must preserve the inferred \
         literal (no widening TS2322).\noutput:\n{out}"
    );
}

/// Renamed binders (anti-hardcoding gate): the same structural position with
/// entirely different identifiers must behave identically.
#[test]
fn ambient_class_method_imported_alias_constraint_preserves_literal_renamed() {
    let Some((ok, out)) = run_project(&[
        ("names.ts", "export type RowName = string;\n"),
        (
            "builder.ts",
            "import type { RowName } from './names';\n\
             export declare class Builder {\n\
             \x20 pick<K extends RowName>(name: K): K;\n\
             }\n",
        ),
        (
            "entry.ts",
            "import type { Builder } from './builder';\n\
             declare const b: Builder;\n\
             const picked = b.pick('users');\n\
             const _w: 'users' = picked;\n",
        ),
    ]) else {
        return;
    };
    assert!(
        ok && !out.contains("TS2322"),
        "renamed imported-alias constraint must preserve the inferred literal.\noutput:\n{out}"
    );
}

/// The constraint must be genuinely resolved and *enforced*, not merely
/// dropped: when the imported alias is `number`, a `string` argument must still
/// be rejected with `TS2345`. Before the fix, the `Error` constraint made the
/// assignability check vacuously true, so this error was silently missed.
#[test]
fn ambient_class_method_imported_alias_constraint_is_enforced() {
    let Some((_ok, out)) = run_project(&[
        ("keys.ts", "export type NumKey = number;\n"),
        (
            "svc.ts",
            "import type { NumKey } from './keys';\n\
             export declare class QC {\n\
             \x20 take<T extends NumKey>(x: T): T;\n\
             }\n",
        ),
        (
            "main.ts",
            "import type { QC } from './svc';\n\
             declare const q: QC;\n\
             const r = q.take('bad');\n",
        ),
    ]) else {
        return;
    };
    assert!(
        out.contains("TS2345"),
        "imported-alias constraint (number) must reject a string argument with TS2345.\noutput:\n{out}"
    );
}
