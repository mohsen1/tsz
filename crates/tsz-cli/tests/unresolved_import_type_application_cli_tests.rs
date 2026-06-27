//! Unresolved-import type-argument application contagion (#14747).
//!
//! Structural rule: when a type alias/interface imported from a module that
//! failed to resolve (`TS2307`) is applied with type arguments (`Gen<T>`) —
//! typically inside a generic type-parameter constraint validated against an
//! inferred array/tuple/object argument — the application must collapse to the
//! permissive `error`/`any` type. `tsc` substitutes the error type for a
//! reference whose target symbol failed to resolve, so `Gen<{...}>` behaves as
//! `any` for assignability. tsz previously kept it as a live structural
//! `Application(Lazy(unresolved-def), args)` the relation layer rejected,
//! producing a false `TS2322`/`TS2345`/`TS2353` cascade (the remeda
//! `type-fest`-consumer cluster: `pick`/`hasSubObject`/`omit`/…).
//!
//! Owner: the solver type evaluator collapses the application once the
//! resolver (`TypeResolver::is_unresolved_import_def`) reports the base def is
//! an unresolved-module import — the same classification the no-type-argument
//! path already uses to poison a bare reference to `any`.
//!
//! These run through the real multi-file driver (`crate::driver::compile`) so
//! module resolution actually flags the missing module. Binder names, the
//! aliased import name, and the module specifier vary so the behaviour follows
//! structure, not identifier text.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

const TS2307: u32 = 2307; // Cannot find module
const TS2304: u32 = 2304; // Cannot find name
const TS2322: u32 = 2322; // Type X is not assignable to type Y
const TS2345: u32 = 2345; // Argument of type X is not assignable to parameter Y
const TS2353: u32 = 2353; // Object literal may only specify known properties

/// Compile a single entry file with strict bundler resolution and no lib, so a
/// bare missing-module import is genuinely unresolved.
fn compile_entry(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("entry.ts"), source).expect("write entry file");

    let argv: Vec<&str> = vec![
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
        "entry.ts",
    ];

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// The reported repro: `Gen<T>` inside a `readonly Gen<T>[]` constraint applied
/// to an inferred tuple argument. Only the shared `TS2307` may survive.
#[test]
fn array_constraint_over_unresolved_import_application_no_cascade() {
    let diagnostics = compile_entry(
        r#"
import type { Gen } from "totally-missing-module";
declare function pick<T, Keys extends readonly Gen<T>[]>(data: T, keys: Keys): void;
declare const obj: { a: string; b: string };
pick(obj, ["a"]);
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2307),
        "the unresolved-module import must still report TS2307: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2322) && !codes.contains(&TS2345),
        "applying type args to an unresolved import must not cascade assignability: {diagnostics:?}"
    );
}

/// A single-element `Gen<T>` constraint (no surrounding array) applied to an
/// inferred element.
#[test]
fn single_element_constraint_over_unresolved_import_application_no_cascade() {
    let diagnostics = compile_entry(
        r#"
import type { Box } from "no-such-pkg-xyz";
declare function take<T, V extends Box<T>>(data: T, v: V): void;
take({ a: 1 }, "x");
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2307),
        "the unresolved-module import must still report TS2307: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2322) && !codes.contains(&TS2345),
        "an unresolved-import application must not cascade assignability: {diagnostics:?}"
    );
}

/// An object-literal argument checked against a `Sub<T>` constraint must not
/// raise an excess-property `TS2353` off the poisoned shape.
#[test]
fn object_literal_against_unresolved_import_application_no_excess_property() {
    let diagnostics = compile_entry(
        r#"
import type { Sub } from "missing-mod-abc";
declare function hasSub<T, S extends Sub<T>>(data: T, sub: S): void;
hasSub({ a: 1, b: 2 }, { a: 1, c: 3 });
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2307),
        "the unresolved-module import must still report TS2307: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2353) && !codes.contains(&TS2322) && !codes.contains(&TS2345),
        "an unresolved-import application must not cascade structural errors: {diagnostics:?}"
    );
}

/// Name-independence: renamed alias, renamed binders, and a different module
/// specifier behave identically — the collapse is structural.
#[test]
fn unresolved_import_application_collapse_is_name_independent() {
    let diagnostics = compile_entry(
        r#"
import type { Whatever as Zzz } from "another-missing-specifier";
declare function grab<Q, Arr extends readonly Zzz<Q>[]>(d: Q, a: Arr): void;
declare const o: { x: number; y: number };
grab(o, ["x"]);
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2307),
        "the unresolved-module import must still report TS2307: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2322) && !codes.contains(&TS2345),
        "renamed binders must collapse identically — no cascade: {diagnostics:?}"
    );
}

/// Negative gate: a truly *undeclared* name (no import at all) must keep its
/// `TS2304` and must NOT be silently collapsed to `any`. The fix is scoped to
/// the unresolved-IMPORT path, not all error types.
#[test]
fn undeclared_name_in_constraint_still_reports_ts2304_not_collapsed() {
    let diagnostics = compile_entry(
        r#"
declare function pick<T, Keys extends readonly KeysOfUnion<T>[]>(data: T, keys: Keys): void;
declare const obj: { a: string; b: string };
pick(obj, ["a"]);
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2304),
        "an undeclared name in the constraint must still report TS2304: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2307),
        "there is no import here, so no TS2307 is expected: {diagnostics:?}"
    );
}

/// Control: a *resolved* generic alias applied with a mismatched argument must
/// still report its real error — the collapse must not leak to well-formed
/// generic applications.
#[test]
fn resolved_generic_alias_application_still_reports_real_mismatch() {
    let diagnostics = compile_entry(
        r#"
type Wrap<T> = { value: T };
declare function need(w: Wrap<number>): void;
need({ value: "not a number" });
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&TS2322),
        "a resolved generic application must still report the real mismatch: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&TS2307),
        "there is no missing import here: {diagnostics:?}"
    );
}
