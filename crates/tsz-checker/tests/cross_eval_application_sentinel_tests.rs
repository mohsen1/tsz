//! Regression tests for issue #13508 (root cause B, cross-evaluator slice):
//! a fresh `TypeEvaluator` spun up mid-relation or mid-infer-match must not
//! re-expand an `Application` node that another evaluator in the same session
//! is already expanding. The session sentinel defers the re-entry and the
//! in-flight owner produces the result, mirroring `tsc`'s `resolvingType`
//! in-progress marker.
//!
//! Structural rule: when `Application(DefId, args)` is in flight in the
//! current evaluation session and the same node is entered again through a
//! fresh evaluator, tsc answers with the in-progress instantiation instead of
//! re-instantiating; tsz defers through
//! `EvaluationSession::enter_application_expansion` and keeps the application
//! opaque for the deferring evaluator.
//!
//! The CPU-bound termination win on the typebox canary row is owned by the
//! ready-review `project-compile-guard`; these tests pin semantics — the
//! sentinel must not erase true diagnostics, invent false ones, or swallow
//! TS2589 on genuinely divergent aliases.

use tsz_checker::test_utils::check_source_codes;

/// A typebox-engine-shaped mutually-recursive alias interpreter evaluated at
/// a concrete use site: the true assignment must hold with no TS2589 and no
/// false TS2322 (the sentinel defers only concurrent re-entry, never the
/// owning expansion).
#[test]
fn mutually_recursive_interpreter_concrete_use_converges() {
    let codes = check_source_codes(
        r#"
interface TSchema { static: unknown }
interface TStr extends TSchema { kind: 'str' }
interface TNum extends TSchema { kind: 'num' }
interface TArr<I extends TSchema> extends TSchema { kind: 'arr'; items: I }
interface TObj<P extends Record<string, TSchema>> extends TSchema { kind: 'obj'; props: P }
type SProps<P extends Record<string, TSchema>,
  Result = { [K in keyof P]: SType<P[K]> }
> = Result;
type SArr<I extends TSchema, Result = SType<I>[]> = Result;
type SType<T extends TSchema> =
  T extends TArr<infer I extends TSchema> ? SArr<I> :
  T extends TObj<infer P extends Record<string, TSchema>> ? SProps<P> :
  T extends TStr ? string :
  T extends TNum ? number :
  unknown;
declare function parse<T extends TSchema>(t: T): SType<T>;
declare const schema: TObj<{ a: TStr; b: TArr<TNum>; c: TObj<{ d: TStr }> }>;
const ok: { a: string; b: number[]; c: { d: string } } = parse(schema);
"#,
    );
    assert!(
        !codes.contains(&2589),
        "concrete interpreter use must converge (no TS2589). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "true assignment must hold (no false TS2322 from deferral). Got: {codes:?}"
    );
}

/// Renamed-binder variant of the same shape: the sentinel keys on structure
/// (interned application nodes), never on identifier spellings.
#[test]
fn mutually_recursive_interpreter_renamed_binders() {
    let codes = check_source_codes(
        r#"
interface Zeta { static: unknown }
interface Qs extends Zeta { kind: 'qs' }
interface Qn extends Zeta { kind: 'qn' }
interface Qa<Elem extends Zeta> extends Zeta { kind: 'qa'; items: Elem }
interface Qo<Fields extends Record<string, Zeta>> extends Zeta { kind: 'qo'; props: Fields }
type WalkFields<Fields extends Record<string, Zeta>,
  Out = { [Name in keyof Fields]: Walk<Fields[Name]> }
> = Out;
type WalkList<Elem extends Zeta, Out = Walk<Elem>[]> = Out;
type Walk<Node extends Zeta> =
  Node extends Qa<infer Elem extends Zeta> ? WalkList<Elem> :
  Node extends Qo<infer Fields extends Record<string, Zeta>> ? WalkFields<Fields> :
  Node extends Qs ? string :
  Node extends Qn ? number :
  unknown;
declare function decode<Node extends Zeta>(n: Node): Walk<Node>;
declare const tree: Qo<{ x: Qn; y: Qa<Qs> }>;
const ok: { x: number; y: string[] } = decode(tree);
"#,
    );
    assert!(
        !codes.contains(&2589) && !codes.contains(&2322),
        "renamed binders must behave identically. Got: {codes:?}"
    );
}

