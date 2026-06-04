//! Readonly element/index-access writes through *deferred* mapped/alias
//! applications.
//!
//! Structural rule: when the target of an element-access assignment resolves
//! (after evaluating a deferred `Application`/`Lazy` type) to a `readonly`
//! array / readonly index signature, `tsc` reports TS2542; when it resolves to
//! an object with a `readonly` named property reached via a literal element
//! key, `tsc` reports TS2540. tsz's element-write check previously only
//! evaluated the deferred application when it resolved to a readonly *tuple*,
//! so a property whose declared type was a deferred mapped/alias application
//! resolving to a readonly *array* (e.g. ts-essentials' `DeepReadonly`) was
//! silently writable.
//!
//! Binder names are varied across cases so the rule is structural and not keyed
//! to any particular alias/property/type-parameter spelling (anti-hardcoding).

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes(source)
        .into_iter()
        // 2318 = "Cannot find global type" noise in minimal-lib harness runs.
        .filter(|&code| code != 2318)
        .collect()
}

// ---------------------------------------------------------------------------
// Positive: readonly index signature (TS2542) through a deferred application.
// ---------------------------------------------------------------------------

/// A property whose declared type is a user homomorphic readonly mapped alias
/// applied to an array stays a deferred `Application`; writing an element must
/// still report TS2542.
#[test]
fn user_readonly_mapped_alias_over_array_property_emits_2542() {
    let source = r#"
type Frozen<Shape> = { readonly [Key in keyof Shape]: Shape[Key] };
type Wrapper = { items: Frozen<number[]> };
declare const w: Wrapper;
w.items[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "readonly mapped alias over array element write must emit TS2542. Got: {codes:?}"
    );
}

/// Same rule with completely different binder names — proves it is structural.
#[test]
fn user_readonly_mapped_alias_over_array_property_emits_2542_renamed() {
    let source = r#"
type Locked<Rec> = { readonly [P in keyof Rec]: Rec[P] };
type Holder = { payload: Locked<string[]> };
declare const h: Holder;
h.payload[2] = "x";
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "renamed readonly mapped alias over array element write must emit TS2542. Got: {codes:?}"
    );
}

/// An inline application directly as the variable annotation (no named alias
/// wrapper) is also a deferred `Application`; the element write must emit TS2542.
#[test]
fn inline_readonly_mapped_application_variable_emits_2542() {
    let source = r#"
type ReadonlyMap<Src> = { readonly [Member in keyof Src]: Src[Member] };
declare const direct: ReadonlyMap<number[]>;
direct[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "inline readonly mapped application element write must emit TS2542. Got: {codes:?}"
    );
}

/// A conditional type whose chosen branch is a homomorphic readonly mapped type
/// over an array — the ts-essentials `DeepReadonly` shape — nested one object
/// hop deep. This is the original witness from the bug report.
#[test]
fn deep_readonly_nested_array_leaf_emits_2542() {
    let source = r#"
type DeepReadonly<T> = T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;
type Model = DeepReadonly<{ tags: number[] }>;
declare const m: Model;
m.tags[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "DeepReadonly nested array leaf element write must emit TS2542. Got: {codes:?}"
    );
}

/// A conditional alias whose true branch is a readonly mapped array, used as a
/// property type (no recursion). Renamed binders.
#[test]
fn conditional_readonly_array_branch_property_emits_2542() {
    let source = r#"
type Harden<Value> = Value extends unknown[]
    ? { readonly [Slot in keyof Value]: Value[Slot] }
    : Value;
type Box = { contents: Harden<string[]> };
declare const b: Box;
b.contents[0] = "y";
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2542),
        "conditional readonly-array branch element write must emit TS2542. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive: readonly named property (TS2540) via literal element key through a
// deferred object application.
// ---------------------------------------------------------------------------

/// A readonly mapped alias over an *object* reached via a string-literal
/// element key must report TS2540 (named property), not TS2542.
#[test]
fn user_readonly_mapped_alias_over_object_element_key_emits_2540() {
    let source = r#"
type Frozen<Shape> = { readonly [Key in keyof Shape]: Shape[Key] };
type Wrapper = { inner: Frozen<{ field: number }> };
declare const w: Wrapper;
w.inner["field"] = 1;
"#;
    let codes = codes(source);
    assert!(
        codes.contains(&2540),
        "readonly mapped object element-key write must emit TS2540. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2542),
        "readonly named-property write must not be classified as an index signature (TS2542). Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: mutable applications must NOT produce a readonly diagnostic.
// ---------------------------------------------------------------------------

/// An identity (non-readonly) mapped alias over an array stays writable.
#[test]
fn mutable_mapped_alias_over_array_property_no_readonly_error() {
    let source = r#"
type Identity<Shape> = { [Key in keyof Shape]: Shape[Key] };
type Wrapper = { items: Identity<number[]> };
declare const w: Wrapper;
w.items[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2542) && !codes.contains(&2540),
        "mutable mapped alias over array element write must not emit a readonly diagnostic. Got: {codes:?}"
    );
}

/// A `-readonly` mapped alias removes the modifier and must stay writable even
/// when applied to an already-`readonly` array.
#[test]
fn minus_readonly_mapped_alias_over_readonly_array_no_error() {
    let source = r#"
type Thaw<Shape> = { -readonly [Key in keyof Shape]: Shape[Key] };
type Wrapper = { items: Thaw<readonly number[]> };
declare const w: Wrapper;
w.items[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2542) && !codes.contains(&2540),
        "-readonly mapped alias element write must not emit a readonly diagnostic. Got: {codes:?}"
    );
}

/// A deep *mutable* mapped/conditional alias must stay writable through nesting.
#[test]
fn deep_mutable_nested_array_leaf_no_error() {
    let source = r#"
type DeepClone<T> = T extends object ? { [K in keyof T]: DeepClone<T[K]> } : T;
type Model = DeepClone<{ tags: number[] }>;
declare const m: Model;
m.tags[0] = 1;
"#;
    let codes = codes(source);
    assert!(
        !codes.contains(&2542) && !codes.contains(&2540),
        "deep mutable nested array leaf write must not emit a readonly diagnostic. Got: {codes:?}"
    );
}
