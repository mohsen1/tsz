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
//! Scope: this guard pins value-position aliases, generic-call constraints,
//! private/exported aliases, nested utility wrappers, and both project file
//! orders. In particular, a consumer that never names `Omit` must not replace
//! its already-materialized standard-library body with a registration-window
//! `unknown`/self-lazy placeholder (kysely `AlterColumnNode.create`, #10663).
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
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2022", "lib": ["es2022"], "module": "node16", "moduleResolution": "node16", "skipLibCheck": true, "noEmit": true }}, "files": [{}] }}"#,
        names.join(", ")
    );

    compile_project_with_tsconfig(files, &tsconfig)
}

fn compile_project_with_tsconfig(files: &[(&str, &str)], tsconfig: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    for (name, source) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, source).expect("write source");
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

/// TS2322/TS2345 (not assignable) / TS2344 (does not satisfy constraint) —
/// the family a collapsed `keyof Omit<…>` or `Extract<keyof T, string>` key set
/// produces.
fn constraint_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345 || d.code == 2344)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn diagnostic_codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|diag| diag.code).collect()
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

// A declaration-file alias `Path<T> = Extract<keyof T, string>` is the reduced
// form used by `react-hook-form` in the Next project row. It must accept concrete
// string-literal keys when consumed from a source file under `skipLibCheck`.
#[test]
fn value_position_extract_keyof_imported_declaration_alias() {
    let diags = compile_project(&[
        (
            "forms.d.ts",
            r#"
export type Path<T> = Extract<keyof T, string>;
export function useForm<T>(options: { defaultValues: T }): {
    register(field: Path<T>): Record<string, unknown>;
    watch(): T;
};
"#,
        ),
        (
            "domain.ts",
            r#"
export interface IssueInput {
    title: string;
    priority: "low" | "medium" | "high";
    area?: string;
    estimate?: number;
}
"#,
        ),
        (
            "consumer.ts",
            r#"
import type { Path } from "./forms";
import { useForm } from "./forms";
import type { IssueInput } from "./domain";

export const fields: Path<IssueInput>[] = ["title", "priority", "area", "estimate"];
const form = useForm<IssueInput>({
    defaultValues: { title: "a", priority: "low", area: "parser", estimate: 1 },
});
form.register(fields[0]);
"#,
        ),
    ]);
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "Path<IssueInput> from a declaration-file Extract<keyof T, string> alias must include the interface keys"
    );
}

#[test]
fn value_position_extract_keyof_node_package_declaration_alias() {
    let tsconfig = r#"{
        "compilerOptions": {
            "strict": true,
            "target": "es2022",
            "lib": ["es2022"],
            "module": "esnext",
            "moduleResolution": "bundler",
            "skipLibCheck": true,
            "noEmit": true,
            "jsx": "preserve",
            "types": []
        },
        "include": ["consumer.tsx", "domain.ts"],
        "exclude": ["node_modules"]
    }"#;
    let diags = compile_project_with_tsconfig(
        &[
            (
                "node_modules/react-hook-form/package.json",
                r#"{
                    "name": "react-hook-form",
                    "types": "./tsz-benchmark.d.ts",
                    "exports": {
                        ".": {
                            "types": "./tsz-benchmark.d.ts",
                            "default": "./tsz-benchmark.js"
                        }
                    }
                }"#,
            ),
            (
                "node_modules/react-hook-form/tsz-benchmark.d.ts",
                r#"
export type Path<T> = Extract<keyof T, string>;
export function useForm<T>(options: { defaultValues: T }): {
    register(field: Path<T>): Record<string, unknown>;
    watch(): T;
};
"#,
            ),
            (
                "node_modules/react-hook-form/tsz-benchmark.js",
                "export {};",
            ),
            (
                "domain.ts",
                r#"
export type IssueInput = {
    title: string;
    priority: "low" | "medium" | "high";
    area: "parser" | "binder" | "type-checker" | "emitter";
    estimate: number;
};
"#,
            ),
            (
                "consumer.tsx",
                r#"
import { useForm, type Path } from "react-hook-form";
import type { IssueInput } from "./domain";

export function IssueForm() {
    const fields: Path<IssueInput>[] = ["title", "priority", "area", "estimate"];
    const { register } = useForm<IssueInput>({
        defaultValues: { title: "a", priority: "low", area: "parser", estimate: 1 },
    });
    fields.map((field) => register(field));
    return fields;
}
"#,
            ),
        ],
        tsconfig,
    );
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "Path<IssueInput> from a package declaration alias must include the interface keys"
    );
}

