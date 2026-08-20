//! Project-mode coverage for cross-file circular type aliases (TS2456).
//!
//! A type alias whose body refers back to itself through one or more aliases in
//! *other* modules is circular, and tsc reports one `TS2456` per participating
//! alias at its declaration name. tsz resolves cross-file alias bodies by
//! delegating to child checkers per file; without cross-arena cycle detection
//! the delegation ping-pongs between arenas until the depth guard collapses the
//! type to `error`, so no participant is flagged. These tests run the full
//! project driver (shared `DefinitionStore`, every file checked) so both sides
//! of a cycle are observed, mirroring the conformance fixtures
//! `externalModules/typeOnly/circular2.ts` and `circular4.ts`.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS2456: u32 = 2456;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2015", "noEmit": true }}, "files": [{}] }}"#,
        names.join(", ")
    );
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    for (name, source) in files {
        fs::write(dir.path().join(name), source).expect("write source");
    }

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    compile(&args, dir.path())
        .expect("compile succeeds")
        .diagnostics
}

/// Names of aliases flagged TS2456 in the file whose path ends with `suffix`.
fn ts2456_alias_names(diags: &[Diagnostic], suffix: &str) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == TS2456 && d.file.ends_with(suffix))
        .map(|d| d.message_text.clone())
        .collect()
}

fn count_ts2456(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == TS2456).count()
}

#[test]
fn two_file_alias_cycle_reports_ts2456_in_each_file() {
    // Mirrors conformance circular2.ts.
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { B } from './b';\nexport type A = B;\n",
        ),
        (
            "b.ts",
            "import type { A } from './a';\nexport type B = A;\n",
        ),
    ]);
    assert_eq!(
        count_ts2456(&diags),
        2,
        "expected exactly two TS2456 (one per file), got: {diags:?}"
    );
    let a = ts2456_alias_names(&diags, "a.ts");
    let b = ts2456_alias_names(&diags, "b.ts");
    assert!(
        a.iter().any(|m| m.contains("'A'")),
        "expected TS2456 for alias 'A' in a.ts, got: {a:?}"
    );
    assert!(
        b.iter().any(|m| m.contains("'B'")),
        "expected TS2456 for alias 'B' in b.ts, got: {b:?}"
    );
}

#[test]
fn two_file_alias_cycle_is_not_name_specific() {
    // Same shape as above with different identifiers — proves the fix keys on
    // the alias cycle structure, not on the spelling `A`/`B`.
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { Second } from './b';\nexport type First = Second;\n",
        ),
        (
            "b.ts",
            "import type { First } from './a';\nexport type Second = First;\n",
        ),
    ]);
    assert_eq!(count_ts2456(&diags), 2, "got: {diags:?}");
    assert!(
        ts2456_alias_names(&diags, "a.ts")
            .iter()
            .any(|m| m.contains("'First'"))
    );
    assert!(
        ts2456_alias_names(&diags, "b.ts")
            .iter()
            .any(|m| m.contains("'Second'"))
    );
}

#[test]
fn three_file_alias_cycle_reports_ts2456_in_each_file() {
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { Y } from './b';\nexport type X = Y;\n",
        ),
        (
            "b.ts",
            "import type { Z } from './c';\nexport type Y = Z;\n",
        ),
        (
            "c.ts",
            "import type { X } from './a';\nexport type Z = X;\n",
        ),
    ]);
    assert_eq!(
        count_ts2456(&diags),
        3,
        "expected one TS2456 per file in a 3-file cycle, got: {diags:?}"
    );
}

#[test]
fn namespace_nested_qualified_alias_cycle_reports_ts2456() {
    // Mirrors conformance circular4.ts: the cycle runs through qualified names
    // (`ns2.nested.T`) across modules, and the colliding NodeIndex/name shape of
    // the two files must not be mistaken for a local declaration.
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { ns2 } from './b';\nexport namespace ns1 {\n  export namespace nested {\n    export type T = ns2.nested.T;\n  }\n}\n",
        ),
        (
            "b.ts",
            "import type { ns1 } from './a';\nexport namespace ns2 {\n  export namespace nested {\n    export type T = ns1.nested.T;\n  }\n}\n",
        ),
    ]);
    assert_eq!(
        count_ts2456(&diags),
        2,
        "expected one TS2456 per file for the namespace-nested cycle, got: {diags:?}"
    );
    assert!(
        ts2456_alias_names(&diags, "a.ts")
            .iter()
            .any(|m| m.contains("'T'"))
    );
    assert!(
        ts2456_alias_names(&diags, "b.ts")
            .iter()
            .any(|m| m.contains("'T'"))
    );
}

#[test]
fn deferred_cross_file_alias_cycle_is_not_circular() {
    // `A = B[]` / `B = A` is `B = B[]` — a legal recursive alias deferred behind
    // an array, exactly as tsc treats it. No TS2456.
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { B } from './b';\nexport type A = B[];\n",
        ),
        (
            "b.ts",
            "import type { A } from './a';\nexport type B = A;\n",
        ),
    ]);
    assert_eq!(
        count_ts2456(&diags),
        0,
        "deferred (array-wrapped) cross-file cycle must not be TS2456, got: {diags:?}"
    );
}

#[test]
fn non_cyclic_cross_file_alias_has_no_ts2456() {
    let diags = compile_project(&[
        (
            "a.ts",
            "import type { B } from './b';\nexport type A = B;\nexport const v: A = 1;\n",
        ),
        ("b.ts", "export type B = number;\n"),
    ]);
    assert_eq!(
        count_ts2456(&diags),
        0,
        "non-cyclic cross-file alias must not report TS2456, got: {diags:?}"
    );
}
