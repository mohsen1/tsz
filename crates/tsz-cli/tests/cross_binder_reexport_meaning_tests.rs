//! Cross-binder re-export meaning preservation (issue #14168).
//!
//! A symbol re-exported across binder/module boundaries must preserve **all**
//! of its meanings (value, type, namespace). These CLI tests run the real
//! multi-file driver — the bug only manifests once the program skeleton hoists
//! each file's exports into the program-wide index and leaves the per-file
//! binder `module_exports` table empty, which an entry-only checker harness
//! does not reproduce.
//!
//! Two regressions are pinned:
//!
//! * A re-exported `import * as ns` namespace keeps its **type** meaning, so
//!   `ns.SomeType` in type position resolves the member instead of reporting
//!   `TS2503` ("Cannot find namespace").
//! * A re-exported `enum` referenced in a value position is typed as
//!   `typeof Enum`, so `Enum.Member` resolves instead of reporting `TS2339`,
//!   even across several re-export hops where the local symbol is an alias.
//!
//! Binder names and file names are varied across cases so the behaviour follows
//! structure, not identifier text.

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

/// Run `tsz -p tsconfig.json` over `files` (relative name, contents), returning
/// combined stdout+stderr.
///
/// The driver is invoked in **project mode** (a generated `tsconfig.json`),
/// not single-entry mode. The cross-binder namespace regression only manifests
/// once the program skeleton hoists each file's exports into the program-wide
/// index and leaves the per-file binder `module_exports` table empty — which
/// only happens on the project path. `entry` is recorded for documentation but
/// the project's `include` covers every written file.
fn run_tsz(files: &[(&str, &str)], _entry: &str) -> String {
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
        r#"{ "compilerOptions": { "noEmit": true, "strict": true, "target": "esnext", "module": "esnext", "moduleResolution": "bundler", "skipLibCheck": true } }"#,
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

/// Repro A: a namespace import re-exported by name keeps its type meaning, so a
/// `ns.Member` reference in type position resolves rather than reporting TS2503.
#[test]
fn reexported_namespace_keeps_type_meaning() {
    let out = run_tsz(
        &[
            ("sub/impl.ts", "export type Compose<X> = [X];\n"),
            (
                "sub/barrel.ts",
                "import * as F from './impl';\nexport { F };\n",
            ),
            (
                "consumer.ts",
                "import { F } from './sub/barrel';\ndeclare const x: F.Compose<'sync'>;\nexport {};\n",
            ),
        ],
        "consumer.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503"),
        "re-exported namespace must keep its type meaning (no TS2503), got:\n{out}"
    );
}

/// The re-exported namespace must keep BOTH meanings: the value form
/// (`ns.value`) and the type form (`ns.Type`) resolve from the same import.
#[test]
fn reexported_namespace_keeps_value_and_type_meaning() {
    let out = run_tsz(
        &[
            (
                "pkg/origin.ts",
                "export const tag = 1;\nexport type Wrap<X> = [X];\n",
            ),
            (
                "pkg/hub.ts",
                "import * as NS from './origin';\nexport { NS };\n",
            ),
            (
                "site.ts",
                "import { NS } from './pkg/hub';\nconst v = NS.tag;\ntype T = NS.Wrap<'x'>;\ndeclare const y: T;\nexport {};\n",
            ),
        ],
        "site.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2339"),
        "re-exported namespace must keep value and type meaning, got:\n{out}"
    );
}

/// Repro B: an enum re-exported through several hops is typed as `typeof Enum`
/// in value position, so member access resolves instead of reporting TS2339.
#[test]
fn reexported_enum_typed_as_typeof_through_hops() {
    let out = run_tsz(
        &[
            (
                "kinds.ts",
                "export enum Kind { NAME = 'Name', DIRECTIVE = 'Directive' }\n",
            ),
            ("language.ts", "export { Kind } from './kinds';\n"),
            ("index.ts", "export { Kind } from './language';\n"),
            (
                "use.ts",
                "import { Kind } from './index';\nconst a = Kind.DIRECTIVE;\nconst b = Kind.NAME;\nexport {};\n",
            ),
        ],
        "use.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2339"),
        "re-exported enum must be typed as typeof Enum (no TS2339), got:\n{out}"
    );
}

/// Same as above with different binder names and a numeric enum declared in a
/// `.d.ts`, so the fix is structural and not keyed to `Kind`/`DIRECTIVE` text.
#[test]
fn reexported_numeric_enum_typed_as_typeof_renamed_binders() {
    let out = run_tsz(
        &[
            (
                "codes.d.ts",
                "declare enum Status { Open = 1, Closed = 2 }\nexport { Status };\n",
            ),
            ("mid.d.ts", "export { Status } from './codes';\n"),
            ("top.d.ts", "export { Status } from './mid';\n"),
            (
                "app.ts",
                "import { Status } from './top';\nconst s = Status.Open;\nconst t = Status.Closed;\nexport {};\n",
            ),
        ],
        "app.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2339"),
        "re-exported numeric enum must be typed as typeof Enum (no TS2339), got:\n{out}"
    );
}

