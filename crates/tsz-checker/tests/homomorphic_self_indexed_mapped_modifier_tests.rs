//! Homomorphic modifier inheritance for self-indexed mapped types
//! `{ [Q in P]: T[P] }`.
//!
//! Structural rule: tsc derives a mapped type's modifier source from its
//! iteration constraint's type parameter (`getModifiersTypeFromMappedType`
//! follows `P`'s `keyof T` constraint to `T`). When such a mapped type's
//! constraint parameter `P` is substituted by a single property key `k`, the
//! template `T[P]` denotes `T[k]` = `T[Q]` (the sole iterated key), so the
//! result must inherit `T`'s `readonly`/optional modifier for `k`. tsz
//! previously preserved this only when the template indexed by the iteration
//! variable (`T[Q]`); the source-key form (`T[P]`) dropped the `readonly`
//! modifier, which silently broke the ts-essentials `ReadonlyKeys` /
//! `WritableKeys` / `MarkWritable` family (the `IfEquals` identity trick
//! compares `{ [Q in P]: T[P] }` against `{ -readonly [Q in P]: T[P] }`).
//!
//! Binder names are varied across cases so the rule is structural, not keyed to
//! any particular alias/property/type-parameter spelling (anti-hardcoding).

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        // 2318 = "Cannot find global type" noise in minimal-lib harness runs.
        .filter(|&code| code != 2318)
        .collect()
}

// ---------------------------------------------------------------------------
// Positive: `T[P]` over a readonly named property keeps it readonly (TS2540).
// ---------------------------------------------------------------------------

#[test]
fn self_indexed_mapped_preserves_readonly_named_property_emits_2540() {
    let source = r#"
type Obj = { readonly a: number; b: string };
type Probe<T, P extends keyof T> = { [Q in P]: T[P] };
type Picked = Probe<Obj, "a">;
declare const picked: Picked;
picked.a = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2540),
        "self-indexed mapped `T[P]` over readonly property must keep readonly (TS2540). Got: {codes:?}"
    );
}

/// Same rule, completely different binder names — proves it is structural.
#[test]
fn self_indexed_mapped_preserves_readonly_named_property_emits_2540_renamed() {
    let source = r#"
type Record = { readonly handle: number; label: string };
type Lift<Src, Key extends keyof Src> = { [Member in Key]: Src[Key] };
type Held = Lift<Record, "handle">;
declare const held: Held;
held.handle = 42;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2540),
        "renamed self-indexed mapped `Src[Key]` over readonly property must keep readonly (TS2540). Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: a writable source property stays writable (no TS2540).
// ---------------------------------------------------------------------------

#[test]
fn self_indexed_mapped_writable_property_stays_writable_no_2540() {
    let source = r#"
type Obj = { readonly a: number; b: string };
type Probe<T, P extends keyof T> = { [Q in P]: T[P] };
type Picked = Probe<Obj, "b">;
declare const picked: Picked;
picked.b = "x";
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2540),
        "writable source property must not become readonly. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive: `T[P]` over a readonly array property keeps the readonly index
// signature, so an element write reports TS2542.
// ---------------------------------------------------------------------------

#[test]
fn self_indexed_mapped_preserves_readonly_array_emits_2542() {
    let source = r#"
type Obj = { readonly items: readonly number[] };
type Probe<T, P extends keyof T> = { [Q in P]: T[P] };
type Picked = Probe<Obj, "items">;
declare const picked: Picked;
picked.items[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "self-indexed mapped over a readonly array property must keep the readonly index signature (TS2542). Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: an explicit `-readonly` modifier still removes readonly even on the
// self-indexed `T[P]` form.
// ---------------------------------------------------------------------------

#[test]
fn self_indexed_mapped_minus_readonly_removes_readonly_no_2540() {
    let source = r#"
type Obj = { readonly a: number };
type Unlock<T, P extends keyof T> = { -readonly [Q in P]: T[P] };
type Opened = Unlock<Obj, "a">;
declare const opened: Opened;
opened.a = 5;
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2540),
        "`-readonly` must remove readonly even for the self-indexed template. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative (single-key guard): a multi-key constraint must NOT collapse to a
// per-key homomorphic form — `{ [Q in P]: T[P] }` with `P = "a" | "b"` gives
// each property the union `T["a" | "b"]`, so reading a property as the union
// type is accepted (no TS2322).
// ---------------------------------------------------------------------------

#[test]
fn self_indexed_mapped_multi_key_keeps_union_value_no_2322() {
    let source = r#"
type Obj = { a: number; b: string };
type Probe<T, P extends keyof T> = { [Q in P]: T[P] };
type Both = Probe<Obj, "a" | "b">;
declare const both: Both;
const a: number | string = both.a;
const b: number | string = both.b;
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2322),
        "multi-key self-indexed mapped must keep the union value type. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// End-to-end: the ts-essentials `ReadonlyKeys` / `WritableKeys` trick now
// distinguishes readonly from writable keys.
// ---------------------------------------------------------------------------

#[test]
fn readonly_keys_writable_keys_trick_distinguishes_mutability() {
    let source = r#"
type IfEquals<X, Y, A, B> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? A : B;
type ReadonlyKeys<T> = {
  [P in keyof T]-?: IfEquals<{ [Q in P]: T[P] }, { -readonly [Q in P]: T[P] }, never, P>
}[keyof T];
type WritableKeys<T> = {
  [P in keyof T]-?: IfEquals<{ [Q in P]: T[P] }, { -readonly [Q in P]: T[P] }, P, never>
}[keyof T];
type Obj = { readonly a: number; b: string; readonly c: boolean };
const ra: ReadonlyKeys<Obj> = "a";
const rc: ReadonlyKeys<Obj> = "c";
const wb: WritableKeys<Obj> = "b";
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2322),
        "ReadonlyKeys/WritableKeys must accept the correct readonly/writable keys. Got: {codes:?}"
    );
}

/// Negative side of the trick: a writable key is not a `ReadonlyKeys` member and
/// a readonly key is not a `WritableKeys` member — both assignments must error.
#[test]
fn readonly_keys_writable_keys_trick_rejects_wrong_keys() {
    let source = r#"
type IfEquals<X, Y, A, B> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? A : B;
type ReadonlyKeys<T> = {
  [P in keyof T]-?: IfEquals<{ [Q in P]: T[P] }, { -readonly [Q in P]: T[P] }, never, P>
}[keyof T];
type WritableKeys<T> = {
  [P in keyof T]-?: IfEquals<{ [Q in P]: T[P] }, { -readonly [Q in P]: T[P] }, P, never>
}[keyof T];
type Obj = { readonly a: number; b: string; readonly c: boolean };
const wrongReadonly: ReadonlyKeys<Obj> = "b";
const wrongWritable: WritableKeys<Obj> = "a";
"#;
    let codes = codes(source);
    assert!(
        codes.iter().filter(|&&c| c == 2322).count() >= 2,
        "ReadonlyKeys/WritableKeys must reject the wrong-mutability keys (two TS2322). Got: {codes:?}"
    );
}
