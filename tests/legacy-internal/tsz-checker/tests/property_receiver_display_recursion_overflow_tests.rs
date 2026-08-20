//! Regression tests for stack overflow in the property-receiver application
//! display normalization pre-pass (`normalize_property_receiver_application_*`
//! in `error_reporter::core::type_display`).
//!
//! Structural rule: when a diagnostic must render a property-receiver type, the
//! checker runs a cosmetic normalization pre-pass that resolves `Lazy`
//! references, widens fresh literals, and re-applies display aliases. That
//! pre-pass recurses structurally through applications, unions, intersections,
//! and object shapes, and re-enters on each resolved `Lazy` reference using a
//! *fresh* evaluation budget. On deeply self-expanding generic types — e.g. the
//! higher-kinded middleware-mutator chains in `zustand` / `jotai` / `arktype`,
//! where evaluating one layer yields another anonymous layer that is still lazy
//! — the recursion never reaches a non-lazy fixpoint and overflows the worker
//! stack (`SIGABRT`, issue #12455).
//!
//! The downstream solver diagnostic formatter already bounds nested type
//! printing (`max_depth = 8`, long-receiver elision by depth 26), so the
//! pre-pass is capped at a depth far above any visible nesting: once the bound
//! is reached the type is returned unchanged, which bottoms out the recursion
//! without altering any rendered diagnostic.
//!
//! These tests exercise recursive / higher-kinded receiver shapes through the
//! diagnostic display path and assert that they terminate and render their
//! aliased form (matching `tsc`). Binder names are varied so no fixture-name or
//! identifier string can drive the behavior.

use crate::test_utils::check_source_diagnostics;

/// Run the checker purely to confirm the display path terminates (no stack
/// overflow / non-termination) on the given source.
fn assert_terminates(source: &str) {
    let _ = check_source_diagnostics(source);
}

fn message_for_code(source: &str, code: u32) -> String {
    let diags = check_source_diagnostics(source);
    diags
        .iter()
        .find(|d| d.code == code)
        .map(|d| d.message_text.clone())
        .unwrap_or_else(|| {
            panic!(
                "expected a TS{code} diagnostic; got {:?}",
                diags
                    .iter()
                    .map(|d| (d.code, &d.message_text))
                    .collect::<Vec<_>>()
            )
        })
}

/// A self-referential object alias used as a property-receiver renders via its
/// alias name and does not expand (or overflow) while reporting TS2339.
#[test]
fn recursive_object_alias_receiver_renders_via_alias() {
    let source = r#"
type Rec<T> = { next: Rec<T>; payload: T };
declare const value: Rec<number>;
value.missingProperty;
"#;
    let message = message_for_code(source, 2339);
    assert!(
        message.contains("Rec<number>"),
        "receiver should display via its alias, got: {message}"
    );
    // The body must not be expanded into the rendered receiver.
    assert!(
        !message.contains("next:"),
        "recursive body must not be eagerly expanded into the message, got: {message}"
    );
}

/// A homomorphic-mapped (`Identity`) wrapper over a self-referential body still
/// terminates and renders via its alias when a property is missing.
#[test]
fn mapped_wrapped_self_referential_receiver_terminates() {
    let source = r#"
type Identity<O> = { [K in keyof O]: O[K] };
type Grow<T> = Identity<{ head: T; tail: Grow<T> }>;
declare function build<T>(seed: T): Grow<T>;
const out: { totallyDifferent: true } = build(123);
"#;
    let message = message_for_code(source, 2741);
    assert!(
        message.contains("Grow<number>"),
        "receiver should display via its alias, got: {message}"
    );
}

/// Faithful (reduced) model of the `zustand` higher-kinded mutator chain that
/// triggered the original SIGABRT: a recursive conditional (`Mutate`) that peels
/// a tuple tail and re-indexes a declaration-merged interface. Assigning an
/// incompatible value forces the deep `Mutate<...>` receiver to be displayed.
const MUTATOR_CHAIN_DEFS: &str = r#"
interface StoreMutators<S, A> {}
type StoreMutatorIdentifier = keyof StoreMutators<unknown, unknown>;

type Mutate<S, Ms> = number extends Ms['length' & keyof Ms]
  ? S
  : Ms extends []
    ? S
    : Ms extends [[infer Mi, infer Ma], ...infer Mrs]
      ? Mutate<StoreMutators<S, Ma>[Mi & StoreMutatorIdentifier], Mrs>
      : never;

interface StoreApi<T> {
  setState: (partial: T) => void;
  getState: () => T;
}

type StateCreator<
  T,
  Mis extends [StoreMutatorIdentifier, unknown][] = [],
  Mos extends [StoreMutatorIdentifier, unknown][] = [],
  U = T,
> = (store: Mutate<StoreApi<T>, Mis>) => U;
"#;

/// Assigning a non-function to a `StateCreator<...>` (whose body embeds the
/// recursive `Mutate<...>`) reports TS2322 and terminates.
#[test]
fn zustand_style_mutator_chain_mismatch_terminates() {
    let source = format!(
        r#"
{MUTATOR_CHAIN_DEFS}
const bad: StateCreator<{{ n: number }}, [['m/a', never], ['m/b', never]]> = 42;
"#
    );
    let message = message_for_code(&source, 2322);
    assert!(
        message.contains("StateCreator"),
        "target should display via its alias, got: {message}"
    );
}

/// Same shape as above with every binder renamed: the behavior must not depend
/// on any identifier string (anti-hardcoding).
#[test]
fn zustand_style_mutator_chain_renamed_binders_terminates() {
    let source = r#"
interface MergedKinds<St, Ar> {}
type KindId = keyof MergedKinds<unknown, unknown>;

type Fold<St, Ks> = number extends Ks['length' & keyof Ks]
  ? St
  : Ks extends []
    ? St
    : Ks extends [[infer Ki, infer Ka], ...infer Kr]
      ? Fold<MergedKinds<St, Ka>[Ki & KindId], Kr>
      : never;

interface Handle<V> {
  put: (next: V) => void;
  peek: () => V;
}

type Builder<V, Ins extends [KindId, unknown][] = [], Out = V> =
  (handle: Fold<Handle<V>, Ins>) => Out;

const wrong: Builder<{ q: string }, [['k/x', never], ['k/y', never]]> = 7;
"#;
    let message = message_for_code(source, 2322);
    assert!(
        message.contains("Builder"),
        "target should display via its alias, got: {message}"
    );
}

/// A genuinely incompatible plain assignment must still report TS2322 — the
/// display cap must not suppress real diagnostics.
#[test]
fn plain_mismatch_still_reports() {
    let source = r#"
const value: { id: number } = "not an object";
"#;
    assert!(
        !check_source_diagnostics(source)
            .iter()
            .filter(|d| d.code == 2322)
            .collect::<Vec<_>>()
            .is_empty(),
        "a genuine mismatch must still report TS2322"
    );
}

/// Smoke check: the recursive shapes above must not crash even when no property
/// name is missing — exercising the normalization on a clean recursive receiver.
#[test]
fn recursive_receivers_do_not_crash() {
    assert_terminates(
        r#"
type Loop<T> = { self: Loop<T>; value: T };
declare const a: Loop<string>;
const b: number = a.self.self.value;
"#,
    );
}
