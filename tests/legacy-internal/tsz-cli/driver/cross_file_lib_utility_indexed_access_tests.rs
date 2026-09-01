//! Project-mode parity guard for an indexed access into a standard-library
//! conditional utility whose result crosses a module / declaration-file
//! boundary (`Parameters<F>[0]`, `ConstructorParameters<C>[0]`,
//! `ReturnType<G>['fn']`).
//!
//! Structural rule: when an exported type alias body is an indexed access into a
//! lib conditional-utility application, the alias body is lowered through the
//! eager declaration-file / cross-arena resolver. That resolver records only the
//! lib utility's `DefId`; unlike the in-file (`.ts`) reference path
//! (`ensure_def_ready_for_lowering` -> `resolve_lib_type_by_name`), it never
//! primed the utility's body. The utility (`Parameters`, …) then resolved to the
//! `unknown` placeholder, so the indexed access stayed deferred and a value of
//! that type was treated as non-callable — a false `TS2349` "has no call
//! signatures". `tsc` is clean. The fix primes the lib utility body while
//! lowering the *referencing* alias, so the cross-module route registers the
//! same conditional body the in-file route does.
//!
//! These cases run the real multi-file driver (shared `DefinitionStore`, every
//! file checked, real module resolution) — the faithful path for cross-module
//! resolution; the in-crate single-context checker harness cannot host them.
//!
//! Binder names vary across cases so the guard follows the structural shape
//! rather than any identifier (anti-hardcoding).
//!
//! Mined from **msw** (`@open-draft/deferred-promise`'s `ResolveFunction =
//! Parameters<…>[0]`). Issue: <https://github.com/tsz-org/tsz/issues/14729>

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

/// TS2349 ("This expression is not callable. Type '…' has no call signatures")
/// — what a deferred/opaque indexed-access utility result produces at a call.
fn not_callable_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2349)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn assignability_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// `export type R = Parameters<F>[0]` in a **declaration file**, called from
// another module — the dominant msw FP cluster.
#[test]
fn parameters_indexed_alias_from_declaration_file_is_callable() {
    let diags = compile_project(&[
        (
            "deferred.d.ts",
            r#"
type Executor = (resolve: (value: number) => void) => void;
export type ResolveFn = Parameters<Executor>[0];
"#,
        ),
        (
            "main.ts",
            r#"
import type { ResolveFn } from "./deferred";
declare const resolve: ResolveFn;
resolve(5);
"#,
        ),
    ]);
    assert_eq!(
        not_callable_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-module Parameters<Executor>[0] must reduce to a callable"
    );
}

// Same shape exported from a plain `.ts` module (already worked; pins parity so
// the two lowering routes stay consistent).
#[test]
fn parameters_indexed_alias_from_ts_module_is_callable() {
    let diags = compile_project(&[
        (
            "source.ts",
            r#"
type Handler = (cb: (value: string) => void) => void;
export type CbOf = Parameters<Handler>[0];
"#,
        ),
        (
            "consumer.ts",
            r#"
import type { CbOf } from "./source";
declare const cb: CbOf;
cb("ok");
"#,
        ),
    ]);
    assert_eq!(
        not_callable_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-module Parameters<Handler>[0] (.ts) must reduce to a callable"
    );
}

// `ConstructorParameters<typeof C>[0]` cross-module — confirmed adjacent FP.
#[test]
fn constructor_parameters_indexed_alias_is_callable() {
    let diags = compile_project(&[
        (
            "ctor.d.ts",
            r#"
declare class Widget { constructor(onReady: (value: number) => void); }
export type OnReady = ConstructorParameters<typeof Widget>[0];
"#,
        ),
        (
            "app.ts",
            r#"
import type { OnReady } from "./ctor";
declare const onReady: OnReady;
onReady(7);
"#,
        ),
    ]);
    assert_eq!(
        not_callable_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-module ConstructorParameters<typeof Widget>[0] must reduce to a callable"
    );
}

// `ReturnType<G>['fn']` cross-module — confirmed adjacent FP. The index is a
// named property, not a tuple position.
#[test]
fn return_type_named_index_alias_is_callable() {
    let diags = compile_project(&[
        (
            "factory.d.ts",
            r#"
type Make = () => { invoke: (value: number) => void };
export type Invoke = ReturnType<Make>["invoke"];
"#,
        ),
        (
            "runner.ts",
            r#"
import type { Invoke } from "./factory";
declare const invoke: Invoke;
invoke(9);
"#,
        ),
    ]);
    assert_eq!(
        not_callable_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-module ReturnType<Make>['invoke'] must reduce to a callable"
    );
}

// Concrete (non-callable) index: `Parameters<F2>[1]` must reduce to the second
// parameter's type so the assignment type-checks — proves the reduction itself,
// not just that the result happens to be callable.
#[test]
fn parameters_concrete_index_reduces_to_member_type() {
    let diags = compile_project(&[
        (
            "sig.d.ts",
            r#"
type Two = (a: string, b: number) => void;
export type Second = Parameters<Two>[1];
"#,
        ),
        (
            "use.ts",
            r#"
import type { Second } from "./sig";
declare const second: Second;
export const n: number = second;
"#,
        ),
    ]);
    assert_eq!(
        assignability_errors(&diags),
        Vec::<(u32, String)>::new(),
        "cross-module Parameters<Two>[1] must reduce to `number`"
    );
}

// Negative control: a user-defined alias named `Parameters` that shadows the lib
// utility must keep its own meaning — the fix must not substitute the lib body.
#[test]
fn user_shadowing_parameters_alias_is_not_overridden_by_lib() {
    let diags = compile_project(&[
        (
            "shadow.ts",
            r#"
type Parameters<T> = T extends (a: infer A) => unknown ? A : never;
type Fn = (s: string) => void;
export type ParamOf = Parameters<Fn>;
"#,
        ),
        (
            "client.ts",
            r#"
import type { ParamOf } from "./shadow";
declare const p: ParamOf;
export const s: string = p;
"#,
        ),
    ]);
    // User `Parameters<Fn>` is `string` (the inferred `A`), so the assignment
    // holds; if the lib tuple body were wrongly substituted it would be
    // `[s: string]` and TS2322 would fire.
    assert_eq!(
        assignability_errors(&diags),
        Vec::<(u32, String)>::new(),
        "user `Parameters<T>` must keep its own meaning across modules"
    );
}

// Negative control: a plain cross-module indexed access (no lib conditional
// utility) must stay callable and unaffected by the priming.
#[test]
fn plain_cross_module_indexed_access_still_callable() {
    let diags = compile_project(&[
        (
            "box.d.ts",
            r#"
interface Box { run: (value: number) => void }
export type RunOf = Box["run"];
"#,
        ),
        (
            "callsite.ts",
            r#"
import type { RunOf } from "./box";
declare const run: RunOf;
run(3);
"#,
        ),
    ]);
    assert_eq!(
        not_callable_errors(&diags),
        Vec::<(u32, String)>::new(),
        "plain cross-module Box['run'] must stay callable"
    );
}
