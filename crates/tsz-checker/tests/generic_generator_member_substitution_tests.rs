//! Regression tests for issue #14235: a generic lib interface declared in its
//! own lib file (`Generator` in es2015.generator.d.ts, `AsyncGenerator` in
//! es2018.asyncgenerator.d.ts) must substitute the receiver's type arguments
//! when a member is accessed through a still-generic application.
//!
//! Root cause: `type_reference_symbol_type_with_params` pushed the interface's
//! type-parameter nodes against the *current file* arena. For a cross-arena lib
//! interface the `NodeIndex`es collided with unrelated current-file nodes, so
//! `Generator<T, TReturn, TNext>` referenced inside `take<Y, R>(...)` resolved
//! to a single bogus `R` instead of its three parameters. The merged body kept
//! the owner-arena parameter identities, so the substitution built from the
//! mismatched parameters was a no-op and `g.next().value` leaked the
//! interface's own `T | TReturn` — a false TS2322 against the declared `Y | R`.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostics(
    source: &str,
    target: ScriptTarget,
    lib_files: &[Arc<LibFile>],
) -> Vec<(u32, String)> {
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target,
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        lib_files,
    )
    .into_iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

/// `g.next().value` on a generic `Generator<Y, R>` resolves to `Y | R`, so the
/// declared return type is satisfied and tsc emits nothing.
#[test]
fn generic_generator_next_value_substitutes_receiver_type_args() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<Y, R>(g: Generator<Y, R>): Y | R { return g.next().value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "generic Generator<Y, R>.next().value must resolve to Y | R, got: {diags:?}",
    );
}

/// `return` and `throw` share the same `IteratorResult<T, TReturn>` member
/// shape and must substitute identically.
#[test]
fn generic_generator_return_value_substitutes_receiver_type_args() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<Y, R>(g: Generator<Y, R>): Y | R { return g.return(undefined as any).value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "generic Generator<Y, R>.return().value must resolve to Y | R, got: {diags:?}",
    );
}

/// The binder names must not drive the fix: renaming `Y, R` to `A, B, C` keeps
/// the result clean.
#[test]
fn generic_generator_renamed_binders_substitute() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<A, B, C>(g: Generator<A, B, C>): A | B { return g.next().value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "renamed-binder generic Generator must resolve correctly, got: {diags:?}",
    );
}

/// `AsyncGenerator` (declared in its own es2018.asyncgenerator.d.ts file) is the
/// same cross-arena shape and must substitute the receiver type args too.
#[test]
fn generic_async_generator_next_value_substitutes_receiver_type_args() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export async function take<Y, R>(g: AsyncGenerator<Y, R>): Promise<Y | R> { return (await g.next()).value; }",
        ScriptTarget::ESNext,
        &lib_files,
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2322),
        "generic AsyncGenerator<Y, R>.next().value must resolve to Y | R, got: {diags:?}",
    );
}

/// Reversed declaration order of the binders must not change the result: the
/// substitution keys off the receiver's argument positions, not the enclosing
/// function's declaration order.
#[test]
fn generic_generator_reversed_binder_order_still_errors() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<R, Y>(g: Generator<Y, R>): boolean { return g.next().value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "reversed binder order must still report TS2322, got: {diags:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("R | Y") && !msg.contains("TReturn")),
        "TS2322 source must be the substituted union, got: {ts2322:?}",
    );
}

/// The base `Iterator` interface (its own lib file, same cross-arena shape)
/// must substitute identically; renamed binders prove no name coupling.
#[test]
fn generic_iterator_next_value_genuine_mismatch_errors() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<A, B>(it: Iterator<A, B>): boolean { return it.next().value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Iterator<A, B>.next().value against boolean must report TS2322, got: {diags:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("A | B") && !msg.contains("TReturn")),
        "TS2322 source must be the substituted `A | B`, got: {ts2322:?}",
    );
}

/// `AsyncGenerator` negative control: the awaited `.next()` result's `value`
/// must carry the substituted union, so a wrong annotation still errors.
#[test]
fn generic_async_generator_genuine_mismatch_still_errors() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export async function take<Y, R>(g: AsyncGenerator<Y, R>): Promise<boolean> { return (await g.next()).value; }",
        ScriptTarget::ESNext,
        &lib_files,
    );
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "AsyncGenerator<Y, R> awaited next().value against boolean must report TS2322, got: {diags:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("R | Y") && !msg.contains("TReturn")),
        "TS2322 source must be the substituted union, got: {ts2322:?}",
    );
}

/// Negative control: a genuinely wrong annotation must still report TS2322, and
/// the diagnostic must show the *substituted* `Y | R` source (proving the
/// member type was instantiated), not the leaked `T | TReturn`.
#[test]
fn generic_generator_genuine_mismatch_still_errors_with_substituted_source() {
    let lib_files = load_default_lib_files();
    if lib_files.is_empty() {
        return;
    }
    let diags = diagnostics(
        "export function take<Y, R>(g: Generator<Y, R>): boolean { return g.next().value; }",
        ScriptTarget::ES2015,
        &lib_files,
    );
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "assigning Generator<Y, R>.next().value to boolean must report TS2322, got: {diags:?}",
    );
    // tsc 7.0.2 renders the substituted union as `R | Y` on this fixture
    // (`Type 'R | Y' is not assignable to type 'boolean'.`), so pin that
    // exact spelling rather than an order the oracle never prints.
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("R | Y") && !msg.contains("TReturn")),
        "TS2322 source must be the substituted `R | Y`, not the leaked `T | TReturn`, got: {ts2322:?}",
    );
}
