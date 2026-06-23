//! Project-mode parity guard for cross-file `keyof` over imported interfaces
//! and utility-type aliases (`Omit`/`Pick`/`Exclude`).
//!
//! `tsc` resolves `keyof Omit<I, K>` (and friends) to the same concrete key set
//! regardless of which module declares `I`, the alias, or the consumer. tsz
//! must match. These cases run the real multi-file driver (shared
//! `DefinitionStore`, every file checked), the faithful path for cross-module
//! resolution — the in-crate single-context checker harness conflates per-file
//! `SymbolId`/`DefId` namespaces and cannot host them.
//!
//! Scope: this guard pins the cross-file `keyof`/utility-alias behavior tsz
//! already gets right (value-position `keyof Omit<…>`, bare `keyof I`, identity
//! and `keyof` aliases) so future cross-arena identity work (#14344) cannot
//! silently regress them. The one shape that does NOT yet match `tsc` — a
//! generic-call constraint `T extends keyof Omit<ImportedIface, K>`, where the
//! consumer never names the utility alias so its structural body is never
//! lowered locally and collapses to `never` — is captured as an `#[ignore]`d
//! witness (kysely `AlterColumnNode.create`, #10663) pending that work.
//!
//! Binder names vary across cases so the guard follows the structural shape
//! rather than any identifier (anti-hardcoding).

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2022", "lib": ["es2022"], "module": "node16", "moduleResolution": "node16", "skipLibCheck": true, "noEmit": true }}, "files": [{}] }}"#,
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

/// TS2345 (argument not assignable) / TS2344 (does not satisfy constraint) —
/// the family a collapsed `keyof Omit<…>` key set produces.
fn constraint_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2344)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// ---- Cases tsz already matches `tsc` on (parity floor) ----

// `keyof Omit<ImportedIface, K>` in **value position**, cross-file: the omitted
// keys are dropped and the surviving key is assignable.
#[test]
fn value_position_keyof_omit_imported_interface() {
    let diags = compile_project(&[
        (
            "shapes.ts",
            r#"
export interface ColumnNode {
    readonly kind: 'ColumnNode'
    readonly column: string
    readonly dropDefault?: true
}
export type ColumnProps = keyof Omit<ColumnNode, 'kind' | 'column'>
"#,
        ),
        (
            "main.ts",
            r#"
import { ColumnProps } from "./shapes";
export const k: ColumnProps = 'dropDefault';
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "value-position keyof Omit<ColumnNode, …> must include 'dropDefault'"
    );
}

// `keyof Pick<ImportedIface, K>` in value position keeps the picked key.
#[test]
fn value_position_keyof_pick_imported_interface() {
    let diags = compile_project(&[
        (
            "decls.ts",
            r#"
export interface RowSpec {
    readonly id: number
    readonly label: string
}
export type Picked = keyof Pick<RowSpec, 'label'>
"#,
        ),
        (
            "consumer.ts",
            r#"
import { Picked } from "./decls";
export const p: Picked = 'label';
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "value-position keyof Pick<RowSpec, 'label'> must include 'label'"
    );
}

// `Exclude<keyof ImportedIface, K>` resolves cross-file.
#[test]
fn value_position_exclude_keyof_imported_interface() {
    let diags = compile_project(&[
        (
            "model.ts",
            r#"
export interface Entity {
    readonly tag: 'Entity'
    readonly name: string
}
export type Fields = Exclude<keyof Entity, 'tag'>
"#,
        ),
        (
            "use.ts",
            r#"
import { Fields } from "./model";
export const f: Fields = 'name';
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "Exclude<keyof Entity, 'tag'> must include 'name'"
    );
}

// A generic-call constraint `T extends keyof ImportedIface` (bare `keyof`, no
// utility alias) resolves across modules and accepts a valid key.
#[test]
fn constraint_bare_keyof_imported_interface() {
    let diags = compile_project(&[
        (
            "api.ts",
            r#"
export interface Record2 {
    readonly id: number
    readonly title: string
}
export declare const Api: { pick<P extends keyof Record2>(prop: P): void }
"#,
        ),
        (
            "caller.ts",
            r#"
import { Api } from "./api";
export function go() { Api.pick('title'); }
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "T extends keyof Record2 must accept 'title' across modules"
    );
}

// A generic-call constraint `T extends keyof Omit<…>` where the utility alias is
// declared in the **caller** (so its body is lowered locally) resolves and
// rejects an omitted key — the negative control showing the key set is precise
// once the alias is reachable.
#[test]
fn constraint_keyof_omit_local_alias_rejects_excluded_key() {
    let diags = compile_project(&[
        (
            "iface.ts",
            r#"
export interface FieldNode {
    readonly kind: 'FieldNode'
    readonly column: string
    readonly extra?: true
}
"#,
        ),
        (
            "local.ts",
            r#"
import { FieldNode } from "./iface";
type Props = Omit<FieldNode, 'kind' | 'column'>
declare function make<T extends keyof Props>(prop: T): void;
export function run() { make('column'); }
"#,
        ),
    ]);
    // 'column' is removed by Omit, so it must NOT satisfy keyof Props.
    assert!(
        !constraint_errors(&diags).is_empty(),
        "'column' is omitted and must be rejected by keyof Omit<FieldNode, …>"
    );
}

// And the positive side of the same local-alias shape: a surviving key is
// accepted (so the rejection above is not a blanket failure).
#[test]
fn constraint_keyof_omit_local_alias_accepts_surviving_key() {
    let diags = compile_project(&[
        (
            "iface.ts",
            r#"
export interface FieldNode {
    readonly kind: 'FieldNode'
    readonly column: string
    readonly extra?: true
}
"#,
        ),
        (
            "local.ts",
            r#"
import { FieldNode } from "./iface";
type Props = Omit<FieldNode, 'kind' | 'column'>
declare function make<T extends keyof Props>(prop: T): void;
export function run() { make('extra'); }
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "'extra' survives Omit and must satisfy keyof Props"
    );
}

// ---- Known gap: cross-arena utility-alias identity (#10663 / #14344) ----

// kysely `AlterColumnNode.create` witness: the utility alias and the generic
// method live in the SAME module, the interface backs the alias, and the call
// is in a DIFFERENT module that never names the alias. The consumer resolves
// the lib `Omit` def through the shared store's `unknown` placeholder (the
// structural body is keyed to the declaring arena, #14344), so `keyof Omit<…>`
// collapses to `never` and a valid key is rejected. tsc is clean.
#[test]
#[ignore = "cross-arena utility-alias identity (#10663/#14344): consumer resolves the lib Omit def to the unknown placeholder, collapsing keyof Omit<…> to never"]
fn constraint_keyof_omit_imported_alias_inferred_arg() {
    let diags = compile_project(&[
        (
            "node.ts",
            r#"
export interface AlterColumnNode {
    readonly kind: 'AlterColumnNode'
    readonly column: string
    readonly dropDefault?: true
}
export type AlterColumnNodeProps = Omit<AlterColumnNode, 'kind' | 'column'>
export declare const AlterColumnNode: {
    create<T extends keyof AlterColumnNodeProps>(prop: T): void
}
"#,
        ),
        (
            "builder.ts",
            r#"
import { AlterColumnNode } from "./node";
export function f() { AlterColumnNode.create('dropDefault'); }
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "keyof Omit<AlterColumnNode, …> over a cross-module alias must include 'dropDefault'"
    );
}
