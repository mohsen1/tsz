//! Project-mode coverage for a value merged with a self-referential type alias
//! (`const X = <literal>; type X = typeof X`) used in a *consuming* file.
//!
//! The merged symbol stores the self-referential `TypeQuery(X)` as its
//! type-space body. The declaring file registers `X`'s value-space type while
//! checking the alias, but a consuming file's per-file `type_env` is reset, so
//! the deferred `TypeQuery(X)` self-loops in `resolve_type_query` and every
//! relation against the reference fails: a false `TS2344` when the reference is
//! a generic argument (the original ts-pattern `anonymousSelectKey` row, arch
//! #8225) and a false `TS2322` when it is the source of an assignment. These run
//! the full project driver (shared `DefinitionStore`, every file checked) so the
//! per-file reset that triggers the self-loop is exercised — the simplified
//! single-context checker harness does not reproduce it. See #15078.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS2344: u32 = 2344;
const TS2322: u32 = 2322;

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

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

const SYMBOLS: (&str, &str) = (
    "symbols.ts",
    "export const anonymousSelectKey = '@ts-pattern/anonymous-select-key';\n\
     export type anonymousSelectKey = typeof anonymousSelectKey;\n",
);

/// The ts-pattern repro: a string-literal marker merged with its self-`typeof`
/// alias satisfies a `string` constraint across files (no false TS2344).
#[test]
fn string_marker_satisfies_string_constraint_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "patterns.ts",
            "import { anonymousSelectKey } from './symbols';\n\
             type SelectP<key extends string> = key;\n\
             export type Bad = SelectP<anonymousSelectKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "string marker should satisfy the `string` constraint cross-file, got: {diags:?}"
    );
}

/// Renamed import (`as`) keeps the value-space resolution working.
#[test]
fn renamed_marker_satisfies_constraint_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "patterns.ts",
            "import { anonymousSelectKey as Key } from './symbols';\n\
             type SelectP<key extends string> = key;\n\
             export type Bad = SelectP<Key>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "renamed marker should satisfy the constraint, got: {diags:?}"
    );
}

/// Negative control: a numeric marker must NOT satisfy a `string` constraint —
/// the value side is genuinely checked, not blindly accepted.
#[test]
fn number_marker_violates_string_constraint_cross_file() {
    let diags = compile_project(&[
        (
            "nsym.ts",
            "export const numKey = 42;\nexport type numKey = typeof numKey;\n",
        ),
        (
            "nmain.ts",
            "import { numKey } from './nsym';\n\
             type NeedsString<k extends string> = k;\n\
             export type Bad = NeedsString<numKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        1,
        "numeric marker must violate the `string` constraint, got: {diags:?}"
    );
}

/// The same numeric marker satisfies a `number` constraint (value side accepted).
#[test]
fn number_marker_satisfies_number_constraint_cross_file() {
    let diags = compile_project(&[
        (
            "nsym.ts",
            "export const numKey = 42;\nexport type numKey = typeof numKey;\n",
        ),
        (
            "nmain.ts",
            "import { numKey } from './nsym';\n\
             type NeedsNumber<k extends number> = k;\n\
             export type Ok = NeedsNumber<numKey>;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2344),
        0,
        "numeric marker should satisfy the `number` constraint, got: {diags:?}"
    );
}

/// Assignment position: the marker's literal is assignable to `string`, so the
/// previously-deferred self-loop no longer produces a false assignment error.
#[test]
fn marker_assignable_to_string_cross_file() {
    let diags = compile_project(&[
        SYMBOLS,
        (
            "use.ts",
            "import { anonymousSelectKey } from './symbols';\n\
             export const ok: string = null as unknown as anonymousSelectKey;\n",
        ),
    ]);
    assert_eq!(
        count_code(&diags, TS2322),
        0,
        "string marker should be assignable to `string`, got: {diags:?}"
    );
}
