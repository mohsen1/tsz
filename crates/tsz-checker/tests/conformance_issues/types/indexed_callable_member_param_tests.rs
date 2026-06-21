//! Regression guards: a call whose callee type is a *deferred-but-reducible*
//! form — an indexed access `Interface['callableMember']` reached through a type
//! alias used in **parameter position** — must classify on its apparent
//! (callable) type, not as `NoSignatures`.
//!
//! Unlike a `const`, a parameter keeps its annotation lazy, so the indexed
//! access is only reduced at the call site. When the indexed interface's type
//! has not yet been computed in that context, the lookup-only `resolve_lazy`
//! misses it and the callee is wrongly flagged "not callable" (false TS2349).
//! The call path force-resolves the referenced defs and fully re-evaluates the
//! callee before classifying, matching tsc.
//!
//! Binder names are varied across cases so the behaviour follows the type shape,
//! not a spelling.

use super::super::core::*;

/// Member typed by a *type-alias* to a function: `Box['fetch']` reached through
/// the alias `Extracted`, used as a parameter, must stay callable.
#[test]
fn param_indexed_alias_function_member_is_callable() {
    let diags = compile_and_get_diagnostics(
        r#"
type Fetcher = (id: number) => string;
interface Box { fetch: Fetcher; }
type Extracted = Box['fetch'];
function consume(run: Extracted) { return run(7); }
export { consume };
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — a parameter typed by `Box['fetch']` (a type-alias \
         function member reached through an alias) must remain callable. Actual: {diags:#?}"
    );
}

/// Member typed by a *callable interface*: the indexed access must keep the
/// call signature so the parameter stays callable.
#[test]
fn param_indexed_callable_interface_member_is_callable() {
    let diags = compile_and_get_diagnostics(
        r#"
interface Handler { (event: number): boolean; }
interface Registry { handler: Handler; }
type Pulled = Registry['handler'];
function dispatch(h: Pulled) { return h(1); }
export { dispatch };
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — a parameter typed by `Registry['handler']` (a callable \
         interface member) must remain callable. Actual: {diags:#?}"
    );
}

/// Generic interface: `Cell<unknown>['read']` indexed member must keep its call
/// signature through the alias used in parameter position.
#[test]
fn param_indexed_generic_interface_member_is_callable() {
    let diags = compile_and_get_diagnostics(
        r#"
type Reader = (n: number) => string;
interface Cell<Value> { read: (r: Reader) => Value; }
type ReaderArg = Cell<unknown>['read'];
function apply(fn: ReaderArg) { return fn((n) => "x"); }
export { apply };
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — a parameter typed by `Cell<unknown>['read']` must keep \
         its call signature. Actual: {diags:#?}"
    );
}

/// Resolving the callee through the parameter path must not poison the shared
/// type cache: a sibling `const` annotated by the same alias must stay callable
/// too (the order-sensitive cache-pollution witness).
#[test]
fn indexed_callable_member_alias_does_not_poison_sibling_const() {
    let diags = compile_and_get_diagnostics(
        r#"
type Transform = (s: string) => number;
interface Pipe { step: Transform; }
type Stage = Pipe['step'];
declare const direct: Stage;
const a = direct("x");
function viaParam(stage: Stage) { return stage("y"); }
export { a, viaParam };
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — neither the `const` nor the parameter use of \
         `Pipe['step']` may be flagged not-callable. Actual: {diags:#?}"
    );
}

/// Negative control: the apparent-type resolution must not invent call
/// signatures. A parameter typed by an indexed access of a *non-callable*
/// member, when invoked, must still report TS2349.
#[test]
fn param_indexed_non_callable_member_still_errors() {
    let diags = compile_and_get_diagnostics(
        r#"
interface Record1 { count: number; }
type Counted = Record1['count'];
function misuse(c: Counted) { return c(0); }
export { misuse };
"#,
    );
    assert!(
        has_error(&diags, 2349),
        "TS2349 expected — invoking a parameter typed by `Record1['count']` (a \
         `number`, not callable) must still error. Actual: {diags:#?}"
    );
}
