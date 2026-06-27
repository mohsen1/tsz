//! `globalThis.X` in **type** position must resolve `X` to the global type of
//! that name through the full CLI driver pipeline (reconstructed program
//! binders with lib symbols remapped into the unified arena).
//!
//! Regression for #14921: the synthetic-namespace member resolver returned the
//! *lib-arena-local* `SymbolId` of the global type, which the file binder then
//! re-read as an unrelated symbol of the same numeric id — so `globalThis.Record`
//! resolved to `CSSNestedDeclarations`, `globalThis.Array` to `btoa`, etc. (a
//! 258-diagnostic false-positive family on the runtypes row). Only the full
//! program-reconstruction path exhibits the cross-binder id divergence, so this
//! coverage lives at the CLI driver level rather than in the single-file checker
//! harness (where lib ids stay coincident with the file binder).

use crate::args::CliArgs;
use clap::Parser;

fn cli_diag_codes(source: &str, extra_args: &[&str]) -> Vec<u32> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("repro.ts"), source).expect("write repro");

    let mut argv = vec!["tsz", "--ignoreConfig", "--noEmit", "--strict"];
    argv.extend_from_slice(extra_args);
    argv.push("repro.ts");

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    let result = crate::driver::compile(&args, dir.path()).expect("compile should succeed");
    result.diagnostics.iter().map(|diag| diag.code).collect()
}

#[test]
fn cli_global_this_record_resolves_to_global_record() {
    // The reported witness: `globalThis.Record<string, number>` is the global
    // `Record` alias, so a matching object literal assigns with no TS2315
    // (`not generic`) / TS2740 (`missing properties`) / TS2304 noise.
    let codes = cli_diag_codes(
        r#"
type R = globalThis.Record<string, number>;
const x: R = { a: 1 };
x;
"#,
        &["--target", "es2022"],
    );
    assert!(
        !codes.iter().any(|c| matches!(c, 2304 | 2315 | 2740)),
        "expected `globalThis.Record<string, number>` to resolve to the global Record, got: {codes:?}"
    );
}

#[test]
fn cli_global_this_generic_members_resolve_with_dom_lib() {
    // The DOM lib is where the wrong-symbol family surfaced (`CSSNestedDeclarations`,
    // `btoa`, `CSSLayerBlockRule`). Exercise the same lib set and assert every
    // generic global referenced through `globalThis.` resolves cleanly.
    let codes = cli_diag_codes(
        r#"
type R = globalThis.Record<string, number>;
type A = globalThis.Array<number>;
type P = globalThis.Promise<number>;
type M = globalThis.Map<string, number>;
type S = globalThis.Set<number>;
declare const r: R;
declare const a: A;
declare const p: P;
declare const m: M;
declare const s: S;
r; a; p; m; s;
"#,
        &["--target", "es2022", "--lib", "es2022,dom"],
    );
    assert!(
        !codes.iter().any(|c| matches!(c, 2304 | 2315 | 2740)),
        "expected all generic `globalThis.X` members to resolve under es2022,dom, got: {codes:?}"
    );
}

#[test]
fn cli_global_this_missing_member_reports_ts2694() {
    // Negative control: a non-global name still routes through the "no exported
    // member" diagnostic — the fix must not mask it.
    let codes = cli_diag_codes(
        r#"
type X = globalThis.ZzzDefinitelyNotARealGlobalType;
declare const x: X;
x;
"#,
        &["--target", "es2022"],
    );
    assert!(
        codes.contains(&2694),
        "expected TS2694 for a missing `globalThis` member, got: {codes:?}"
    );
}

#[test]
fn cli_global_this_member_validates_type_argument_arity() {
    // Resolving the correct symbol must preserve type-argument arity validation:
    // `Record` takes exactly two type arguments.
    let codes = cli_diag_codes(
        r#"
type R = globalThis.Record<string>;
declare const r: R;
r;
"#,
        &["--target", "es2022"],
    );
    assert!(
        codes.contains(&2314),
        "expected TS2314 for wrong type-argument count on `globalThis.Record`, got: {codes:?}"
    );
}

#[test]
fn cli_global_this_in_module_resolves_lib_member() {
    // An external module (top-level `export`) must still resolve lib members
    // through `globalThis.` — and must not pick up an unrelated symbol.
    let codes = cli_diag_codes(
        r#"
export {};
type R = globalThis.Record<string, number>;
const x: R = { a: 1 };
x;
"#,
        &["--target", "es2022"],
    );
    assert!(
        !codes.iter().any(|c| matches!(c, 2304 | 2315 | 2740)),
        "expected `globalThis.Record` to resolve in a module, got: {codes:?}"
    );
}
