//! Project-mode coverage: a cross-file *user* interface must keep its own
//! declared body and must never be overridden by the name-keyed lib heritage
//! cache (`lib_type_resolution_cache`).
//!
//! Witness: `pmndrs/jotai` `src/vanilla/internals.ts` (issue #12464). A
//! cross-file generic interface (`Atom<Value>`) referenced through a local alias
//! (`type AnyAtom = Atom<unknown>`) inside a callable object type (a call
//! signature plus a method, both mentioning the alias), stored in an optional
//! property and re-derived through a homomorphic mapped type
//! (`{ -readonly [P in keyof StoreHooks]: StoreHooks[P] }`), collapsed to an
//! empty `{}` during re-instantiation. The empty body made the re-derived
//! parameter incompatible with the original, producing a spurious `TS2719`
//! ("Type 'X' is not assignable to type 'X'. Two different types with this name
//! exist, but they are unrelated.") / `TS2322`. `tsc` accepts the code.
//!
//! Root cause: `resolve_lazy` upgraded the interface body from the
//! name-keyed `lib_type_resolution_cache`, which only ever holds heritage-merged
//! *library* interface bodies. A user interface named `Atom` (whose own
//! in-progress resolution had seeded an empty/partial entry under that name) was
//! wrongly overridden with the empty cache entry. The fix restricts the override
//! to actual library definitions.
//!
//! These run the full project driver (shared `DefinitionStore`, project-mode lib
//! resolution) because the buggy state only arises under the project pipeline —
//! the single-checker test harness never seeds the name cache. The matrix varies
//! the binder names (anti-hardcoding) and includes a negative case so the fix
//! does not blanket-suppress genuine mismatches.

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// "X is not assignable to X" false-positive family.
const TS2322: u32 = 2322;
const TS2719: u32 = 2719;
/// "Property 'p' is missing in type 'S' but required in type 'T'." — the
/// drilled form tsc reports when an assignability failure reduces to one
/// missing property.
const TS2741_PROPERTY_MISSING: u32 = 2741;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
fn compile_project(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = files
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "esnext", "module": "esnext", "moduleResolution": "bundler", "noEmit": true }}, "files": [{}] }}"#,
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

fn same_name_false_positives(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == TS2322 || d.code == TS2719)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Compile `files` and assert the re-derived alias does not collapse into a
/// spurious "X is not assignable to X" (`TS2322`/`TS2719`).
fn assert_no_same_name_false_positive(files: &[(&str, &str)], context: &str) {
    let offending = same_name_false_positives(&compile_project(files));
    assert!(
        offending.is_empty(),
        "{context}: must not emit TS2322/TS2719, got {offending:#?}"
    );
}

/// The reduced jotai witness.
#[test]
fn cross_file_callable_alias_mapped_no_same_name_false_positive() {
    let files = &[
        (
            "atom.ts",
            "export interface Atom<Value> {\n  read: (get: <V>(a: Atom<V>) => V) => Value;\n}\n",
        ),
        (
            "internals.ts",
            r#"import type { Atom } from './atom.ts';
type AnyAtom = Atom<unknown>;
type StoreHookForAtoms = {
  (atom: AnyAtom): void;
  add(atom: AnyAtom, callback: () => void): () => void;
  add(atom: undefined, callback: (atom: AnyAtom) => void): () => void;
};
type StoreHooks = {
  readonly i?: StoreHookForAtoms;
  readonly r?: StoreHookForAtoms;
};
declare function createStoreHookForAtoms(): StoreHookForAtoms;
export function initializeStoreHooks(storeHooks: StoreHooks) {
  type SH = { -readonly [P in keyof StoreHooks]: StoreHooks[P] };
  (storeHooks as SH).i ||= createStoreHookForAtoms();
  (storeHooks as SH).r ||= createStoreHookForAtoms();
}
"#,
        ),
    ];
    assert_no_same_name_false_positive(files, "jotai witness");
}

/// Same shape, every user binder renamed: the fix keys on lib-ness, not on any
/// particular interface/alias/property name.
#[test]
fn cross_file_callable_alias_renamed_binders_no_same_name_false_positive() {
    let files = &[
        (
            "dep.ts",
            "export interface Widget<Payload> {\n  read: (get: <X>(a: Widget<X>) => X) => Payload;\n}\n",
        ),
        (
            "registry.ts",
            r#"import type { Widget } from './dep.ts';
type AnyWidget = Widget<unknown>;
type HookThing = {
  (w: AnyWidget): void;
  add(w: AnyWidget, cb: () => void): () => void;
};
type Registry = { readonly h?: HookThing; readonly k?: HookThing };
declare function makeHook(): HookThing;
export function setup(reg: Registry) {
  type Mut = { -readonly [K in keyof Registry]: Registry[K] };
  (reg as Mut).h ||= makeHook();
  (reg as Mut).k ||= makeHook();
}
"#,
        ),
    ];
    assert_no_same_name_false_positive(files, "renamed-binder witness");
}

/// Negative case: a genuinely incompatible value (missing the `add` method and a
/// wrong call-signature parameter) must still error. The fix must not
/// blanket-suppress real mismatches that flow through the same path. tsc
/// drills the assignment failure to the specific missing property (`TS2741`
/// "Property 'add' is missing …"); the generic `TS2322` is accepted too so the
/// guard stays shape-agnostic.
#[test]
fn cross_file_genuine_mismatch_still_reports_ts2322() {
    let diags = compile_project(&[
        (
            "dep.ts",
            "export interface Box<Value> {\n  value: Value;\n}\n",
        ),
        (
            "main.ts",
            r#"import type { Box } from './dep.ts';
type AnyBox = Box<unknown>;
type Sig = {
  (b: AnyBox): void;
  add(b: AnyBox, cb: () => void): () => void;
};
type Reg = { readonly h?: Sig };
declare function wrong(): { (b: number): void };
export function setup(reg: Reg) {
  type Mut = { -readonly [K in keyof Reg]: Reg[K] };
  (reg as Mut).h = wrong();
}
"#,
        ),
    ]);
    assert!(
        diags
            .iter()
            .any(|d| d.code == TS2322 || d.code == TS2741_PROPERTY_MISSING),
        "expected a real assignability error (TS2322 or drilled TS2741) for the genuinely incompatible assignment, got {:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
