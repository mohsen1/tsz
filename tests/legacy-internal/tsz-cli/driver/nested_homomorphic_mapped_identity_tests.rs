//! Parity guard for nested homomorphic *identity* mapped types
//! (`Id<Id<T>>`, the `Prettify<Prettify<T>>` idiom).
//!
//! `tsc` reduces `keyof { [P in keyof S]: S[P] }` to `keyof S` and
//! `{ [P in keyof S]: S[P] }[K]` to `S[K]`, so a homomorphic identity mapped
//! type is interchangeable with its source. A generic source `T` therefore
//! relates to `Id<Id<T>>` exactly as it does to the single-level `Id<T>`
//! (`tsc` is clean). tsz used to emit a false `TS2322` because the relation's
//! homomorphic-mapped recognition stopped at the inner mapped type instead of
//! peeling it to its underlying type parameter.
//!
//! Owner: solver relation layer (`is_assignable_to_homomorphic_mapped` /
//! `homomorphic_mapped_constraint_source`). These cases pin the structural
//! shape — binder/alias spellings are varied across cases so the guard follows
//! the structure rather than any identifier (anti-hardcoding).

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// Write `source` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the single-file project compile. Returns every emitted diagnostic.
fn compile_single(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let tsconfig = r#"{ "compilerOptions": { "strict": true, "target": "es2022", "lib": ["es2022"], "noEmit": true }, "files": ["main.ts"] }"#;
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    fs::write(dir.path().join("main.ts"), source).expect("write source");

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

/// TS2322 (type not assignable) — the family the missing homomorphic-identity
/// peel produced.
fn assignability_errors(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn no_errors() -> Vec<(u32, String)> {
    Vec::new()
}

// ---- Positive cases (tsc-clean; tsz must match) ----

/// `T <: Id<Id<T>>` — the reported residual. Two-level homomorphic identity.
#[test]
fn source_param_assignable_to_double_nested_identity() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
function widen<T>(x: T): Id<Id<T>> { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags),
        no_errors(),
        "T must be assignable to Id<Id<T>> (keyof/index identity collapses the nesting)"
    );
}

/// Triple nesting and a differently-spelled alias/parameter still collapse.
#[test]
fn source_param_assignable_to_triple_nested_identity_renamed() {
    let diags = compile_single(
        r#"
type Same<U> = { [P in keyof U]: U[P] };
function pass<Widget>(value: Widget): Same<Same<Same<Widget>>> { return value; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags),
        no_errors(),
        "renamed triple-nested homomorphic identity must still accept the source param"
    );
}

/// Reverse direction: `Id<Id<T>> <: T` (the mapped source relates back to T).
#[test]
fn double_nested_identity_assignable_to_source_param() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
function narrow<Shape>(x: Id<Id<Shape>>): Shape { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags),
        no_errors(),
        "Id<Id<Shape>> must be assignable back to Shape"
    );
}

/// A `readonly` outer wrapper over an identity inner mapped type still peels the
/// inner source. `readonly` does not change assignability from the source.
#[test]
fn readonly_over_identity_inner_accepts_source_param() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
type Frozen<T> = { readonly [K in keyof T]: T[K] };
function freeze<T>(x: T): Frozen<Id<T>> { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags),
        no_errors(),
        "T must be assignable to Readonly<Id<T>> (inner identity peeled, readonly is irrelevant)"
    );
}

/// `Pick`-shape (`{ [P in K]: S[P] }`, constraint a key subset rather than
/// `keyof S`) over an identity inner wrapper still peels to the source, so the
/// source type parameter supplies every picked property.
#[test]
fn pick_shape_over_identity_inner_accepts_source_param() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
type Subset<T, K extends keyof T> = { [P in K]: T[P] };
function take<T, K extends keyof T & keyof Id<T>>(x: T): Subset<Id<T>, K> { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags),
        no_errors(),
        "T must be assignable to a Pick-shape over Id<T> (inner identity peeled)"
    );
}

// ---- Negative controls (tsc errors; tsz must keep erroring) ----

/// `Required<Id<T>>` removes optionality, so `T` (which may carry optional
/// members) is NOT assignable. The peel must not over-accept here.
#[test]
fn required_over_identity_inner_still_rejects_source_param() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
type AllRequired<T> = { [K in keyof T]-?: T[K] };
function demand<T>(x: T): AllRequired<Id<T>> { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags).len(),
        1,
        "T must NOT be assignable to Required<Id<T>> (optionality removal narrows the target)"
    );
}

/// Nesting around a *concrete* inner source is not interchangeable with a bare
/// type parameter. `T <: Id<Id<{ a: number }>>` must still fail.
#[test]
fn concrete_inner_nesting_still_rejects_source_param() {
    let diags = compile_single(
        r#"
type Id<T> = { [K in keyof T]: T[K] };
function bad<T>(x: T): Id<Id<{ a: number }>> { return x; }
"#,
    );
    assert_eq!(
        assignability_errors(&diags).len(),
        1,
        "T must NOT be assignable to a nested identity over a concrete object shape"
    );
}
