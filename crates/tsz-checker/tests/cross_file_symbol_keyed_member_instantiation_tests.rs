//! A computed symbol-keyed member keeps its canonical identity when its
//! declaring type is generic, imported, or both.
//!
//! Structural rule, oracle-pinned against the repo's pinned `tsc` (7.0.2, per
//! `scripts/conformance/typescript-versions.json`): the key a computed member
//! contributes is decided by the key EXPRESSION — syntactic reach to the global
//! `Symbol`, a `typeof Symbol.<member>` alias, or a `unique symbol` binding —
//! and nothing about the declaring type's generic arity or its file of origin
//! changes it. So `interface I[T] { [Symbol.iterator]: () => T }` describes the
//! same member whether `I` is read in its own file or imported, and whether it
//! is used bare or instantiated.
//!
//! This suite is the cross-file and generic-arity floor for the #16307 family
//! (#16319 / #16326 / #16329 / #16331 / #16336 / #16348 closed the single-file,
//! non-generic shapes). Every case here is `exit 0` under the oracle.
//!
//! Binder names vary deliberately across cases (`it`, `so`, `key`, `k`): the
//! rule reads the declaration's annotation and the key expression's shape,
//! never the identifier the user chose.
//!
//! KNOWN GAP, deliberately not asserted here: when the declaring type is BOTH
//! generic AND declared in another file, the heritage/assignability direction
//! compares against the UNINSTANTIATED type (`() => T` where `() => number` was
//! written). That is not symbol-specific — an ordinary named member on the same
//! cross-file generic interface fails identically — so it is tracked separately
//! rather than pinned as a symbol-key expectation.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{check_multi_file_with_libs_stamped, check_source_with_libs};

/// `Symbol.iterator` is declared in `lib.es2015.iterable.d.ts`, NOT in
/// `lib.es2015.symbol.wellknown.d.ts` (which carries `hasInstance`, `match`,
/// `species`, `toStringTag`, ...). Loading only the latter leaves
/// `Symbol.iterator` unresolved and every case below reports an unrelated
/// `TS2339` instead of the assignability answer under test.
fn libs() -> Vec<Arc<LibFile>> {
    tsz_checker::test_utils::load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
    ])
}

fn options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2022,
        ..CheckerOptions::default()
    }
}

/// Full `TS<code> <message>` rows, with each diagnostic's related-information
/// chain appended. A bare code vector cannot tell "the member was not found"
/// from "the member was found and its type was wrong", and this family
/// produces both.
fn single(source: &str) -> Vec<String> {
    check_source_with_libs(source, "test.ts", options(), &libs())
        .into_iter()
        .map(|d| format!("TS{} {}", d.code, d.message_text))
        .collect()
}

fn multi(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    check_multi_file_with_libs_stamped(files, entry, options(), &libs())
        .into_iter()
        .map(|d| {
            let related: Vec<String> = d
                .related_information
                .iter()
                .map(|r| format!(" || TS{} {}", r.code, r.message_text))
                .collect();
            format!(
                "{} TS{} {}{}",
                d.file,
                d.code,
                d.message_text,
                related.join("")
            )
        })
        .collect()
}

// --- single file -----------------------------------------------------------

/// Baseline: a `typeof Symbol.<member>` alias keys the same member as the
/// inline form, with no generics in play.
#[test]
fn alias_key_matches_inline_key_without_generics() {
    assert_eq!(
        single(
            r#"
declare const it: typeof Symbol.iterator;
interface Target { [it]: () => number }
class Impl { [it](): number { return 1 } }
declare function want(v: Target): void;
declare const impl: Impl;
want(impl);
"#,
        ),
        Vec::<String>::new(),
    );
}

/// Generic arity alone does not disturb the key: the member survives
/// instantiation of its declaring interface and class.
#[test]
fn generic_declaring_types_keep_the_alias_key() {
    assert_eq!(
        single(
            r#"
declare const it: typeof Symbol.iterator;
interface Target<T> { [it]: () => T }
class Impl<T> { [it](): T { return null as any } }
declare function want(v: Target<number>): void;
declare const impl: Impl<number>;
want(impl);
"#,
        ),
        Vec::<String>::new(),
    );
}

/// The `implements` type argument reaches the interface through a conditional
/// type alias rather than as a bare type parameter — the shape xstate's
/// `Actor[TLogic]` uses (`SnapshotFrom[TLogic]`, ...).
#[test]
fn conditional_alias_type_argument_keeps_the_key() {
    assert_eq!(
        single(
            r#"
declare const it: typeof Symbol.iterator;
interface Target<T> { [it]: () => T }
type Snap<T> = T extends { s: infer S } ? S : never;
interface Logic { s: number }
class Impl<TL extends Logic> implements Target<Snap<TL>> {
  [it](): Snap<TL> { return null as any }
}
declare function want(v: Target<any>): void;
declare const impl: Impl<Logic>;
want(impl);
"#,
        ),
        Vec::<String>::new(),
    );
}

/// The symbol member is inherited through an interface heritage chain rather
/// than declared on the implemented interface directly.
#[test]
fn inherited_symbol_member_survives_a_heritage_chain() {
    assert_eq!(
        single(
            r#"
declare const it: typeof Symbol.iterator;
interface Target<T> { [it]: () => T }
interface Sub<T> extends Target<T> { subscribe(): void }
type Snap<T> = T extends { s: infer S } ? S : never;
interface Logic { s: number }
class Impl<TL extends Logic> implements Sub<Snap<TL>> {
  [it](): Snap<TL> { return null as any }
  subscribe(): void {}
}
declare function want(v: Sub<any>): void;
declare const impl: Impl<Logic>;
want(impl);
"#,
        ),
        Vec::<String>::new(),
    );
}

