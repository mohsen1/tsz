//! Regression coverage for issue #12455: formatting a diagnostic whose receiver
//! type is a self-referential generic graph must terminate, never overflow the
//! native stack.
//!
//! The property-receiver display normalizer
//! (`normalize_property_receiver_application_display_{type,alias,arg}`) walks a
//! type's structure to widen fresh literals and resolve `Lazy` aliases before
//! the receiver is printed in diagnostics such as TS2339. On self-referential
//! graphs it had no depth or cycle guard, so on a sufficiently deep expansion —
//! produced in the wild by zustand's `StoreMutators` middleware-mutator chain
//! and jotai's `Atom`/`Getter`/`Readonly<BuildingBlocks>` cycle — each `Lazy`
//! resolution re-interned a fresh `Application` and the walk grew without bound,
//! overflowing the worker thread's stack while *only formatting an error* (the
//! type checking itself had already terminated). The definitive end-to-end
//! reproduction is the full `pmndrs/zustand` project (`src/` + `tests/`), which
//! aborted with `thread 'main' has overflowed its stack` before the fix and
//! completes after it.
//!
//! These tests exercise the same normalizer on self-referential receivers and
//! assert it both terminates and still reports the expected property-access
//! diagnostic. The sources are kept free of `lib.d.ts` types so they run under
//! the minimal checker harness. Binder names are varied across cases so the
//! guard cannot be keyed to any particular identifier.

use tsz_checker::test_utils::check_source_codes;

/// jotai's mutually-referential atom signatures (`Atom` -> `Read` -> `Getter`
/// -> `Atom`, and the writable variant). Accessing a missing member forces a
/// TS2339 whose receiver walks that recursive graph during display formatting.
#[test]
fn jotai_atom_cycle_property_access_terminates() {
    let codes = check_source_codes(
        r#"
type Getter = <Value>(atom: Atom<Value>) => Value;
type Setter = <Value, Args extends unknown[], Result>(
  atom: WritableAtom<Value, Args, Result>,
  ...args: Args
) => Result;
type Read<Value, SetSelf = never> = (get: Getter, options: { readonly setSelf: SetSelf }) => Value;
type Write<Args extends unknown[], Result> = (get: Getter, set: Setter, ...args: Args) => Result;
interface Atom<Value> { read: Read<Value>; }
interface WritableAtom<Value, Args extends unknown[], Result> extends Atom<Value> {
  write: Write<Args, Result>;
}
interface AtomState<Value = unknown> { dependency: AtomState; current: Value; }
interface BuildingBlocks {
  readState: <Value>(atom: Atom<Value>) => AtomState<Value>;
  writeState: <Value, Args extends unknown[], Result>(
    atom: WritableAtom<Value, Args, Result>,
    ...args: Args
  ) => Result;
  rootAtom: Atom<unknown>;
}
declare const store: BuildingBlocks;
const bad = store.nonExistentMember;
"#,
    );

    assert!(
        codes.contains(&2339),
        "expected TS2339 for the missing member on the recursive receiver, got: {codes:?}"
    );
}

/// The same recursive cycle with every binder renamed. A guard keyed on
/// identifiers (rather than on structural recursion) would not catch this.
#[test]
fn renamed_recursive_cycle_property_access_terminates() {
    let codes = check_source_codes(
        r#"
type Fetch = <Payload>(node: Node<Payload>) => Payload;
type Commit = <Payload, Params extends unknown[], Out>(
  node: MutableNode<Payload, Params, Out>,
  ...params: Params
) => Out;
type Pull<Payload, Echo = never> = (fetch: Fetch, opts: { readonly echo: Echo }) => Payload;
type Push<Params extends unknown[], Out> = (fetch: Fetch, commit: Commit, ...params: Params) => Out;
interface Node<Payload> { pull: Pull<Payload>; }
interface MutableNode<Payload, Params extends unknown[], Out> extends Node<Payload> {
  push: Push<Params, Out>;
}
interface NodeState<Payload = unknown> { parent: NodeState; current: Payload; }
interface Registry {
  pullState: <Payload>(node: Node<Payload>) => NodeState<Payload>;
  pushState: <Payload, Params extends unknown[], Out>(
    node: MutableNode<Payload, Params, Out>,
    ...params: Params
  ) => Out;
  rootNode: Node<unknown>;
}
declare const registry: Registry;
const oops = registry.totallyMissing;
"#,
    );

    assert!(
        codes.contains(&2339),
        "expected TS2339 for the missing member on the renamed recursive receiver, got: {codes:?}"
    );
}

/// zustand's middleware-mutator chain: a registry interface (`StoreMutators`)
/// indexed by a conditional `Mutate` type and augmented with a self-referential
/// entry, then a missing-member access on a value of the mutated store type.
#[test]
fn store_mutator_chain_property_access_terminates() {
    let codes = check_source_codes(
        r#"
interface StoreMutators<S, A> {}
type MutatorId = keyof StoreMutators<unknown, unknown>;
type Lookup<T, K> = K extends keyof T ? T[K] : never;
type Mutate<S, Ms> = Ms extends []
  ? S
  : Ms extends [[infer Mi, infer Ma], ...infer Mrs]
  ? Mutate<Lookup<StoreMutators<S, Ma>, Mi & MutatorId>, Mrs>
  : never;
interface StoreApi<T> { getState: () => T; setState: (partial: T) => void; }
type WithSelf<S> = S extends { getState: () => infer T }
  ? StoreApi<T> & { marker: WithSelf<S> }
  : never;
interface StoreMutators<S, A> {
  self: WithSelf<S>;
}
type Mutated = Mutate<StoreApi<{ count: number }>, [["self", never]]>;
declare const store: Mutated;
const bad = store.missingMember;
"#,
    );

    // Reaching this assertion at all means formatting the diagnostic for this
    // self-referential graph terminated rather than overflowing the stack.
    // `store.missingMember` always produces a property-access diagnostic.
    assert!(
        codes.contains(&2339),
        "expected TS2339 for the missing member on the store-mutator receiver, got: {codes:?}"
    );
}
