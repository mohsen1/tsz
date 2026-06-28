//! Tests for TS2352 `void`/`undefined` assertion comparability.
//!
//! Structural rule: `void` and `undefined` overlap in tsc's comparable
//! relation. `tsc`'s `isSimpleTypeRelatedTo` relates an `undefined` source to a
//! `void` target, and `checkAssertionWorker` runs comparability in both
//! directions, so an assertion between `void` and `undefined` — at the top
//! level or nested in a shared property / element / contravariant callback
//! parameter — is accepted. Previously tsz's assertion descent treated
//! `undefined` (source) vs `void` (target) as non-overlapping, producing a
//! false-positive TS2352 on hand-rolled thenable -> `Promise`/`PromiseLike`
//! casts (mined from zustand `persist.ts`).
//!
//! Owner: solver `types_are_comparable_for_assertion_inner`
//! (`crates/tsz-solver/src/type_queries/flow/comparability.rs`).

use crate::test_utils::check_source_strict_codes as check_strict;

fn ts2352(source: &str) -> Vec<u32> {
    check_strict(source)
        .into_iter()
        .filter(|c| *c == 2352)
        .collect()
}

// ---------------------------------------------------------------------------
// Reported repro: nested contravariant callback param `undefined` vs `void`
// ---------------------------------------------------------------------------

/// The minimal zustand repro: a hand-rolled thenable whose `then` callback
/// takes `undefined` asserted to one whose optional `then` callback takes
/// `void` must NOT emit TS2352.
#[test]
fn thenable_undefined_callback_to_optional_void_callback_no_ts2352() {
    let source = r#"
type Src = { then(cb: (v: undefined) => unknown): unknown };
type Tgt = { then(onfulfilled?: (value: void) => unknown): unknown };
declare const s: Src;
const a = s as Tgt;
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — `undefined` and `void` overlap in the contravariant \
         callback-parameter position. Got: {:?}",
        check_strict(source)
    );
}

/// Binder-name invariance: the same shape with renamed aliases / parameters
/// must behave identically (no name-keyed logic).
#[test]
fn thenable_void_undefined_alias_name_invariant_no_ts2352() {
    for (src_alias, tgt_alias, p1, p2) in [
        ("Source", "Target", "value", "result"),
        ("Wrapped", "Outer", "x", "y"),
        ("ThenableA", "ThenableB", "input", "out"),
    ] {
        let source = format!(
            r#"
type {src_alias} = {{ then({p1}: (v: undefined) => unknown): unknown }};
type {tgt_alias} = {{ then({p2}?: (value: void) => unknown): unknown }};
declare const s: {src_alias};
const a = s as {tgt_alias};
"#
        );
        assert!(
            ts2352(&source).is_empty(),
            "[{src_alias} -> {tgt_alias}] no TS2352 expected (binder-name invariant). \
             Got: {:?}",
            check_strict(&source)
        );
    }
}

/// Generic thenable wrapper: the same shape parameterized over the resolved
/// value type still resolves the inner `undefined`/`void` overlap.
#[test]
fn generic_thenable_void_undefined_no_ts2352() {
    let source = r#"
type Src<V> = { then(cb: (v: V) => unknown): unknown };
type Tgt = { then(onfulfilled?: (value: void) => unknown): unknown };
declare const s: Src<undefined>;
const a = s as Tgt;
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — generic thenable resolves V=undefined vs void. Got: {:?}",
        check_strict(source)
    );
}

// ---------------------------------------------------------------------------
// Top-level and reverse-direction overlap
// ---------------------------------------------------------------------------

/// Top-level `void as undefined` and `undefined as void` are both accepted by
/// tsc (comparability is bidirectional at the assertion site).
#[test]
fn top_level_void_undefined_both_directions_no_ts2352() {
    let forward = r#"
declare const v: void;
const a = v as undefined;
"#;
    let reverse = r#"
declare const u: undefined;
const a = u as void;
"#;
    assert!(
        ts2352(forward).is_empty(),
        "no TS2352 expected — `void as undefined`. Got: {:?}",
        check_strict(forward)
    );
    assert!(
        ts2352(reverse).is_empty(),
        "no TS2352 expected — `undefined as void`. Got: {:?}",
        check_strict(reverse)
    );
}

/// Optional method params generally: `{ m?: (x: undefined) => void }` vs
/// `{ m?: (x: void) => void }` overlap.
#[test]
fn optional_method_param_void_undefined_no_ts2352() {
    let source = r#"
type A = { m?: (x: undefined) => void };
type B = { m?: (x: void) => void };
declare const a: A;
const b = a as B;
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — optional-method callback `undefined`/`void` overlap. Got: {:?}",
        check_strict(source)
    );
}

/// Shared-property element nesting: `undefined`/`void` overlap inside a shared
/// non-callback property too.
#[test]
fn shared_property_void_undefined_no_ts2352() {
    let source = r#"
type A = { tag: undefined };
type B = { tag: void };
declare const a: A;
const b = a as B;
"#;
    assert!(
        ts2352(source).is_empty(),
        "no TS2352 expected — shared property `undefined`/`void` overlap. Got: {:?}",
        check_strict(source)
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the rule must stay scoped to void/undefined
// ---------------------------------------------------------------------------

/// `undefined as string` must STILL emit TS2352 — the void/undefined rule must
/// not widen to unrelated primitives.
#[test]
fn undefined_to_string_still_emits_ts2352() {
    let source = r#"
declare const u: undefined;
const a = u as string;
"#;
    assert!(
        !ts2352(source).is_empty(),
        "TS2352 expected — `undefined` does not overlap `string`. Got: {:?}",
        check_strict(source)
    );
}

/// `void as number` must STILL emit TS2352.
#[test]
fn void_to_number_still_emits_ts2352() {
    let source = r#"
declare const v: void;
const a = v as number;
"#;
    assert!(
        !ts2352(source).is_empty(),
        "TS2352 expected — `void` does not overlap `number`. Got: {:?}",
        check_strict(source)
    );
}

/// A nested callback param that is genuinely incomparable (`string` vs `number`)
/// must STILL emit TS2352 — the descent still rejects non-overlapping inner
/// params; only `undefined`/`void` were made comparable.
#[test]
fn thenable_incomparable_inner_param_still_emits_ts2352() {
    let source = r#"
type Src = { then(cb: (v: string) => unknown): unknown; tag: "src" };
type Tgt = { then(onfulfilled?: (value: number) => unknown): unknown; tag: "tgt" };
declare const s: Src;
const a = s as Tgt;
"#;
    assert!(
        !ts2352(source).is_empty(),
        "TS2352 expected — `string` and `number` callback params do not overlap. Got: {:?}",
        check_strict(source)
    );
}