/// xstate's own convention, single file: a `declare global` augmentation types
/// `Symbol.observable` as a PLAIN `symbol`, yet `typeof Symbol.observable` is
/// still the well-known member — `tsc`'s discriminator is syntactic reach to
/// the global `Symbol`, never the augmented member's declared kind (#16307).
#[test]
fn wide_augmented_well_known_still_keys_the_named_member() {
    assert_eq!(
        single(
            r#"
declare global { interface SymbolConstructor { readonly observable: symbol } }
export {};
declare const so: typeof Symbol.observable;
interface Interop<T> { [so]: () => T }
interface Sub<T> extends Interop<T> { subscribe(): void }
interface Ref<TS, TE> extends Sub<TS>, Interop<TS> { send(e: TE): void }
type SnapshotFrom<T> = T extends { snap: infer S } ? S : never;
type EventFrom<T> = T extends { evt: infer E } ? E : never;
interface AnyLogic { snap: unknown; evt: unknown }
type AnyRef = Ref<any, any>;
class Actor<TL extends AnyLogic> implements Ref<SnapshotFrom<TL>, EventFrom<TL>> {
  [so](): SnapshotFrom<TL> { return null as any }
  subscribe(): void {}
  send(_e: EventFrom<TL>): void {}
}
declare function want(r: AnyRef): void;
declare const a: Actor<AnyLogic>;
want(a);
"#,
        ),
        Vec::<String>::new(),
    );
}

// --- across files ----------------------------------------------------------

/// Control for the multi-file harness itself: declaring and implementing in one
/// file must stay clean when routed through `check_multi_file_with_libs_stamped`.
#[test]
fn single_file_control_through_the_multi_file_harness() {
    assert_eq!(
        multi(
            &[(
                "actor.ts",
                r#"interface Interop { [Symbol.iterator]: () => number }
export class Actor implements Interop {
  public [Symbol.iterator](): number { return 1 }
}
declare function want(r: Interop): void;
declare const a: Actor;
want(a);
"#,
            )],
            "actor.ts",
        ),
        Vec::<String>::new(),
    );
}

/// An imported interface's inline `[Symbol.iterator]` member is the same member
/// the importing file's class declares inline. The two files bind the key
/// through separate binders, so this pins that the key is not per-binder.
#[test]
fn imported_interface_symbol_member_is_implementable() {
    assert_eq!(
        multi(
            &[
                (
                    "types.ts",
                    "export interface Interop { [Symbol.iterator]: () => number }\n",
                ),
                (
                    "actor.ts",
                    r#"import type { Interop } from './types';
export class Actor implements Interop {
  public [Symbol.iterator](): number { return 1 }
}
declare function want(r: Interop): void;
declare const a: Actor;
want(a);
"#,
                ),
            ],
            "actor.ts",
        ),
        Vec::<String>::new(),
    );
}

/// The same imported member reached as a value, with no class in the graph.
#[test]
fn imported_interface_symbol_member_is_assignable_as_a_value() {
    assert_eq!(
        multi(
            &[
                (
                    "types.ts",
                    "export interface Interop { [Symbol.iterator]: () => number }\n",
                ),
                (
                    "use.ts",
                    r#"import type { Interop } from './types';
declare const i: Interop;
declare function want(r: Interop): void;
want(i);
"#,
                ),
            ],
            "use.ts",
        ),
        Vec::<String>::new(),
    );
}

/// An object literal type written in the importing file satisfies an imported
/// GENERIC interface's symbol-keyed member.
#[test]
fn object_source_satisfies_an_imported_generic_symbol_member() {
    assert_eq!(
        multi(
            &[
                (
                    "types.ts",
                    "export interface Interop<T> { [Symbol.iterator]: () => T }\n",
                ),
                (
                    "use.ts",
                    r#"import { Interop } from './types';
declare const a: { [Symbol.iterator]: () => number };
declare function want(r: Interop<number>): void;
want(a);
"#,
                ),
            ],
            "use.ts",
        ),
        Vec::<String>::new(),
    );
}

/// The key is an imported `unique symbol` const rather than a well-known, and
/// the declaring interface is generic. Both files must agree on the key even
/// though each binder mints its own `SymbolId` for the imported `const`.
#[test]
fn imported_unique_symbol_key_agrees_across_binders() {
    assert_eq!(
        multi(
            &[
                (
                    "types.ts",
                    "export declare const k: unique symbol;\nexport interface Interop<T> { [k]: () => T }\n",
                ),
                (
                    "use.ts",
                    r#"import { k, type Interop } from './types';
declare const a: { [k]: () => number };
declare function want(r: Interop<number>): void;
want(a);
"#,
                ),
            ],
            "use.ts",
        ),
        Vec::<String>::new(),
    );
}

/// Same as above, but the type arrives through a separate `import type`
/// statement while the key const arrives through a value import — the two
/// halves must land on one member identity.
#[test]
fn imported_unique_symbol_key_agrees_under_a_split_type_import() {
    assert_eq!(
        multi(
            &[
                (
                    "types.ts",
                    "export declare const k: unique symbol;\nexport interface Interop<T> { [k]: () => T }\n",
                ),
                (
                    "use.ts",
                    r#"import { k } from './types';
import type { Interop } from './types';
declare const a: { [k]: () => number };
declare function want(r: Interop<number>): void;
want(a);
"#,
                ),
            ],
            "use.ts",
        ),
        Vec::<String>::new(),
    );
}
