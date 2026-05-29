//! Regression tests for homomorphic mapped-type modifier preservation through
//! generic type-alias application (issue #9621).
//!
//! Structural rule: a mapped type `{ [P in keyof T]: X }` is *homomorphic*
//! because its constraint is `keyof T`; homomorphic mapped types inherit T's
//! `readonly`/optional property modifiers regardless of the template `X`.
//! Instantiating such an alias (e.g. `AllProps<SQ>` where
//! `AllProps<T> = T & { [P in keyof T]: unknown }`) must recover the
//! homomorphic source object while still leaving the instantiated mapped type
//! as a finite concrete result; preserving an un-collapsed `keyof <source>`
//! caused broad conformance and fourslash regressions.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Reported repro: `readonly` array property through `AllProps<T>` applied to a
/// concrete interface must type-check cleanly.
#[test]
fn readonly_array_property_through_generic_intersection_alias_is_clean() {
    let src = r#"
        interface SQ { readonly items: readonly string[]; }
        type AllProps<T> = T & { [P in keyof T]: unknown };
        function g<T>(obj: AllProps<T>): T { return obj as T; }
        declare const node: SQ;
        const r = g<SQ>({ items: node.items });
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// The kysely `requireAllProps` shape: `-?` (remove-optional) modifier plus a
/// string-literal discriminant and a `readonly` array, used as a call argument
/// inside a function whose return type re-checks the literal.
#[test]
fn kysely_require_all_props_literal_discriminant_is_clean() {
    let src = r#"
        interface SQ {
            readonly kind: "SQ";
            readonly from: string | undefined;
            readonly selections: readonly string[] | undefined;
        }
        type AllProps<T> = T & { [P in keyof T]-?: unknown };
        function requireAllProps<T>(obj: AllProps<T>): T { return obj as T; }
        function transform(node: SQ): SQ {
            return requireAllProps<SQ>({
                kind: "SQ",
                from: node.from,
                selections: node.selections,
            });
        }
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// The rule is structural, not tied to the spelling of the type parameter or
/// the mapped iteration variable.
#[test]
fn renamed_type_parameter_and_iteration_variable_is_clean() {
    let src = r#"
        interface SQ { readonly items: readonly string[]; }
        type AllProps<K> = K & { [Q in keyof K]: unknown };
        function g<K>(obj: AllProps<K>): K { return obj as K; }
        declare const node: SQ;
        const r = g<SQ>({ items: node.items });
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// Optional modifier must be inherited too: an interface with an optional
/// property accepts an object literal that omits it through the alias.
#[test]
fn optional_modifier_inherited_through_alias_is_clean() {
    let src = r#"
        interface SQ { items?: readonly string[]; }
        type AllProps<T> = T & { [P in keyof T]: unknown };
        function g<T>(obj: AllProps<T>): T { return obj as T; }
        const r = g<SQ>({});
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// Nested `readonly` tuple property exercises the array/tuple modifier path.
#[test]
fn readonly_tuple_property_through_alias_is_clean() {
    let src = r#"
        interface SQ { readonly t: readonly [string, number]; }
        type AllProps<T> = T & { [P in keyof T]: unknown };
        function g<T>(obj: AllProps<T>): T { return obj as T; }
        declare const node: SQ;
        const r = g<SQ>({ t: node.t });
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// Identity key remapping (`as K`) remains homomorphic: when the mapped result
/// is intersected with extra members, `readonly` from the source still applies.
#[test]
fn remapped_identity_intersection_preserves_readonly_modifier() {
    let src = r#"
        type Rename<T> = { [K in keyof T as K]: T[K] };
        type Source = { readonly a?: number; b?: string; c: boolean };
        type Wrapped = Rename<Source> & { d: number };
        let x: Wrapped = { c: true, d: 1 };
        x.a = 1;
    "#;
    assert_eq!(
        codes(src),
        vec![2540],
        "expected only TS2540 for writing through remapped readonly property"
    );
}

/// Same structural rule with renamed binders: optionality inherited from the
/// homomorphic source must survive the `as Key` remap and intersection merge.
#[test]
fn remapped_identity_intersection_preserves_optional_modifier() {
    let src = r#"
        type CopyShape<Item> = { [Field in keyof Item as Field]: Item[Field] };
        type Input = { readonly a?: number; b?: string; c: boolean };
        type Output = CopyShape<Input> & { d: number };
        declare const out: Output;
        const mustBeNumber: number = out.a;
        const ok: Output = { c: false, d: 1 };
    "#;
    assert_eq!(
        codes(src),
        vec![2322],
        "expected only TS2322 because remapped optional property reads as number | undefined"
    );
}

/// Regression guard: a mutable array property continued to work before the fix
/// and must keep working (covariant assignability to inherited `unknown`).
#[test]
fn mutable_array_property_through_alias_stays_clean() {
    let src = r#"
        interface SQ { readonly items: string[]; }
        type AllProps<T> = T & { [P in keyof T]: unknown };
        function g<T>(obj: AllProps<T>): T { return obj as T; }
        declare const node: SQ;
        const r = g<SQ>({ items: node.items });
    "#;
    assert!(
        codes(src).is_empty(),
        "expected no diagnostics, got {:?}",
        codes(src)
    );
}

/// Negative guard: excess properties must still be rejected (TS2353) — the fix
/// only restores modifier inheritance, it does not weaken freshness checking.
#[test]
fn excess_property_through_alias_still_reports_ts2353() {
    let src = r#"
        interface SQ { readonly items: readonly string[]; }
        type AllProps<T> = T & { [P in keyof T]: unknown };
        function g<T>(obj: AllProps<T>): T { return obj as T; }
        declare const node: SQ;
        const r = g<SQ>({ items: node.items, extra: 1 });
    "#;
    assert!(
        codes(src).contains(&2353),
        "expected TS2353 for the excess `extra` property, got {:?}",
        codes(src)
    );
}