/// Negative case: a genuinely wrong assignment through the same recursive
/// interpreter must still fail. The sentinel's deferral must never make a
/// mismatch pass by leaving the source opaque.
#[test]
fn mutually_recursive_interpreter_wrong_assignment_still_errors() {
    let codes = check_source_codes(
        r#"
interface TSchema { static: unknown }
interface TStr extends TSchema { kind: 'str' }
interface TNum extends TSchema { kind: 'num' }
interface TArr<I extends TSchema> extends TSchema { kind: 'arr'; items: I }
interface TObj<P extends Record<string, TSchema>> extends TSchema { kind: 'obj'; props: P }
type SProps<P extends Record<string, TSchema>,
  Result = { [K in keyof P]: SType<P[K]> }
> = Result;
type SType<T extends TSchema> =
  T extends TArr<infer I extends TSchema> ? SType<I>[] :
  T extends TObj<infer P extends Record<string, TSchema>> ? SProps<P> :
  T extends TStr ? string :
  T extends TNum ? number :
  unknown;
declare function parse<T extends TSchema>(t: T): SType<T>;
declare const schema: TObj<{ a: TStr; b: TArr<TNum> }>;
const bad: { a: number; b: string[] } = parse(schema);
"#,
    );
    assert!(
        codes.contains(&2322),
        "a wrong assignment must still produce TS2322. Got: {codes:?}"
    );
}

/// TS2589 parity: a genuinely divergent recursive alias (unbounded growth,
/// never convergent) must still surface TS2589 — the sentinel must not turn
/// the divergence into a silent opaque type. The depth-detection pass is
/// exempt from the sentinel precisely so this keeps firing.
#[test]
fn divergent_alias_still_reports_ts2589() {
    let codes = check_source_codes(
        r#"
type Grow<T> = Grow<[T, T]>;
type Boom = Grow<1>;
"#,
    );
    assert!(
        codes.contains(&2589) || codes.contains(&2456) || codes.contains(&2315),
        "a divergent alias must keep its circularity/depth diagnostic. Got: {codes:?}"
    );
}

/// Generic (substitution-dependent) declaration-graph form: checking the
/// aliases themselves — with free type parameters everywhere — must not hang
/// or introduce diagnostics. This is the typebox `properties.ts` accumulator
/// trampoline shape (defaulted type parameters computed from earlier ones).
#[test]
fn generic_accumulator_trampolines_check_clean() {
    let codes = check_source_codes(
        r#"
interface TSchema { static: unknown }
interface TOpt<T extends TSchema> extends TSchema { opt: true; inner: T }
type OptionalKeys<P extends Record<string, TSchema>,
  Result extends PropertyKey = { [K in keyof P]: P[K] extends TOpt<TSchema> ? K : never }[keyof P]
> = Result;
type RequiredKeys<P extends Record<string, TSchema>,
  Result extends PropertyKey = Exclude<keyof P, OptionalKeys<P>>
> = Result;
type WithModifiers<P extends Record<string, TSchema>, V extends Record<PropertyKey, unknown>,
  Result = Partial<Pick<V, OptionalKeys<P>>> & Required<Pick<V, RequiredKeys<P>>>
> = Result;
type Walk<P extends Record<string, TSchema>,
  Bare extends Record<PropertyKey, unknown> = { [K in keyof P]: P[K]['static'] },
  Result = WithModifiers<P, Bare>
> = Result;
declare function walk<P extends Record<string, TSchema>>(p: P): Walk<P>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "generic trampoline declarations must not trip TS2589. Got: {codes:?}"
    );
}
