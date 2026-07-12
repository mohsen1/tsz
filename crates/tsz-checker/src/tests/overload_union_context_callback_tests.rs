//! Overload selection must not be decided by callbacks typed under a lossy
//! union-of-signatures context, and deferred conditional pairs must infer
//! pairwise.
//!
//! Structural rules:
//! - When an overloaded call has a contextually-typed callback argument and
//!   the union of candidate signatures yields no contextual signature for it
//!   (tsc's `getIntersectedSignatures` returns nothing when generic and
//!   non-generic overloads mix), tsc types the callback per candidate
//!   (`chooseOverload`); tsz defers such first-pass matches to the
//!   signature-specific pass. Owner: checker overload resolution
//!   (`overload_resolution/resolve_signatures.rs` +
//!   `overload_resolution/helpers.rs`).
//! - When both the inference source and target are conditionals stuck on an
//!   abstract check type (HKT-style `C extends K ? Registry<A>[C] : any`),
//!   tsc's `inferToConditionalType` infers pairwise over the corresponding
//!   positions without reducing; tsz mirrors that in the constraint walker.
//!   Owner: solver `operations/constraints/deferred_conditional.rs`.
//!
//! Witness family: fp-ts style HKT encodings (issue #14345, families A/B of
//! the P1 measurement), reduced to lib-free form.

use crate::test_utils::check_source_strict_codes as check_strict;

const REJECTION_CODES: &[u32] = &[2322, 2345, 2739, 2740, 2741, 2769];

fn has_rejection(codes: &[u32]) -> bool {
    codes.iter().any(|code| REJECTION_CODES.contains(code))
}

/// The fp-ts `altAll` shape: a two-overload receiver (concrete `(T, T) => T`
/// plus generic `<U>`) whose callback runs a nested generic call on the
/// accumulator. The union context cannot type the callback (generic +
/// non-generic overloads), so selection must fall back to per-candidate
/// contextual typing instead of committing an inference poisoned by
/// implicit-`any` callback parameters.
#[test]
fn overloaded_callback_nested_generic_call_selects_concrete_overload() {
    let codes = check_strict(
        r#"
interface Box<Tag, Val> {
  readonly _tag: Tag
  readonly _val: Val
}
interface Merge<G> {
  readonly merge: <V>(fst: Box<G, V>, snd: () => Box<G, V>) => Box<G, V>
}
interface Bag<T> {
  fold(step: (prev: T, cur: T) => T, seed: T): T
  fold<U>(step: (prev: U, cur: T) => U, seed: U): U
}
export function mergeAll<G>(
  m: Merge<G>
): <V>(seed: Box<G, V>) => (bag: Bag<Box<G, V>>) => Box<G, V> {
  return (seed) => (bag) => bag.fold((prev, cur) => m.merge(prev, () => cur), seed)
}
"#,
    );
    assert!(!has_rejection(&codes), "{codes:?}");
}

/// Same shape with the callback parameters in the other order and a method
/// (not property) receiver, so the rule is not tied to one member form.
#[test]
fn overloaded_callback_nested_generic_call_renamed_binders() {
    let codes = check_strict(
        r#"
interface Pair<L, R> {
  readonly left: L
  readonly right: R
}
interface Alg<K> {
  combine<E>(a: Pair<K, E>, b: () => Pair<K, E>): Pair<K, E>
}
interface Seq<T> {
  collapse(f: (acc: T, item: T) => T, init: T): T
  collapse<O>(f: (acc: O, item: T) => O, init: O): O
}
export function collapseAll<K>(
  alg: Alg<K>
): <E>(init: Pair<K, E>) => (seq: Seq<Pair<K, E>>) => Pair<K, E> {
  return (init) => (seq) => seq.collapse((acc, item) => alg.combine(acc, () => item), init)
}
"#,
    );
    assert!(!has_rejection(&codes), "{codes:?}");
}

