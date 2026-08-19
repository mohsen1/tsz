//! Property-drill intermediate relation frame (#17687).
//!
//! When a single (non-union) object target fails a property-type relation whose
//! own relation is itself structural — an optional-widened union
//! (`boolean | undefined` vs `string | undefined`) or an object measured against
//! an index signature (`{ extra: boolean }` vs `{ [k: string]: string }`) — tsc
//! emits the property-pair relation line beneath the `Types of property 'X' are
//! incompatible.` header before drilling into the reduced sub-reason:
//!
//! ```text
//! Type 'S' is not assignable to type 'T'.
//!   Types of property 'm' are incompatible.
//!     Type '<src prop>' is not assignable to type '<tgt prop>'.   <- this frame
//!       <reduced sub-reason drill>
//! ```
//!
//! tsz's property drill previously path-compressed one level too aggressively
//! and rendered the reduced leaf (or index-signature line) directly beneath the
//! property header, dropping that frame. A simple primitive property
//! (`{ p: boolean }` vs `{ p: string }`) has no distinct frame — the pair line
//! *is* the leaf — and must stay single (regression fence below).
//!
//! Every chain is oracle-pinned against `tsc` (typescript 7.0.2 dev / 6.0.2,
//! `--strict --target es2020`). Property, binder, and value-type names vary so a
//! fix keyed to a spelling would not satisfy them.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn chain_of(diags: &[Diagnostic], code: u32) -> Vec<(u8, String)> {
    diags
        .iter()
        .find(|d| d.code == code)
        .map(|d| {
            d.related_information
                .iter()
                .map(|info| (info.depth, info.message_text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Assert the chain of the first TS`code` diagnostic is EXACTLY `expected`
/// (same length, each entry's depth equal and message containing the needle).
fn assert_exact_chain(source: &str, code: u32, expected: &[(u8, &str)]) {
    let diags = check_source_diagnostics(source);
    let chain = chain_of(&diags, code);
    assert_eq!(
        chain.len(),
        expected.len(),
        "chain length for TS{code}: got {chain:?}, expected {expected:?}\nall: {diags:?}"
    );
    for ((got_depth, got_text), (want_depth, want_needle)) in chain.iter().zip(expected) {
        assert!(
            got_depth == want_depth && got_text.contains(want_needle),
            "chain entry mismatch for TS{code}: got (depth {got_depth}, {got_text:?}), \
             expected (depth {want_depth}, contains {want_needle:?})\nfull: {chain:?}"
        );
    }
}

#[test]
fn optional_property_drill_keeps_the_widened_union_frame() {
    assert_exact_chain(
        "declare const src: { m?: boolean; v: string };\nconst z: { m?: string; v: string } = src;\n",
        2322,
        &[
            (0, "Types of property 'm' are incompatible."),
            (
                1,
                "Type 'boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (2, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn optional_property_drill_renamed_binder_and_types() {
    // Anti-hardcoding: property name `flag`, primitives number/string.
    assert_exact_chain(
        "declare const src: { flag?: number };\nconst z: { flag?: string } = src;\n",
        2322,
        &[
            (0, "Types of property 'flag' are incompatible."),
            (
                1,
                "Type 'number | undefined' is not assignable to type 'string | undefined'.",
            ),
            (2, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn optional_property_drill_union_source_widens_into_the_frame() {
    assert_exact_chain(
        "declare const src: { m?: boolean | number };\nconst z: { m?: string } = src;\n",
        2322,
        &[
            (0, "Types of property 'm' are incompatible."),
            (
                1,
                "Type 'number | boolean | undefined' is not assignable to type 'string | undefined'.",
            ),
            (2, "Type 'number' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn index_signature_property_drill_keeps_the_object_vs_index_frame() {
    assert_exact_chain(
        "declare const src: { m: { extra: boolean } };\nconst z: { m: { [k: string]: string } } = src;\n",
        2322,
        &[
            (0, "Types of property 'm' are incompatible."),
            (
                1,
                "Type '{ extra: boolean; }' is not assignable to type '{ [k: string]: string; }'.",
            ),
            (2, "Property 'extra' is incompatible with index signature."),
            (3, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn index_signature_property_drill_renamed_binder_and_types() {
    // Anti-hardcoding: property name `data`, source member `present`, number/boolean.
    assert_exact_chain(
        "declare const src: { data: { present: number } };\nconst z: { data: { [key: string]: boolean } } = src;\n",
        2322,
        &[
            (0, "Types of property 'data' are incompatible."),
            (
                1,
                "Type '{ present: number; }' is not assignable to type '{ [key: string]: boolean; }'.",
            ),
            (
                2,
                "Property 'present' is incompatible with index signature.",
            ),
            (3, "Type 'number' is not assignable to type 'boolean'."),
        ],
    );
}

#[test]
fn alias_to_primitive_property_drill_stays_single_line() {
    // A property type that is an alias of a primitive reduces to that primitive
    // in both the outer display and the leaf, so the pair line coincides with
    // the leaf — no extra frame (tsc shows `{ m: boolean; }` and one drill line).
    assert_exact_chain(
        "type Foo = boolean;\ndeclare const src: { m: Foo };\nconst z: { m: string } = src;\n",
        2322,
        &[
            (0, "Types of property 'm' are incompatible."),
            (1, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}

#[test]
fn simple_primitive_property_drill_stays_single_line() {
    // Regression fence: the pair line coincides with the leaf, so tsc (and tsz)
    // emit exactly one drill line beneath the property header — no extra frame.
    assert_exact_chain(
        "declare const src: { p: boolean };\nconst z: { p: string } = src;\n",
        2322,
        &[
            (0, "Types of property 'p' are incompatible."),
            (1, "Type 'boolean' is not assignable to type 'string'."),
        ],
    );
}