#[test]
fn imported_tsx_component_does_not_leak_delegated_path_diagnostic() {
    let tsconfig = r#"{
        "compilerOptions": {
            "strict": true,
            "target": "es2022",
            "lib": ["es2022"],
            "module": "esnext",
            "moduleResolution": "bundler",
            "skipLibCheck": true,
            "noEmit": true,
            "jsx": "preserve",
            "types": []
        },
        "include": ["types/**/*.d.ts", "app/page.tsx", "components/dashboard.tsx", "lib/domain.ts"],
        "exclude": ["node_modules"]
    }"#;
    let diags = compile_project_with_tsconfig(
        &[
            (
                "node_modules/react-hook-form/package.json",
                r#"{
                    "name": "react-hook-form",
                    "types": "./tsz-benchmark.d.ts",
                    "exports": {
                        ".": {
                            "types": "./tsz-benchmark.d.ts",
                            "default": "./tsz-benchmark.js"
                        }
                    }
                }"#,
            ),
            (
                "node_modules/react-hook-form/tsz-benchmark.d.ts",
                r#"
export type Path<T> = Extract<keyof T, string>;
export function useForm<T>(options: { defaultValues: T }): {
    register(field: Path<T>): Record<string, unknown>;
    watch(): T;
};
"#,
            ),
            (
                "node_modules/react-hook-form/tsz-benchmark.js",
                "export {};",
            ),
            (
                "types/jsx.d.ts",
                r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        form: unknown;
        input: unknown;
    }
}
"#,
            ),
            (
                "lib/domain.ts",
                r#"
export type IssueInput = {
    title: string;
    priority: "low" | "medium" | "high";
    area: "parser" | "binder" | "type-checker" | "emitter";
    estimate: number;
};
export type IssueDraft = IssueInput & { id: string };
export function issueDefaults(draft: IssueDraft): IssueInput {
    return draft;
}
"#,
            ),
            (
                "components/dashboard.tsx",
                r#"
import { useForm, type Path } from "react-hook-form";
import type { IssueDraft, IssueInput } from "../lib/domain";
import { issueDefaults } from "../lib/domain";

export function IssueForm({ draft }: { draft: IssueDraft }) {
    const fields: Path<IssueInput>[] = ["title", "priority", "area", "estimate"];
    const { register } = useForm<IssueInput>({ defaultValues: issueDefaults(draft) });
    return <form>{fields.map((field) => <input {...register(field)} />)}</form>;
}
"#,
            ),
            (
                "app/page.tsx",
                r#"
import { IssueForm } from "../components/dashboard";
export const value = IssueForm;
"#,
            ),
        ],
        tsconfig,
    );
    assert_eq!(
        constraint_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-file TSX export materialization must not leak a delegated Path<IssueInput> diagnostic"
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

// ---- Cross-file utility-alias publication regression (#10663) ----

// Kysely `AlterColumnNode.create` witness: the utility alias and generic method
// live in the same module, while the caller never names the alias. Both root
// orders must retain the structural `Omit` body and accept the surviving key.
#[test]
fn constraint_keyof_omit_imported_alias_inferred_arg() {
    let provider = (
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
    );
    let consumer = (
        "builder.ts",
        r#"
import { AlterColumnNode } from "./node";
export function f() { AlterColumnNode.create('dropDefault'); }
"#,
    );

    for roots in [[provider, consumer], [consumer, provider]] {
        let diags = compile_project(&roots);
        assert_eq!(
            diagnostic_codes(&diags),
            Vec::<u32>::new(),
            "keyof Omit<AlterColumnNode, …> must include 'dropDefault' when {} is the first project root",
            roots[0].0,
        );
    }
}

// Negative companion for the exact exported-alias architecture: publication
// must preserve the reduced key set, not merely widen it enough for survivors.
#[test]
fn constraint_keyof_omit_imported_alias_rejects_omitted_key() {
    let diags = compile_project(&[
        (
            "change.ts",
            r#"
export interface ChangeNode {
    readonly kind: 'ChangeNode'
    readonly column: string
    readonly clearValue?: true
}
export type ChangeProps = Omit<ChangeNode, 'kind' | 'column'>
export declare const Changes: {
    create<Name extends keyof ChangeProps>(name: Name): void
}
"#,
        ),
        (
            "consumer.ts",
            r#"
import { Changes } from "./change";
Changes.create('column');
"#,
        ),
    ]);
    assert_eq!(
        diagnostic_codes(&diags),
        vec![2345],
        "the key removed by the imported Omit alias must still be rejected",
    );
}

// The alias need not be exported itself. A callable's public type can be the
// only path that carries `keyof` of the private mapped alias to the consumer.
#[test]
fn constraint_keyof_private_omit_alias_in_exported_callable_is_precise() {
    let diags = compile_project(&[
        (
            "mutations.ts",
            r#"
export interface MutationShape {
    readonly tag: 'MutationShape'
    readonly value: string
    readonly reset?: true
}
type MutableNames = Omit<MutationShape, 'tag'>
export declare const Mutations: {
    apply<Slot extends keyof MutableNames>(slot: Slot): void
}
"#,
        ),
        (
            "run.ts",
            r#"
import { Mutations } from "./mutations";
Mutations.apply('reset');
Mutations.apply('tag');
"#,
        ),
    ]);
    assert_eq!(
        diagnostic_codes(&diags),
        vec![2345],
        "the private alias must accept its survivor and reject its removed key",
    );
}

// Nested standard-library aliases exercise the same publication invariant for
// more than a direct `keyof Omit<…>` body.
#[test]
fn constraint_keyof_nested_utility_aliases_is_precise() {
    let diags = compile_project(&[
        (
            "preferences.ts",
            r#"
export interface Preferences {
    readonly discriminator: 'Preferences'
    readonly theme: string
    readonly retries?: number
}
type EditablePreferences = Required<Readonly<Omit<Preferences, 'discriminator'>>>
export declare function edit<Field extends keyof EditablePreferences>(field: Field): void
"#,
        ),
        (
            "screen.ts",
            r#"
import { edit } from "./preferences";
edit('theme');
edit('retries');
edit('discriminator');
"#,
        ),
    ]);
    assert_eq!(
        diagnostic_codes(&diags),
        vec![2345],
        "nested aliases must accept surviving keys and reject the removed key",
    );
}

// A renamed generic wrapper and its concrete alias must both evaluate through
// the canonical utility body; neither may depend on binder spelling.
#[test]
fn constraint_keyof_generic_and_concrete_omit_wrappers_are_precise() {
    let diags = compile_project(&[
        (
            "envelope.ts",
            r#"
interface Envelope {
    readonly kind: 'Envelope'
    readonly secret: string
    readonly payload: Uint8Array
}
type Without<Model, Hidden extends keyof Model> = Omit<Model, Hidden>
type EnvelopeFields = Without<Envelope, 'kind' | 'secret'>
export declare const Lens: {
    generic<Name extends keyof Without<Envelope, 'kind' | 'secret'>>(name: Name): void
    concrete<Name extends keyof EnvelopeFields>(name: Name): void
}
"#,
        ),
        (
            "reader.ts",
            r#"
import { Lens } from "./envelope";
Lens.generic('payload');
Lens.concrete('payload');
Lens.generic('secret');
"#,
        ),
    ]);
    assert_eq!(
        diagnostic_codes(&diags),
        vec![2345],
        "generic and concrete wrappers must retain the same precise key set",
    );
}
