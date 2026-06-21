//! Repro + adjacent matrix for the ts-rest `ClientInferRequestBase`
//! false-positive: a type-parameter *default* of the form
//! `H = 'k' extends keyof T ? X : never` must resolve its `keyof T` against the
//! supplied argument for `T`, even when that argument is held as an unresolved
//! semantic ref (`Lazy(DefId)`), so the conditional picks the true branch and the
//! defaulted member survives a surrounding key-remapping mapped type (`Without`).
//!
//! Structural rule: when a generic type alias' parameter default is a conditional
//! whose `check`/`extends` still holds an unresolved `Lazy(DefId)`/`Recursive`
//! ref after substitution (e.g. `keyof T` with `T := Lazy(Route)`), tsc resolves
//! the alias and picks the branch through the resolver; tsz must NOT pick the
//! branch with the resolver-less subtype check used during default resolution
//! (which silently answers `false` and bakes the false branch into the default).
//! The owner is `maybe_evaluate_concrete_conditional`
//! (`crates/tsz-solver/src/instantiation/instantiate/api.rs`), which now defers
//! such conditionals so the later resolver-aware evaluator picks the branch.
//!
//! Kill switch: `TSZ_DISABLE_DEFER_LAZY_DEFAULT_CONDITIONAL=1` restores the prior
//! eager (resolver-less) behavior.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn count_code(diags: &[u32], expected: u32) -> usize {
    diags.iter().filter(|&&c| c == expected).count()
}

/// Core witness: the defaulted `headers` member (`H = 'headers' extends keyof T
/// ? { h: 1 } : never`) must survive the `Without` mapped type. Destructuring it
/// off the alias must NOT raise TS2339.
#[test]
fn keyof_default_conditional_member_survives_without() {
    let diags = codes(
        r#"
type Without<S, V> = { [Q in keyof S as S[Q] extends V ? never : Q]: S[Q] };
type Prettify<U> = { [W in keyof U]: U[W] } & {};
interface Route { body: { x: number }; method: 'POST'; headers?: unknown }
type CIB<
  T extends Route,
  H = 'headers' extends keyof T ? { h: 1 } : never
> = Prettify<
  Without<
    { body: T extends { method: 'POST' } ? T['body'] : never; query: never; headers: H },
    never
  >
>;
declare const inputArgs: CIB<Route> | undefined;
const { body, headers } = (inputArgs as CIB<Route> & { next?: any }) || {};
body; headers;
"#,
    );
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "defaulted `headers` member must survive `Without`; got {diags:#?}"
    );
}

/// Anti-hardcoding: rename every binder (alias names, parameters, the probed
/// key, the kept member). The rule is structural, not name-driven.
#[test]
fn keyof_default_conditional_rule_is_binder_name_independent() {
    let diags = codes(
        r#"
type Strip<Src, Drop> = { [Idx in keyof Src as Src[Idx] extends Drop ? never : Idx]: Src[Idx] };
type Flatten<Whole> = { [Field in keyof Whole]: Whole[Field] } & {};
interface Endpoint { payload: { y: string }; verb: 'PUT'; meta?: unknown }
type Shape<
  E extends Endpoint,
  Tag = 'meta' extends keyof E ? { tag: 1 } : never
> = Flatten<
  Strip<
    { payload: E extends { verb: 'PUT' } ? E['payload'] : never; nope: never; meta: Tag },
    never
  >
>;
declare const raw: Shape<Endpoint> | undefined;
const { payload, meta } = (raw as Shape<Endpoint> & { trailer?: any }) || {};
payload; meta;
"#,
    );
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "renamed-binder form must also keep the defaulted member; got {diags:#?}"
    );
}

/// The same default-conditional kept member must survive `Without` even when the
/// mapped source is an intersection `{obj} & Application` (the original ts-rest
/// `& ExtractExtraParametersFromClientArgs<...>` shape).
#[test]
fn keyof_default_conditional_member_survives_without_over_intersection() {
    let diags = codes(
        r#"
type Without<S, V> = { [Q in keyof S as S[Q] extends V ? never : Q]: S[Q] };
type Prettify<U> = { [W in keyof U]: U[W] } & {};
interface Box<P> { boxed: P }
interface Route { body: { x: number }; method: 'POST'; headers?: unknown }
type CIB<
  T extends Route,
  H = 'headers' extends keyof T ? { h: 1 } : never
> = Prettify<
  Without<
    { body: T['body']; headers: H } & Box<number>,
    never
  >
>;
declare const inputArgs: CIB<Route> | undefined;
const { body, headers, boxed } = (inputArgs as CIB<Route> & { next?: any }) || {};
body; headers; boxed;
"#,
    );
    assert_eq!(
        count_code(&diags, 2339),
        0,
        "defaulted member must survive `Without` over an intersection w/ Application; got {diags:#?}"
    );
}

/// Negative control: a member whose value is genuinely `never` must STILL be
/// filtered out by `Without`, so destructuring it raises TS2339 (matching tsc).
/// The deferral must not resurrect a correctly-dropped member.
#[test]
fn never_valued_member_is_still_filtered_by_without() {
    let diags = codes(
        r#"
type Without<S, V> = { [Q in keyof S as S[Q] extends V ? never : Q]: S[Q] };
type Prettify<U> = { [W in keyof U]: U[W] } & {};
interface Route { body: { x: number }; method: 'POST'; headers?: unknown }
type CIB<
  T extends Route,
  H = 'headers' extends keyof T ? { h: 1 } : never
> = Prettify<
  Without<
    { body: T['body']; headers: H; gone: never },
    never
  >
>;
declare const inputArgs: CIB<Route> | undefined;
const { body, gone } = (inputArgs as CIB<Route> & { next?: any }) || {};
body; gone;
"#,
    );
    assert_eq!(
        count_code(&diags, 2339),
        1,
        "the `never`-valued member must remain filtered out (exactly one TS2339); got {diags:#?}"
    );
}
