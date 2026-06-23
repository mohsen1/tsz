//! Cross-file symbol-identity guards: a module-local declaration must not be
//! confused with a same-raw-`SymbolId` declaration in another file.
//!
//! Per-file binders number their `SymbolId`s independently (each file starts at
//! `0`), so a local `function f` in one file and a `class C` in another can
//! share the same raw `SymbolId`. The real multi-file driver keeps them
//! distinct (per-file checker contexts plus a `(SymbolId, file_idx)`-keyed
//! cross-file cache), so resolving `f` never picks up `C`'s construct
//! signature. The in-crate `check_multi_file_with_libs` checker harness resolves
//! every file through a single context whose `symbol_types` cache is keyed by
//! the raw `SymbolId`, so it conflates the two and cannot host this guard —
//! hence the real multi-module driver test (`crate::driver::compile`).
//!
//! This is a regression floor for the cross-arena identity work (#14344): the
//! content-addressing flip changes how `DefId`/`SymbolId` resolve to types, and
//! its core risk is exactly the collision class below — a local value resolving
//! to a foreign declaration that merely shares its raw id. tsc is clean on every
//! case here; the witness fails with a spurious diagnostic only when that
//! collision leaks (e.g. a local `function` resolved to a foreign class's
//! constructor draws TS2348, "Value of type '…' is not callable. Did you mean
//! to include 'new'?").
//!
//! Binder and file names are varied across cases so the behaviour follows
//! structure, not identifier text. Each case is checked in both root-file orders
//! so the result cannot depend on which file the driver happens to check first.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) with the given root-file order.
fn compile_in_order(files: &[(&str, &str)], root_order: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--module",
        "esnext",
        "--moduleResolution",
        "bundler",
        "--lib",
        "es2022",
    ];
    argv.extend_from_slice(root_order);

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

/// Diagnostic codes that signal the identity collision leaking into a call:
/// the callee resolved to a non-callable foreign declaration (TS2348 class
/// constructor without `new`, or TS2349 not callable), or a cross-file member
/// access lost the instance's real member (TS2339).
fn identity_collision_codes(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2348 | 2349))
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Assert the repro is clean in both root-file orders (consumer-first is the
/// cross-file regression direction).
fn assert_clean_both_orders(files: &[(&str, &str)]) {
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let forward = identity_collision_codes(&compile_in_order(files, &names));
    assert!(
        forward.is_empty(),
        "expected no cross-file identity-collision diagnostics in order {names:?}, got: {forward:?}"
    );
    let reversed: Vec<&str> = names.iter().rev().copied().collect();
    let backward = identity_collision_codes(&compile_in_order(files, &reversed));
    assert!(
        backward.is_empty(),
        "expected no cross-file identity-collision diagnostics in order {reversed:?}, got: {backward:?}"
    );
}

/// A local `function` called with an argument that is a member access on a
/// cross-file class instance must stay callable: the function must not be
/// resolved to the imported class's constructor (false TS2348).
#[test]
fn local_function_not_shadowed_by_cross_file_class_constructor() {
    assert_clean_both_orders(&[
        (
            "widget.ts",
            "export class Widget { measure() { return 5; } }",
        ),
        (
            "main.ts",
            "import { Widget } from './widget';\n\
             const w = new Widget();\n\
             function handle(x: number) {}\n\
             handle(w.measure());\n\
             export {};\n",
        ),
    ]);
}

/// Same shape, different names and a property (not a method) access — proves the
/// rule follows structure, not the method-call spelling or identifier text.
#[test]
fn local_function_not_shadowed_property_access_variant() {
    assert_clean_both_orders(&[
        ("model.ts", "export class Account { balance: number = 0; }"),
        (
            "entry.ts",
            "import { Account } from './model';\n\
             const acct = new Account();\n\
             function consume(value: number) {}\n\
             consume(acct.balance);\n\
             export {};\n",
        ),
    ]);
}

/// A re-export hop between the class declaration and the consuming file must not
/// change the result: the local callee stays callable.
#[test]
fn local_function_callable_through_reexport_hop() {
    assert_clean_both_orders(&[
        ("origin.ts", "export class Service { run() { return 1; } }"),
        ("barrel.ts", "export { Service } from './origin';"),
        (
            "site.ts",
            "import { Service } from './barrel';\n\
             const svc = new Service();\n\
             function dispatch(n: number) {}\n\
             dispatch(svc.run());\n\
             export {};\n",
        ),
    ]);
}

/// A local `const` instance and an imported class must keep distinct value
/// types: the local instance must not be re-typed as one of the class's member
/// function types (which previously surfaced as a spurious TS2339 on a real
/// member).
#[test]
fn cross_file_instance_keeps_its_own_member_type() {
    assert_clean_both_orders(&[
        ("source.ts", "export class Repo { fetch() { return 7; } }"),
        (
            "use.ts",
            "import { Repo } from './source';\n\
             const repo = new Repo();\n\
             const n: number = repo.fetch();\n\
             export {};\n",
        ),
    ]);
}

/// Negative control: calling an actual imported class WITHOUT `new` must still
/// draw TS2348. The identity guard must not blanket-suppress the genuine error.
#[test]
fn imported_class_called_without_new_still_reports_ts2348() {
    let files = &[
        ("ctor.ts", "export class Builder { build() { return 1; } }"),
        (
            "caller.ts",
            "import { Builder } from './ctor';\n\
             const b = Builder();\n\
             export {};\n",
        ),
    ];
    let diagnostics = compile_in_order(files, &["ctor.ts", "caller.ts"]);
    assert!(
        diagnostics.iter().any(|d| d.code == 2348),
        "calling an imported class without `new` must still report TS2348; got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
