//! A rest-parameter arrow contextually typed by an OVERLOADED callable takes
//! its rest tuple from the COMBINED signature — the parameter-wise union
//! across same-arity overloads (tsc's `getIntersectedSignatures` feeding the
//! effective rest tuple) — not from the first overload.
//!
//! Witness (zustand devtools, canary Family A): `StoreApi<S>['setState']`
//! with two `setState` overloads `(a: T, b?: false)` / `(a: T, b: true)`
//! must type `(...a)` as `[S, (boolean | undefined)?]` so the arrow
//! satisfies BOTH overloads; the first-overload tuple `[S, false |
//! undefined]` fails the second and produced a false TS2322. Optionality
//! merges per index (optional when ANY overload marks it optional) so
//! call-site arity is unaffected.

use crate::test_utils::check_source_diagnostics;

fn ts2322_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .count()
}

const OVERLOADED_STORE: &str = r#"
interface StoreApi<T> {
  setState: { (a: T, b?: false): void; (a: T, b: true): void }
}
"#;

/// The canary witness: rest-param arrow against the alias-arg indexed access.
#[test]
fn rest_arrow_satisfies_overloaded_member_via_alias_arg() {
    let source = format!(
        "{OVERLOADED_STORE}
type S = {{ a: number }}
export function wire() {{
  const s: StoreApi<S>['setState'] = (...a) => {{}}
}}
"
    );
    assert_eq!(ts2322_count(&source), 0, "tsc accepts the rest-param arrow");
}

/// Inline-argument form (was already green; pins that the merge did not
/// regress it). Binder names varied.
#[test]
fn rest_arrow_satisfies_overloaded_member_via_inline_arg() {
    let source = format!(
        "{OVERLOADED_STORE}
export function hook() {{
  const s: StoreApi<{{ n: string }}>['setState'] = (...rest) => {{}}
}}
"
    );
    assert_eq!(ts2322_count(&source), 0, "inline-arg form stays accepted");
}

/// Function-local alias over the enclosing generic — the zustand shape.
#[test]
fn rest_arrow_satisfies_overloaded_member_via_local_alias() {
    let source = format!(
        "{OVERLOADED_STORE}
export function wire<T>(fn: () => T) {{
  type S = ReturnType<typeof fn> & {{ [k: string]: ReturnType<typeof fn> }}
  const s: StoreApi<S>['setState'] = (...a) => {{}}
}}
"
    );
    assert_eq!(
        ts2322_count(&source),
        0,
        "local-alias generic form accepted"
    );
}

/// The merged tuple's second element unions the overloads' parameter types
/// (`false | undefined` with `true` = `boolean | undefined`), revealed via a
/// deliberate never-assignment.
#[test]
fn merged_rest_tuple_unions_parameter_types_across_overloads() {
    let source = format!(
        "{OVERLOADED_STORE}
type S = {{ a: number }}
export function reveal() {{
  const s: StoreApi<S>['setState'] = (...a) => {{ const n: never = a }}
}}
"
    );
    let diags = check_source_diagnostics(&source);
    let reveal = diags
        .iter()
        .find(|d| d.code == 2322 && d.message_text.contains("'never'"))
        .expect("the never-assignment must fail");
    assert!(
        reveal.message_text.contains("boolean | undefined"),
        "the rest tuple must union the overload parameter types, got: {}",
        reveal.message_text
    );
    assert!(
        !reveal.message_text.contains("false | undefined]"),
        "the rest tuple must not be the first overload's shape, got: {}",
        reveal.message_text
    );
}

/// Negative control: a PRE-DECLARED mono-signature value (not a
/// context-sensitive arrow) is genuinely not assignable to the overloaded
/// member — tsc rejects it too.
#[test]
fn predeclared_mono_signature_still_fails_overloaded_member() {
    let source = format!(
        "{OVERLOADED_STORE}
type S = {{ a: number }}
declare const fixed: (...a: [a: S, b?: false | undefined]) => void
const t: StoreApi<S>['setState'] = fixed
"
    );
    assert_eq!(
        ts2322_count(&source),
        1,
        "a pre-declared first-overload-shaped value must keep failing"
    );
}

/// Negative control: merged optionality must not require the optional
/// parameter at call sites through the contextual arrow.
#[test]
fn merged_tuple_keeps_single_argument_calls_working() {
    let source = format!(
        "{OVERLOADED_STORE}
type S = {{ a: number }}
export function relay(api: StoreApi<S>) {{
  const s: StoreApi<S>['setState'] = (...a) => {{ api.setState(...a) }}
  s({{ a: 1 }})
}}
"
    );
    let diags = check_source_diagnostics(&source);
    assert!(
        diags.is_empty(),
        "single-argument dispatch through the merged tuple must stay clean, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The full zustand devtools chain (canary Family A, final leg): an arrow
/// with NAMED parameters cast to `NamedSet<S>` — a conditional that captures
/// the two `setState` overloads with `infer` and rebuilds them with
/// `[...TakeTwo<Sa>, action?: Action]` variadic rest tuples. The multi-sig
/// contextual callable must keep its overload set (no mono-collapse) and
/// each signature's deferred rest tuple must env-evaluate so the parameters
/// contextually type (tsc reports nothing here; tsz emitted 2x TS7006).
#[test]
fn named_params_arrow_casts_to_infer_rebuilt_overload_set() {
    let source = r#"
type SetStateInternal<T> = {
  _(
    partial: T | Partial<T> | { _(state: T): T | Partial<T> }['_'],
    replace?: false,
  ): void
  _(state: T | { _(state: T): T }['_'], replace: true): void
}['_']
interface StoreApi<T> {
  setState: SetStateInternal<T>
}
type Cast<T, U> = T extends U ? T : U
type TakeTwo<T> = T extends { length: 2 }
  ? T
  : T extends [infer A0, (infer A1)?, ...unknown[]]
    ? [A0, A1?]
    : never
type Action = string | { type: string }
type StoreDevtools<S> = S extends {
  setState: {
    (...args: infer Sa1): infer Sr1
    (...args: infer Sa2): infer Sr2
  }
}
  ? {
      setState(...args: [...args: TakeTwo<Sa1>, action?: Action]): Sr1
      setState(...args: [...args: TakeTwo<Sa2>, action?: Action]): Sr2
    }
  : never
export type NamedSet<T> = StoreDevtools<StoreApi<T>>['setState']
type S = { a: number }
export function wire(api: StoreApi<S>) {
  api.setState = ((state, replace, nameOrAction: Action) => {
    void state
    void replace
    void nameOrAction
  }) as NamedSet<S>
}
"#;
    let diags = check_source_diagnostics(source);
    let implicit_any = diags.iter().filter(|d| d.code == 7006).count();
    assert_eq!(
        implicit_any,
        0,
        "state/replace must be contextually typed through the rebuilt overload set, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    assert!(
        !diags.iter().any(|d| d.code == 2352),
        "the cast must sufficiently overlap, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
