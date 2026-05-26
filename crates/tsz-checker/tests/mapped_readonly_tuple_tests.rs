//! Tests for the `readonly` mapped-type modifier applied to a tuple source.
//!
//! A homomorphic mapped type that adds `readonly` over a tuple must produce a
//! readonly tuple: element writes error with TS2540 and the result is not
//! assignable to a mutable tuple (TS4104). A `-readonly` mapped type over a
//! readonly tuple must strip readonly. The rule is structural, so it must hold
//! regardless of the iteration-variable name, tuple arity, `readonly` vs
//! `+readonly` spelling, and whether the mapped type is reached through a named
//! alias or an inline application.

use tsz_checker::test_utils::check_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn readonly_mapped_tuple_via_alias_write_emits_2540() {
    let source = r"
type RO<T> = { readonly [K in keyof T]: T[K] };
type R = RO<[number, string]>;
declare const r: R;
r[0] = 5;
";
    assert!(
        codes(source).contains(&2540),
        "writing a readonly mapped tuple element must emit TS2540"
    );
}

#[test]
fn readonly_mapped_tuple_inline_application_write_emits_2540() {
    // Inline application (no named alias for the result) must behave identically.
    let source = r"
type RO<T> = { readonly [K in keyof T]: T[K] };
declare const r: RO<[number, string]>;
r[0] = 5;
";
    assert!(
        codes(source).contains(&2540),
        "inline readonly mapped tuple element write must emit TS2540"
    );
}

#[test]
fn readonly_mapped_tuple_renamed_iter_var_multi_element_write_emits_2540() {
    // Renamed iteration variable + 3 elements: proves the rule is structural,
    // not keyed on the iteration-variable name `K`.
    let source = r#"
type RO<T> = { readonly [P in keyof T]: T[P] };
declare const r: RO<[number, string, boolean]>;
r[1] = "x";
"#;
    assert!(
        codes(source).contains(&2540),
        "renamed-var multi-element readonly mapped tuple write must emit TS2540"
    );
}

#[test]
fn plus_readonly_mapped_tuple_write_emits_2540() {
    let source = r"
type RO<T> = { +readonly [K in keyof T]: T[K] };
declare const r: RO<[number, string]>;
r[0] = 5;
";
    assert!(
        codes(source).contains(&2540),
        "explicit +readonly mapped tuple write must emit TS2540"
    );
}

#[test]
fn readonly_mapped_tuple_not_assignable_to_mutable_tuple_emits_4104() {
    let source = r"
type RO<T> = { readonly [K in keyof T]: T[K] };
declare const r: RO<[number, string]>;
const mut: [number, string] = r;
";
    assert!(
        codes(source).contains(&4104),
        "readonly mapped tuple must not be assignable to a mutable tuple (TS4104)"
    );
}

#[test]
fn minus_readonly_mapped_over_readonly_tuple_strips_readonly() {
    // `-readonly` over a readonly tuple yields a mutable tuple: no TS2540.
    let source = r"
type Mut<T> = { -readonly [K in keyof T]: T[K] };
declare const m: Mut<readonly [number, string]>;
m[0] = 5;
";
    assert!(
        !codes(source).contains(&2540),
        "-readonly mapped tuple element write must NOT emit TS2540"
    );
}

#[test]
fn no_modifier_mapped_over_mutable_tuple_stays_mutable() {
    // Negative control: identity mapped type over a mutable tuple stays mutable.
    let source = r"
type Id<T> = { [K in keyof T]: T[K] };
declare const r: Id<[number, string]>;
r[0] = 5;
";
    assert!(
        !codes(source).contains(&2540),
        "identity mapped tuple (no readonly) write must NOT emit TS2540"
    );
}

#[test]
fn no_modifier_mapped_over_readonly_tuple_preserves_readonly() {
    // Homomorphic preservation: no modifier over a readonly tuple stays readonly.
    let source = r"
type Id<T> = { [K in keyof T]: T[K] };
declare const r: Id<readonly [number, string]>;
r[0] = 5;
";
    assert!(
        codes(source).contains(&2540),
        "identity mapped over a readonly tuple must preserve readonly (TS2540)"
    );
}

#[test]
fn readonly_mapped_over_object_still_emits_2540() {
    // Negative control from the issue: `+readonly` over a plain object source
    // still produces a readonly property (the object path must not regress).
    let source = r"
type RO<T> = { readonly [K in keyof T]: T[K] };
declare const o: RO<{ a: number }>;
o.a = 5;
";
    assert!(
        codes(source).contains(&2540),
        "readonly mapped object property write must still emit TS2540"
    );
}