/// The conditional-alias (`Kind`-style) encoding of the same shape: the
/// accumulator and the parameter are both conditionals stuck on the abstract
/// URI parameter, so candidates must register from the deferred pair.
#[test]
fn overloaded_callback_deferred_conditional_alias() {
    let codes = check_strict(
        r#"
interface Registry<A> {
  readonly One: { readonly value: A }
  readonly Many: { readonly values: readonly A[] }
}
type Keys = keyof Registry<any>
type Pick<K extends Keys, A> = K extends Keys ? Registry<A>[K] : any

interface Chooser<K extends Keys> {
  readonly choose: <A>(fst: Pick<K, A>, snd: () => Pick<K, A>) => Pick<K, A>
}
interface Stream<T> {
  fold(step: (prev: T, cur: T) => T, seed: T): T
  fold<U>(step: (prev: U, cur: T) => U, seed: U): U
}
export function chooseAll<K extends Keys>(
  c: Chooser<K>
): <A>(seed: Pick<K, A>) => (s: Stream<Pick<K, A>>) => Pick<K, A> {
  return (seed) => (s) => s.fold((prev, cur) => c.choose(prev, () => cur), seed)
}
"#,
    );
    assert!(!has_rejection(&codes), "{codes:?}");
}

/// Deferred-conditional pairwise inference on a plain (non-overloaded) call:
/// the argument's stuck conditional must seed the parameter's type argument,
/// so the sibling callback parameter is not degraded to `unknown`.
#[test]
fn deferred_conditional_pair_infers_type_argument() {
    let codes = check_strict(
        r#"
interface Registry<A> {
  readonly One: { readonly value: A }
}
type Keys = keyof Registry<any>
type Pick<K extends Keys, A> = K extends Keys ? Registry<A>[K] : any

interface Mapper<K extends Keys> {
  readonly map: <A, B>(fa: Pick<K, A>, f: (a: A) => B) => Pick<K, B>
}
export function lift<K extends Keys>(
  m: Mapper<K>
): <A, B>(f: (a: A) => B) => (fa: Pick<K, A>) => Pick<K, B> {
  return (f) => (fa) => m.map(fa, f)
}
"#,
    );
    assert!(!has_rejection(&codes), "{codes:?}");
}

/// Negative case: the deferral must not swallow genuine mismatches. A callback
/// whose result cannot satisfy the declared outer return type still errors.
#[test]
fn overloaded_callback_wrong_return_still_rejected() {
    let codes = check_strict(
        r#"
interface Box<Tag, Val> {
  readonly _tag: Tag
  readonly _val: Val
}
interface Bag<T> {
  fold(step: (prev: T, cur: T) => T, seed: T): T
  fold<U>(step: (prev: U, cur: T) => U, seed: U): U
}
export function bad<G>(): <V>(seed: Box<G, V>) => (bag: Bag<Box<G, V>>) => Box<G, V> {
  return (seed) => (bag) => bag.fold(() => "oops", seed)
}
"#,
    );
    assert!(
        has_rejection(&codes),
        "wrong callback result must still be rejected: {codes:?}"
    );
}

/// Negative case for the conditional pair: a genuinely mismatched registry is
/// still rejected once the conditionals reduce under concrete keys. (The
/// fully-deferred cross-registry mismatch is a relation-layer gap that
/// predates the pairwise inference arm — the abstract-check-type assignability
/// falls back to the `any` false branch; tracked under the #14345 relation
/// seat.)
#[test]
fn deferred_conditional_pair_mismatched_alias_still_rejected() {
    let codes = check_strict(
        r#"
interface RegistryA<A> {
  readonly One: { readonly value: A }
}
interface RegistryB<A> {
  readonly Uno: { readonly v: A }
}
type KeysA = keyof RegistryA<any>
type KeysB = keyof RegistryB<any>
type PickA<K extends KeysA, A> = K extends KeysA ? RegistryA<A>[K] : any
type PickB<K extends KeysB, A> = K extends KeysB ? RegistryB<A>[K] : any

interface Mapper<K extends KeysB> {
  readonly consume: <A>(fa: PickB<K, A>) => A
}
declare const m: Mapper<"Uno">;
declare const fa: PickA<"One", string>;
export const r: string = m.consume(fa);
"#,
    );
    assert!(
        has_rejection(&codes),
        "mismatched registry must still be rejected: {codes:?}"
    );
}

/// Concrete (fully monomorphic) folds keep working through the same overload
/// set — the deferral only reroutes selection, it must not change results.
#[test]
fn concrete_fold_overload_still_resolves() {
    let codes = check_strict(
        r#"
interface Bag<T> {
  fold(step: (prev: T, cur: T) => T, seed: T): T
  fold<U>(step: (prev: U, cur: T) => U, seed: U): U
}
declare const bag: Bag<number>;
const total: number = bag.fold((prev, cur) => prev + cur, 0);
const text: string = bag.fold((prev, cur) => prev + String(cur), "");
"#,
    );
    assert!(!has_rejection(&codes), "{codes:?}");
}
