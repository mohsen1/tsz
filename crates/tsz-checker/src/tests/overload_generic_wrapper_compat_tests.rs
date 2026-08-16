//! Structural regression tests for overload↔implementation compatibility
//! (TS2394) over generic-wrapper / conditional-typed return positions.
//!
//! Companion to the unresolved-`Lazy` undetermined-negative guard in
//! `is_implementation_compatible_with_overload_inner`: these lock the
//! resolvable-in-isolation behavior (the wrapper overloads compile clean; a
//! genuine return mismatch still reports TS2394) so the guard cannot silence a
//! real incompatibility. Binder names are varied so no fix can key on an
//! identifier.

use crate::test_utils::check_source_diagnostics;

/// A no-argument overload returning a concrete instantiation of a generic
/// wrapper plus a generic overload whose return is a conditional instantiation
/// of the same wrapper, with an implementation returning the wrapper union.
/// tsc accepts every overload; tsz must not report TS2394 here.
#[test]
fn generic_wrapper_conditional_return_overloads_are_compatible() {
    let diags = check_source_diagnostics(
        r#"
type Pat<a> = a | { [k in keyof a]: Pat<a[k]> };
type UnknownPat = Pat<unknown>;

interface SelP<a, k extends string = never> {
  readonly __sel: a;
  readonly __key: k;
}
interface AnonSelP {
  readonly __anon: true;
}

interface Chain<p, omitted extends string = never> {
  optional(): Chain<p, omitted>;
  with(): Chain<p, omitted>;
}

function pick(): Chain<AnonSelP, never>;
function pick<input, k extends string = never>(
  keyOrPattern: k | (unknown extends input ? UnknownPat : Pat<input>),
): k extends string
  ? Chain<SelP<unknown, k>, never>
  : Chain<SelP<input>, never>;
function pick(
  ..._args: any[]
): Chain<SelP<unknown>, never> | Chain<AnonSelP, never> {
  return undefined as any;
}

export { pick };
"#,
    );

    let ts2394: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        ts2394.is_empty(),
        "wrapper/conditional overloads are compatible with the wrapper-returning \
         implementation; TS2394 must not fire. got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Renamed-binder variant of the same shape: identical structure, completely
/// different identifiers. Locks out any name-based behavior.
#[test]
fn generic_wrapper_conditional_return_overloads_are_compatible_renamed() {
    let diags = check_source_diagnostics(
        r#"
type Shape<q> = q | { [j in keyof q]: Shape<q[j]> };
type AnyShape = Shape<unknown>;

interface Holder<v, hidden extends string = never> {
  readonly __hold: v;
  readonly __hidden: hidden;
}
interface Blank {
  readonly __blank: true;
}

interface Wrap<w, gone extends string = never> {
  maybe(): Wrap<w, gone>;
  also(): Wrap<w, gone>;
}

function grab(): Wrap<Blank, never>;
function grab<src, j extends string = never>(
  keyOrShape: j | (unknown extends src ? AnyShape : Shape<src>),
): j extends string
  ? Wrap<Holder<unknown, j>, never>
  : Wrap<Holder<src>, never>;
function grab(
  ..._rest: any[]
): Wrap<Holder<unknown>, never> | Wrap<Blank, never> {
  return undefined as any;
}

export { grab };
"#,
    );

    let ts2394: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        ts2394.is_empty(),
        "renamed wrapper/conditional overloads must also stay clean of TS2394. got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The `any extends U ? A : B` conditional this file's compatibility guard
/// distributes into `A | B` can sit one alias hop away: `Cond<T>` referenced
/// as `Cond<any>` post-erasure is an `Application` alias reference, not a
/// literal `TypeData::Conditional`, until evaluated. The overload's erased
/// return type only relates to ONE of the two branches directly, so the
/// implementation must be checked against the distributed union, not a
/// single branch. tsc accepts this; tsz must not report TS2394.
#[test]
fn generic_wrapper_conditional_return_via_type_alias_is_compatible() {
    let diags = check_source_diagnostics(
        r#"
type Cond<t> = t extends string ? { tag: "s"; value: string } : { tag: "n"; value: number };

function pick<t>(x: t): Cond<t>;
function pick(x: unknown): { tag: "n"; value: number } {
  return { tag: "n", value: 0 };
}

export { pick };
"#,
    );

    let ts2394: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        ts2394.is_empty(),
        "overload return reached through a conditional-type alias must still distribute \
         its `any`-check-type branches before comparing against the implementation return; \
         TS2394 must not fire. got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Renamed-binder variant of the alias-hop case above: identical structure,
/// completely different identifiers, locking out any name-based behavior.
#[test]
fn generic_wrapper_conditional_return_via_type_alias_is_compatible_renamed() {
    let diags = check_source_diagnostics(
        r#"
type Branch<payload> = payload extends string ? { kind: "text"; body: string } : { kind: "num"; body: number };

function choose<payload>(input: payload): Branch<payload>;
function choose(input: unknown): { kind: "num"; body: number } {
  return { kind: "num", body: 0 };
}

export { choose };
"#,
    );

    let ts2394: Vec<_> = diags.iter().filter(|d| d.code == 2394).collect();
    assert!(
        ts2394.is_empty(),
        "renamed alias-hop conditional-return overload must also stay clean of TS2394. \
         got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Parity floor for the alias-hop case: an implementation return that matches
/// NEITHER distributed branch of the alias-wrapped conditional must still
/// report TS2394. Distribution must widen the compared set, not silence
/// unrelated mismatches.
#[test]
fn generic_wrapper_conditional_return_via_type_alias_genuine_mismatch_still_reports_ts2394() {
    let diags = check_source_diagnostics(
        r#"
type Cond<t> = t extends string ? { tag: "s"; value: string } : { tag: "n"; value: number };

function pick<t>(x: t): Cond<t>;
function pick(x: unknown): boolean {
  return false;
}

export { pick };
"#,
    );

    let ts2394 = diags.iter().filter(|d| d.code == 2394).count();
    assert_eq!(
        ts2394,
        1,
        "an implementation return incompatible with both distributed branches of the \
         alias-wrapped conditional must still report exactly one TS2394. got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Parity floor: a genuine return-type mismatch between an overload and its
/// implementation — with no unresolved reference involved — must still report
/// TS2394. The guard must only suppress *undetermined* negatives, never real
/// ones.
#[test]
fn genuine_return_mismatch_still_reports_ts2394() {
    let diags = check_source_diagnostics(
        r#"
function conv(x: string): string;
function conv(x: number): number {
  return x;
}
export { conv };
"#,
    );

    let ts2394 = diags.iter().filter(|d| d.code == 2394).count();
    assert_eq!(
        ts2394,
        1,
        "an overload whose return ({{string}}) is incompatible with the implementation \
         return ({{number}}) must still report exactly one TS2394. got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Boundary-level guard tests.
//
// These exercise `is_implementation_compatible_with_overload_inner` directly
// against synthesized function types whose return relation depends on an
// unresolved `Lazy(DefId)` — the order/cache-dependent shape that has no
// single-file source witness (it only surfaces when a generic-wrapper return is
// compared before its definition is registered, as in ts-pattern's
// `patterns.ts`). They live in `src/tests/` so the synthesized-type
// construction stays out of checker `src/` proper (the architecture contract
// forbids direct solver-internal construction there).
// ---------------------------------------------------------------------------

use crate::context::{CheckerContext, CheckerOptions};
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefId;
use tsz_solver::{FunctionShape, TypeId};

/// Build a checker over a trivial module and run `probe` against its
/// [`CheckerState`].
fn with_trivial_checker<R>(
    types: &TypeInterner,
    probe: impl FnOnce(&mut CheckerState<'_>) -> R,
) -> R {
    let mut parser = ParserState::new("fixture.ts".to_string(), "export {};".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let arena = parser.get_arena().clone();
    let mut checker = CheckerState {
        ctx: CheckerContext::new(
            &arena,
            &binder,
            types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        ),
    };
    checker.check_source_file(root);
    probe(&mut checker)
}

/// An overload/implementation compatibility verdict whose return relation could
/// only be computed against an *unresolved* `Lazy(DefId)` body is undetermined,
/// not a proven mismatch. The guard in
/// `is_implementation_compatible_with_overload_inner` must report such a pair
/// compatible (no false TS2394). The lazy-resolve-failure counter must advance,
/// proving the verdict actually traveled the unresolved-`Lazy` path, and the
/// guard must flip the raw (unguarded) incompatibility to compatible.
#[test]
fn unresolved_lazy_return_makes_overload_compat_undetermined() {
    let types = TypeInterner::new();
    // Implementation return is a generic wrapper whose definition is not yet
    // registered (an unresolved `Lazy(DefId)`); the overload return is concrete.
    // The structural return relation cannot resolve the wrapper
    // (`note_lazy_resolve_failure`) and degrades to a transient incompatibility.
    let impl_fn = types.function(FunctionShape::new(Vec::new(), types.lazy(DefId(900_001))));
    let overload_fn = types.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    with_trivial_checker(&types, |checker| {
        // Raw structural decision (no guard).
        let before = crate::query_boundaries::common::lazy_resolve_failure_count();
        let raw =
            checker.compute_implementation_compatible_with_overload(impl_fn, overload_fn, false);
        let after = crate::query_boundaries::common::lazy_resolve_failure_count();
        assert!(
            after > before,
            "the unresolved-wrapper return relation must record a lazy-resolve failure \
             (before={before}, after={after}); otherwise the guard is not under test",
        );
        assert!(
            !raw,
            "without the guard, the unresolved-wrapper return relation degrades to an \
             incompatibility — the order/cache-dependent false TS2394",
        );

        // Guarded decision flips the undetermined negative to compatible.
        let compatible =
            checker.is_implementation_compatible_with_overload_inner(impl_fn, overload_fn, false);
        assert!(
            compatible,
            "a compatibility verdict derived from an unresolved wrapper return must be treated \
             as undetermined (compatible), not a TS2394 incompatibility",
        );
    });
}

/// Parity floor at the same boundary: a genuine concrete return mismatch
/// (`string` vs `number`) with no unresolved reference must remain incompatible,
/// so the guard never silences a real TS2394.
#[test]
fn concrete_return_mismatch_stays_incompatible() {
    let types = TypeInterner::new();
    let impl_fn = types.function(FunctionShape::new(Vec::new(), TypeId::NUMBER));
    let overload_fn = types.function(FunctionShape::new(Vec::new(), TypeId::STRING));

    with_trivial_checker(&types, |checker| {
        let before = crate::query_boundaries::common::lazy_resolve_failure_count();
        let compatible =
            checker.is_implementation_compatible_with_overload_inner(impl_fn, overload_fn, false);
        let after = crate::query_boundaries::common::lazy_resolve_failure_count();

        assert_eq!(
            before, after,
            "a concrete primitive return mismatch must not record any lazy-resolve failure",
        );
        assert!(
            !compatible,
            "an overload returning `string` is genuinely incompatible with an implementation \
             returning `number`; this must stay a TS2394 incompatibility",
        );
    });
}
