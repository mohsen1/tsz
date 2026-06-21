//! Qualified-type-name LHS resolution under a local-declaration shadow
//! (issue #14225, mined from `ts-toolbelt`).
//!
//! tsc resolves the left side of a qualified type name `X.Member` with
//! *namespace* meaning. When a module re-imports a namespace by name
//! (`import { X }`, where the source re-exports an `import * as X`) **and**
//! declares a same-named local `type X` / `interface X`, tsc keeps the local
//! declaration for a bare `: X` reference but still resolves `X.Member` through
//! the imported namespace. tsz previously gated namespace-anchor recognition on
//! the *syntactic* `import * as` / `import =` form, so the shadowed re-import
//! lost its namespace meaning and reported a spurious `TS2503`.
//!
//! These run the **real multi-file driver** in project mode. The bug only
//! manifests once the program skeleton hoists each file's exports into the
//! program-wide index and links the same-named local declaration and import
//! alias via `program_alias_partners`; an entry-only checker harness resolves
//! the re-export differently and does not reproduce it.
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
/// combined stdout+stderr. The driver is invoked in project mode so the
/// program-wide skeleton (and `program_alias_partners`) is built.
fn run_tsz(files: &[(&str, &str)]) -> String {
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

/// The mined repro: `import * as T` re-exported under the same name, consumed by
/// `import { T }` alongside a local `type T`. `T.Intersect<...>` must resolve
/// through the imported namespace (tsc clean), not collapse onto the shadowing
/// tuple alias and report `TS2503`/`TS2694`.
#[test]
fn type_alias_shadow_resolves_qualified_member_through_imported_namespace() {
    let out = run_tsz(&[
        ("lib.ts", "export type Intersect<A, B> = A & B;\n"),
        ("index.ts", "import * as T from './lib';\nexport { T };\n"),
        (
            "use.ts",
            "import { T } from './index';\ntype T = [1, 2, 3];\ntype R = T.Intersect<{ a: 1 }, { b: 2 }>;\nconst r: R = { a: 1, b: 2 };\nexport { r };\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2694"),
        "shadowed re-imported namespace must anchor T.Member, got:\n{out}"
    );
}

/// A bare `: T` reference keeps the local `type T` (a tuple) — the qualified
/// anchor redirect must not change the meaning of a non-qualified type
/// reference. The tuple is assignable from the literal, so the whole file stays
/// clean.
#[test]
fn bare_type_reference_keeps_local_alias_while_qualified_anchor_uses_namespace() {
    let out = run_tsz(&[
        ("lib.ts", "export type Intersect<A, B> = A & B;\n"),
        ("index.ts", "import * as T from './lib';\nexport { T };\n"),
        (
            "use.ts",
            "import { T } from './index';\ntype T = [1, 2, 3];\nconst local: T = [1, 2, 3];\ntype R = T.Intersect<{ a: 1 }, { b: 2 }>;\nconst r: R = { a: 1, b: 2 };\nexport { local, r };\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2694") && !out.contains("TS2322"),
        "bare `: T` keeps the local alias while `T.Member` uses the namespace, got:\n{out}"
    );
}

/// An `interface` shadow (rather than a `type` alias) under the same name must
/// also resolve the qualified member through the imported namespace. Binder and
/// file names differ from the first case so the behaviour is structural.
#[test]
fn interface_shadow_resolves_qualified_member_through_imported_namespace() {
    let out = run_tsz(&[
        ("shapes.ts", "export type Pick2<A, B> = { a: A; b: B };\n"),
        (
            "barrel.ts",
            "import * as Box from './shapes';\nexport { Box };\n",
        ),
        (
            "consumer.ts",
            "import { Box } from './barrel';\ninterface Box { z: number }\ntype R = Box.Pick2<1, 2>;\nconst r: R = { a: 1, b: 2 };\nexport { r };\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2694"),
        "interface shadow must anchor Box.Member on the namespace, got:\n{out}"
    );
}

/// Renaming the namespace binder (`import * as Widget` re-exported, local
/// `type Widget`) proves the fix is not keyed on any particular identifier.
#[test]
fn renamed_binder_shadow_resolves_qualified_member() {
    let out = run_tsz(&[
        (
            "palette.ts",
            "export type Pair<A, B> = { first: A; second: B };\n",
        ),
        (
            "gateway.ts",
            "import * as Widget from './palette';\nexport { Widget };\n",
        ),
        (
            "screen.ts",
            "import { Widget } from './gateway';\ntype Widget = [1];\ntype R = Widget.Pair<1, 2>;\nconst r: R = { first: 1, second: 2 };\nexport { r };\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2694"),
        "renamed binder shadow must anchor Widget.Member, got:\n{out}"
    );
}

/// A deeper re-export chain (`leaf` -> `mid` -> `hub`) under a local shadow must
/// still walk through to the terminal namespace.
#[test]
fn deep_reexport_chain_shadow_resolves_qualified_member() {
    let out = run_tsz(&[
        ("leaf.ts", "export type Pick2<A, B> = { a: A; b: B };\n"),
        ("mid.ts", "import * as Ns from './leaf';\nexport { Ns };\n"),
        ("hub.ts", "export { Ns } from './mid';\n"),
        (
            "entry.ts",
            "import { Ns } from './hub';\ntype Ns = [1];\ntype R = Ns.Pick2<1, 2>;\nconst r: R = { a: 1, b: 2 };\nexport { r };\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS2694"),
        "deep re-export chain shadow must anchor Ns.Member, got:\n{out}"
    );
}

/// Negative control: when the named import is genuinely a *value* (not a
/// namespace), a same-named local type does not invent a namespace anchor —
/// `Token.Member` still reports `TS2503`, matching tsc.
#[test]
fn value_import_shadow_still_reports_cannot_find_namespace() {
    let out = run_tsz(&[
        ("runtime.ts", "export const Token = 1;\n"),
        (
            "client.ts",
            "import { Token } from './runtime';\ntype Token = [1];\ntype R = Token.Member;\nexport type Exported = R;\n",
        ),
    ]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("TS2503"),
        "a value import shadowed by a local type must not become a namespace anchor, got:\n{out}"
    );
}
