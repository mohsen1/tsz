//! Project-mode coverage: a `unique symbol` used as a computed property *name*
//! in an interface, reached through a **type-only namespace import**
//! (`import type * as ns`), must key the member under the same binding-identity
//! atom as the value-import element access that reads it (refs #14342).
//!
//! Witness: `gvergnaud/ts-pattern` `src/internals/helpers.ts`. A cross-file
//! `export const isVariadic = Symbol.for(...)` is referenced via a type-only
//! namespace import as the declaration-side computed name and via a value
//! namespace import on the access side. The declaration-side value-position type
//! of `ns.member` is ERROR (the type-only namespace has no value meaning), so
//! the member-key resolver previously dropped the member and the value-import
//! element access produced a false `TS7053`.
//!
//! These run the full project driver (shared symbol arenas, project-mode lib
//! resolution) because the buggy state only arises under the project pipeline:
//! the cross-file binding's declaration arena is not registered on the importing
//! file's binder, so the single-checker test harness never exercises the miss.
//! The matrix varies the binder names (anti-hardcoding) and includes the value
//! import baseline so the fix does not regress the already-passing leg.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

const TS7053: u32 = 7053;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2015", "module": "esnext", "moduleResolution": "bundler", "noEmit": true }}, "files": [{}] }}"#,
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

fn assert_no_ts7053(files: &[(&str, &str)], context: &str) {
    let diags = compile_project(files);
    // Guard against a vacuous pass from collapsed/unresolved global names.
    let unresolved: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2583 | 2584))
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        unresolved.is_empty(),
        "{context}: witness has unresolved names; fix the test source: {unresolved:#?}"
    );
    let offending: Vec<_> = diags
        .iter()
        .filter(|d| d.code == TS7053)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        offending.is_empty(),
        "{context}: must not emit TS7053, got {offending:#?}"
    );
}

/// The reduced ts-pattern witness: `Symbol.for` const, type-only declaration
/// leg, value access leg.
#[test]
fn type_only_namespace_symbol_for_const_member_access_resolves() {
    let files = &[
        ("symbols.ts", "export const isVariadic = Symbol.for('v');\n"),
        (
            "main.ts",
            r#"import type * as symt from './symbols';
import * as symv from './symbols';

interface Matcher {
  [symt.isVariadic]?: boolean;
}
function f(m: Matcher) {
  return m[symv.isVariadic];
}
"#,
        ),
    ];
    assert_no_ts7053(files, "Symbol.for const, type-only decl / value access");
}

/// `unique symbol` annotation form, every user binder renamed: the fix keys on
/// the binding's symbol identity, not on any particular name.
#[test]
fn type_only_namespace_unique_symbol_annotation_renamed_binders_resolves() {
    let files = &[
        ("keys.ts", "export declare const tag: unique symbol;\n"),
        (
            "consumer.ts",
            r#"import type * as types from './keys';
import * as values from './keys';

interface Shape {
  [types.tag]?: number;
}
function read(s: Shape) {
  return s[values.tag];
}
"#,
        ),
    ];
    assert_no_ts7053(files, "unique symbol annotation, renamed binders");
}

/// Value namespace import on both legs must keep working (no regression on the
/// already-passing leg).
#[test]
fn value_namespace_symbol_member_access_still_resolves() {
    let files = &[
        ("symbols.ts", "export const isVariadic = Symbol.for('v');\n"),
        (
            "main.ts",
            r#"import * as symt from './symbols';
import * as symv from './symbols';

interface Matcher {
  [symt.isVariadic]?: boolean;
}
function f(m: Matcher) {
  return m[symv.isVariadic];
}
"#,
        ),
    ];
    assert_no_ts7053(files, "value namespace import on both legs");
}
