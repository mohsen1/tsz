//! Tests for TS2540 readonly element-access writes through a *type-level* index.
//!
//! Structural rule: when `obj[key]` is an assignment target and `key`'s type
//! resolves to one or more concrete property keys (`keyof T`, a `"a" | "b"`
//! literal-key union, or a single literal-key-typed variable), `tsc` reports
//! one TS2540 per readonly key it targets and suppresses the TS2322
//! type-mismatch — exactly as for a directly-written readonly property write.
//! A bare generic `keyof T` (T a free type parameter) stays deferred and is not
//! flagged. Differential ground truth: `tsc` 6.0.2.
//!
//! These exercise the gap where a literal index expression (`o["a"]`) was
//! detected but a *type-level* index was not: `keyof T` was classified as
//! "other" so no key was resolved, and a union index reported only its first
//! readonly key instead of one per key.

use tsz_checker::test_utils::check_source_code_messages;

/// `(code, message)` pairs with the empty-lib `TS2318` ("Cannot find global
/// type") noise filtered out, matching the sibling readonly suites.
fn diags(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn count_code(source: &str, code: u32) -> usize {
    diags(source).iter().filter(|(c, _)| *c == code).count()
}

fn has_code(source: &str, code: u32) -> bool {
    count_code(source, code) >= 1
}

/// True when a TS2540 naming `prop` (rendered as `'prop'` in the message) is
/// present.
fn has_readonly_for(source: &str, prop: &str) -> bool {
    let needle = format!("'{prop}'");
    diags(source)
        .iter()
        .any(|(c, m)| *c == 2540 && m.contains(&needle))
}

// ---------------------------------------------------------------------------
// keyof index
// ---------------------------------------------------------------------------

#[test]
fn keyof_single_readonly_property_emits_2540() {
    let source = r"
type Rec = { readonly field: number };
declare const obj: Rec;
declare const key: keyof Rec;
obj[key] = 99;
";
    assert!(
        has_readonly_for(source, "field"),
        "keyof index onto a readonly property must emit TS2540, got: {:?}",
        diags(source)
    );
    assert!(
        !has_code(source, 2322),
        "a readonly named-property write must suppress TS2322, got: {:?}",
        diags(source)
    );
}

#[test]
fn keyof_all_readonly_emits_one_2540_per_key() {
    let source = r"
type Pair = { readonly left: number; readonly right: number };
declare const pair: Pair;
declare const key: keyof Pair;
pair[key] = 1;
";
    assert_eq!(
        count_code(source, 2540),
        2,
        "each readonly key in the keyof union must emit its own TS2540, got: {:?}",
        diags(source)
    );
    assert!(has_readonly_for(source, "left"));
    assert!(has_readonly_for(source, "right"));
}

#[test]
fn keyof_mixed_readonly_and_mutable_emits_only_readonly_keys() {
    let source = r"
type Mixed = { readonly locked: number; open: number };
declare const value: Mixed;
declare const key: keyof Mixed;
value[key] = 7;
";
    assert_eq!(
        count_code(source, 2540),
        1,
        "only the readonly key must emit TS2540, got: {:?}",
        diags(source)
    );
    assert!(has_readonly_for(source, "locked"));
    assert!(
        !has_readonly_for(source, "open"),
        "the mutable key must not emit TS2540"
    );
    assert!(!has_code(source, 2322));
}

#[test]
fn keyof_readonly_value_mismatch_suppresses_2322() {
    // A real value mismatch is still suppressed by the readonly named-property
    // write (tsc emits only TS2540).
    let source = r"
type Rec = { readonly amount: number };
declare const obj: Rec;
declare const key: keyof Rec;
obj[key] = 'not a number';
";
    assert!(has_readonly_for(source, "amount"));
    assert!(
        !has_code(source, 2322),
        "readonly named-property write suppresses the value-mismatch TS2322, got: {:?}",
        diags(source)
    );
}

// ---------------------------------------------------------------------------
// explicit literal-key union / single literal-key variable
// ---------------------------------------------------------------------------

#[test]
fn literal_key_union_emits_one_2540_per_readonly_key() {
    let source = r"
type Rec = { readonly alpha: number; readonly beta: number };
declare const rec: Rec;
declare const key: 'alpha' | 'beta';
rec[key] = 0;
";
    assert_eq!(
        count_code(source, 2540),
        2,
        "a written-out literal-key union must report each readonly key, got: {:?}",
        diags(source)
    );
    assert!(has_readonly_for(source, "alpha"));
    assert!(has_readonly_for(source, "beta"));
}

#[test]
fn single_literal_key_typed_variable_emits_2540() {
    let source = r"
type Rec = { readonly only: number };
declare const rec: Rec;
declare const key: 'only';
rec[key] = 5;
";
    assert!(has_readonly_for(source, "only"));
}

// ---------------------------------------------------------------------------
// `as const` receiver via `keyof typeof`
// ---------------------------------------------------------------------------

#[test]
fn const_asserted_object_keyof_typeof_index_emits_2540() {
    let source = r"
const frozen = { count: 1 } as const;
let key: keyof typeof frozen = 'count';
frozen[key] = 2;
";
    assert!(
        has_readonly_for(source, "count"),
        "writing a `keyof typeof` index of an `as const` object must emit TS2540, got: {:?}",
        diags(source)
    );
}

// ---------------------------------------------------------------------------
// compound assignment / increment route through the same check
// ---------------------------------------------------------------------------

#[test]
fn keyof_readonly_compound_assignment_emits_2540() {
    let source = r"
type Pair = { readonly first: number; readonly second: number };
declare const pair: Pair;
declare const key: keyof Pair;
pair[key] += 1;
";
    assert_eq!(
        count_code(source, 2540),
        2,
        "compound assignment to a keyof-indexed readonly target must emit TS2540 per key, got: {:?}",
        diags(source)
    );
}

#[test]
fn keyof_readonly_increment_emits_2540() {
    let source = r"
type Rec = { readonly total: number };
declare const obj: Rec;
declare const key: keyof Rec;
obj[key]++;
";
    assert!(has_readonly_for(source, "total"));
}

// ---------------------------------------------------------------------------
// controls
// ---------------------------------------------------------------------------

#[test]
fn keyof_mutable_object_does_not_emit_2540() {
    let source = r"
type Rec = { writable: number };
declare const obj: Rec;
declare const key: keyof Rec;
obj[key] = 3;
";
    assert!(
        !has_code(source, 2540),
        "a fully-mutable receiver must not emit any readonly error, got: {:?}",
        diags(source)
    );
}

#[test]
fn generic_keyof_of_bare_type_parameter_is_not_flagged() {
    // `keyof T` for a free type parameter stays deferred; tsc does not flag the
    // write (the cast launders the value type), so neither should tsz.
    let source = r"
function write<T extends { readonly a: number }>(obj: T, key: keyof T): void {
    obj[key] = 1 as T[keyof T];
}
";
    assert!(
        !has_code(source, 2540),
        "a bare generic keyof index must not be flagged readonly, got: {:?}",
        diags(source)
    );
}

#[test]
fn renamed_binders_emit_identical_per_key_2540() {
    // Anti-hardcoding control: a structurally identical program with different
    // identifiers must behave the same (two TS2540, one per readonly key).
    let source = r"
type Bundle = { readonly xÿz: number; readonly qrs: number };
declare const bundle: Bundle;
declare const sel: keyof Bundle;
bundle[sel] = 4;
";
    assert_eq!(
        count_code(source, 2540),
        2,
        "renamed binders must still report each readonly key, got: {:?}",
        diags(source)
    );
    assert!(has_readonly_for(source, "xÿz"));
    assert!(has_readonly_for(source, "qrs"));
}