/// Repro C (#14358): a `declare class` re-exported through two or more
/// `export { C } from '...'` hops keeps its **type** meaning, so `readonly C[]`
/// in type position resolves to the class instance type (member access on an
/// element succeeds) rather than falling back to the constructor value
/// `typeof C` and reporting TS2339.
///
/// The single-step re-export chase lands on the first re-export *stub* (a symbol
/// carrying an import-module reference but no TYPE shape of its own) at 2+ hops;
/// the type reference then wrongly used the value side. tsz now re-chases the
/// stub to the declaring module so the type meaning is preserved.
#[test]
fn reexported_declare_class_keeps_type_meaning_through_two_hops() {
    let out = run_tsz(
        &[
            ("a.ts", "export declare class C { m: string; }\n"),
            ("b.ts", "export { C } from './a';\n"),
            ("c.ts", "export { C } from './b';\n"),
            (
                "use.ts",
                "import { C } from './c';\nexport function f(d: readonly C[]) { return d[0].m; }\n",
            ),
        ],
        "use.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2339"),
        "2-hop re-exported declare class must keep its type meaning (no TS2339), got:\n{out}"
    );
}

/// Three hops, with all user-chosen names varied (class, member, file stems), so
/// the re-chase follows structure rather than identifier text and is not capped
/// at a single extra hop.
#[test]
fn reexported_declare_class_keeps_type_meaning_through_three_hops_renamed() {
    let out = run_tsz(
        &[
            (
                "widget.ts",
                "export declare class Widget { label: string; }\n",
            ),
            ("mid.ts", "export { Widget } from './widget';\n"),
            ("barrel.ts", "export { Widget } from './mid';\n"),
            ("top.ts", "export { Widget } from './barrel';\n"),
            (
                "consumer.ts",
                "import { Widget } from './top';\nexport function g(items: readonly Widget[]) { return items[0].label; }\n",
            ),
        ],
        "consumer.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2339"),
        "3-hop renamed re-exported declare class must keep its type meaning (no TS2339), got:\n{out}"
    );
}

/// Parity guard: a value-only declaration (`declare const`) re-exported through
/// two hops and used in a type position must STILL report TS2749 — the re-chase
/// only preserves a TYPE-bearing target, so a value never becomes a type.
#[test]
fn reexported_value_in_type_position_still_errors_through_hops() {
    let out = run_tsz(
        &[
            ("av.ts", "export declare const V: number;\n"),
            ("bv.ts", "export { V } from './av';\n"),
            ("cv.ts", "export { V } from './bv';\n"),
            (
                "usev.ts",
                "import { V } from './cv';\nexport function f(d: readonly V[]) { return d; }\n",
            ),
        ],
        "usev.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("TS2749"),
        "a 2-hop re-exported value used as a type must still report TS2749, got:\n{out}"
    );
}

/// Parity guard: a genuinely absent member on the re-exported class instance
/// still reports TS2339 — the fix resolves the type, it does not suppress real
/// missing-property errors.
#[test]
fn reexported_declare_class_missing_member_still_errors() {
    let out = run_tsz(
        &[
            ("ac.ts", "export declare class C { m: string; }\n"),
            ("bc.ts", "export { C } from './ac';\n"),
            ("cc.ts", "export { C } from './bc';\n"),
            (
                "usec.ts",
                "import { C } from './cc';\nexport function f(d: readonly C[]) { return d[0].nope; }\n",
            ),
        ],
        "usec.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("TS2339"),
        "a genuinely absent member on the resolved class instance must still report TS2339, got:\n{out}"
    );
}

/// Parity guard: a genuinely absent enum member on a re-exported enum still
/// reports TS2339 — the fix must not blanket-suppress member errors.
#[test]
fn reexported_enum_missing_member_still_errors() {
    let out = run_tsz(
        &[
            ("e.ts", "export enum Color { Red = 'r' }\n"),
            ("re1.ts", "export { Color } from './e';\n"),
            ("re2.ts", "export { Color } from './re1';\n"),
            (
                "consume.ts",
                "import { Color } from './re2';\nconst c = Color.Blue;\nexport {};\n",
            ),
        ],
        "consume.ts",
    );
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("TS2339"),
        "a genuinely absent enum member must still report TS2339, got:\n{out}"
    );
}
