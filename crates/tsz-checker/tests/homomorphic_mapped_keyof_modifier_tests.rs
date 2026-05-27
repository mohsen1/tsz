//! Regression tests for homomorphic mapped-type modifier preservation through
//! generic type-alias application (issue #9621).
//!
//! Structural rule: a mapped type `{ [P in keyof T]: X }` is *homomorphic*
//! because its constraint is `keyof T`; homomorphic mapped types inherit T's
//! `readonly`/optional property modifiers regardless of the template `X`.
//! Instantiating such an alias (e.g. `AllProps<SQ>` where
//! `AllProps<T> = T & { [P in keyof T]: unknown }`) must keep the `keyof T`
//! constraint as `keyof <instantiated source>` rather than collapsing it to a
//! concrete key set, otherwise the evaluator can no longer recover the source
//! object and silently drops `readonly`/optional — which made a `readonly`
//! array (`ReadonlyArray`) property fail to satisfy the inherited-`readonly`
//! target and produced spurious TS2345/TS2322 (and an impossible
//! `'"X"' not assignable to '"X"'` elaboration).

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Reported repro: `readonly` array property through `AllProps<T>` applied to a
/// concrete interface must type-check cleanly.
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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

/// Regression guard: a mutable array property continued to work before the fix
/// and must keep working (covariant assignability to inherited `unknown`).
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
#[ignore = "Reproduction for #9621. The instantiation-time keyof-preservation fix was \
reverted from PR #10491 because it regressed 55 conformance tests + caused 317 fourslash \
timeouts (broad type-display/perf blast radius). A correct fix must inherit homomorphic \
modifiers without keeping `keyof <source>` un-collapsed in the constraint; un-ignore once \
that lands."]
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
