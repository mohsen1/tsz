//! Regression tests for issue #15248.
//!
//! When a class extends a *value* whose type carries construct signatures (a
//! class-like constructor function rather than a class/interface reference),
//! `tsc` computes the base type from
//! `getInstantiatedConstructorsForTypeArguments(baseConstructorType,
//! typeArguments)[0]`: the construct signatures are filtered to those applicable
//! for the extends clause's type-argument count `N`
//! (`N in [minTypeArgumentCount, typeParameters.len()]`) and the base is the
//! *first* survivor's return type. A generic construct signature whose minimum
//! arity exceeds `N` — e.g. `new <K, V>(): Map<K, V>` when `class X extends Map`
//! supplies `N == 0` — is dropped.
//!
//! tsz previously unioned *every* construct signature's return type, producing a
//! spurious `Map<K, V> | Map<any, any>` base whose leaked type parameters
//! misfired the TS2416 override-variance check (the immer `DraftMap extends Map`
//! false positive).
//!
//! The tests vary binder names and signature order so no name literal or
//! declaration order drives the behavior.

use tsz_checker::test_utils::check_source_code_messages;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn codes(source: &str) -> Vec<u32> {
    diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// The core bug: extending a two-construct-signature value with no type
/// arguments takes only the applicable (non-generic) signature's return type,
/// not the union with the under-applied generic signature. The under-applied
/// generic branch would leak `tag: "b"` into the base and reject the assignment.
#[test]
fn zero_type_args_base_picks_non_generic_construct_return_only() {
    let source = r#"
interface A { tag: "a"; m(): void; }
interface B<T> { tag: "b"; val: T; m(): void; }
interface Ctor {
    new (): A;
    new <T>(x: T): B<T>;
}
declare const Ctor: Ctor;
class X extends Ctor {}

// Base is `A`, so `tag` is `"a"`. A union base `A | B<T>` would make `tag`
// `"a" | "b"` and reject this assignment (the #15248 false positive).
declare const x: X;
const t: "a" = x.tag;
"#;
    assert!(
        diagnostics(source).is_empty(),
        "expected no diagnostics (base resolves to `A`), got {:?}",
        diagnostics(source)
    );
}

/// The immer `DraftMap extends Map` shape: a `set(K, V)` override that is valid
/// against `Coll<any, any>` must not be rejected against a spurious
/// `Coll<any, any> | Coll<K, V>` union base.
#[test]
fn map_like_set_override_valid_against_any_base_has_no_ts2416() {
    let source = r#"
interface Coll<K, V> {
    set(key: K, value: V): this;
    get(key: K): V | undefined;
}
interface CollCtor {
    new (): Coll<any, any>;
    new <K, V>(entries?: readonly (readonly [K, V])[]): Coll<K, V>;
    readonly prototype: Coll<any, any>;
}
declare const CollCtor: CollCtor;
class DraftColl extends CollCtor {
    set(key: any, value: any): this {
        return this;
    }
}
"#;
    let ts2416: Vec<_> = diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2416)
        .collect();
    assert!(
        ts2416.is_empty(),
        "expected no TS2416 for a valid `set` override against the `Coll<any, any>` base, got {ts2416:?}"
    );
}

/// Declaration order must not matter: the generic signature declared *first* is
/// still filtered out for a zero-type-argument base, leaving the non-generic
/// signature as the base.
#[test]
fn generic_signature_declared_first_is_still_filtered_for_zero_args() {
    let source = r#"
interface Base0 { kind: "zero"; ping(): void; }
interface Base1<Elem> { kind: "one"; item: Elem; ping(): void; }
interface Factory {
    new <Elem>(seed: Elem): Base1<Elem>;
    new (): Base0;
}
declare const Factory: Factory;
class Gadget extends Factory {}

declare const g: Gadget;
const k: "zero" = g.kind;
"#;
    assert!(
        diagnostics(source).is_empty(),
        "expected no diagnostics (base resolves to `Base0` regardless of signature order), got {:?}",
        diagnostics(source)
    );
}

/// A genuinely incompatible override must still report TS2416 — and against the
/// correct (non-union) base type. Before the fix the base displayed as the
/// `A2 | B2<T>` union with a `number | T` target; now it is the plain `A2`.
#[test]
fn genuinely_incompatible_override_still_reports_ts2416_against_correct_base() {
    let source = r#"
interface A2 { alpha: number; }
interface B2<T> { alpha: T; }
interface Ctor2 {
    new (): A2;
    new <T>(x: T): B2<T>;
}
declare const Ctor2: Ctor2;
class Bad extends Ctor2 {
    alpha!: string;
}
"#;
    let messages: Vec<String> = diagnostics(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2416).then_some(message))
        .collect();
    // The base must display as the plain `A2`, never the spurious `A2 | B2<T>`
    // union that leaked the generic signature's return type before the fix.
    assert!(
        messages
            .iter()
            .any(|message| message.contains("base type 'A2'")),
        "expected TS2416 against the plain `A2` base, got {:?}",
        diagnostics(source)
    );
    assert!(
        !messages.iter().any(|message| message.contains("B2<")),
        "base type must not be the `A2 | B2<T>` union, got {messages:?}"
    );
}

/// When the extends clause *does* supply the type argument, the generic
/// signature is the applicable one and its instantiated return type is the base
/// (`B3<string>`), so the base carries `val: string` and `tag: "b"`.
#[test]
fn supplied_type_argument_selects_generic_signature_base() {
    let source = r#"
interface A3 { tag: "a"; ping(): void; }
interface B3<T> { tag: "b"; val: T; ping(): void; }
interface Ctor3 {
    new (): A3;
    new <T>(x: T): B3<T>;
}
declare const Ctor3: Ctor3;
class Z extends Ctor3<string> {}

declare const z: Z;
const v: string = z.val;
const tg: "b" = z.tag;
"#;
    // `val` must exist (base is `B3<string>`, not `A3`) and both assignments
    // must type-check; no TS2339 (missing `val`) and no TS2322 (`tag` mismatch).
    let relevant: Vec<_> = codes(source)
        .into_iter()
        .filter(|code| matches!(code, 2339 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "expected base `B3<string>` (val: string, tag: \"b\"), got {:?}",
        diagnostics(source)
    );
}
