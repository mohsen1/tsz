//! tsc-parity for index-signature *key* applicability and the display of
//! union-keyed index signatures and string-literal excess-property names.
//!
//! Structural rule (`tsc` `typeRelatedToIndexInfo` / `getApplicableIndexInfo`):
//! a source string-like index satisfies a target index only when the source
//! key *covers* the target key (the target key set is assignable to the source
//! key set). When no source index is applicable, a named interface/class is an
//! error ("Index signature for type X is missing"), while an anonymous
//! object/type-literal is checked structurally and a property-less one passes.
//! This is why two interfaces keyed by unrelated branded strings are mutually
//! unassignable, but the structurally identical anonymous object types are not.
//!
//! Owner: solver `check_string_index_compatibility`
//! (`relations/subtype/rules/objects.rs`). Display owners: the union-keyed index
//! split in `diagnostics/format/compound/object_with_index.rs` and the
//! string-literal excess-property name in
//! `error_reporter/render_failure_property_helpers.rs`.
//!
//! Binder names are varied across cases (no `I1`/`TaggedString1` reuse) so the
//! checks exercise structure, not the conformance fixture's identifiers.
//!
//! Regression coverage for removing the `indexSignatures1` conformance rewrite
//! (#14141).

use tsz_checker::test_utils::check_source_code_messages as check;
use tsz_common::diagnostics::diagnostic_codes;

const TS2322: u32 = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
const TS2353: u32 =
    diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE;
const TS7053: u32 =
    diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN;

const BRANDS: &str = r#"
type BrandA = string & { __a: void };
type BrandB = string & { __b: void };
"#;

fn assert_message(code: u32, needle: &str, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().any(|(c, m)| *c == code && m.contains(needle)),
        "Expected TS{code} containing {needle:?}. Got: {diags:?}"
    );
}

fn assert_no_code(code: u32, source: &str) {
    let diags = check(source);
    assert!(
        diags.iter().all(|(c, _)| *c != code),
        "Expected no TS{code}. Got: {diags:?}"
    );
}

// --- (A) Index-signature key applicability ---------------------------------

#[test]
fn interface_branded_index_keys_are_not_mutually_assignable() {
    // Neither brand covers the other, and interfaces are not inferable-index,
    // so each direction is a hard TS2322.
    assert_message(
        TS2322,
        "Type 'Second' is not assignable to type 'First'.",
        &format!(
            "{BRANDS}
interface First {{ [k: BrandA]: string }}
interface Second {{ [k: BrandB]: string }}
declare let first: First;
declare let second: Second;
first = second;
"
        ),
    );
}

#[test]
fn anonymous_branded_index_objects_are_assignable() {
    // Structurally identical to the interface case, but anonymous type literals
    // are inferable-index: with no named members the missing applicable index is
    // not an error.
    assert_no_code(
        TS2322,
        &format!(
            "{BRANDS}
declare let lhs: {{ [k: BrandA]: string }};
declare let rhs: {{ [k: BrandB]: string }};
lhs = rhs;
"
        ),
    );
}

#[test]
fn union_keyed_source_index_covers_single_branded_target() {
    // `BrandA` is assignable to `BrandA | BrandB`, so a source index keyed by the
    // union *does* cover a target keyed by a single brand: assignable.
    assert_no_code(
        TS2322,
        &format!(
            "{BRANDS}
interface Narrow {{ [k: BrandA]: string }}
interface Wide {{ [k: BrandA | BrandB]: string }}
declare let narrow: Narrow;
declare let wide: Wide;
narrow = wide;
"
        ),
    );
}

#[test]
fn single_branded_source_does_not_cover_union_keyed_target() {
    // The reverse of the above: a target keyed by `BrandA | BrandB` requires the
    // source to cover `BrandB` too, which a `BrandA`-only interface cannot.
    assert_message(
        TS2322,
        "Type 'Narrow' is not assignable to type 'Wide'.",
        &format!(
            "{BRANDS}
interface Narrow {{ [k: BrandA]: string }}
interface Wide {{ [k: BrandA | BrandB]: string }}
declare let narrow: Narrow;
declare let wide: Wide;
wide = narrow;
"
        ),
    );
}

#[test]
fn plain_string_index_assignability_is_unaffected() {
    // Equal keys (the common plain-`string` index) stay trivially applicable —
    // value-type compatibility still governs, so this remains an error.
    assert_message(
        TS2322,
        "Type 'StringToNumber' is not assignable to type 'StringToString'.",
        r#"
interface StringToString { [k: string]: string }
interface StringToNumber { [k: string]: number }
declare let s2s: StringToString;
declare let s2n: StringToNumber;
s2s = s2n;
"#,
    );
}

// --- (B) Union-keyed index signature display split -------------------------

#[test]
fn union_keyed_index_signature_displays_split_clauses() {
    // tsc renders `[k: BrandA | BrandB]: string` as one clause per member.
    assert_message(
        TS7053,
        "{ [k: BrandA]: string; [k: BrandB]: string; }",
        &format!(
            "{BRANDS}
declare let table: {{ [k: BrandA | BrandB]: string }};
declare let loose: string;
table[loose];
"
        ),
    );
}

#[test]
fn union_keyed_index_split_respects_source_order_with_renamed_param() {
    // Member order follows source declaration order, independent of the index
    // parameter name (`entry` here, not `k`).
    assert_message(
        TS7053,
        "{ [entry: BrandA]: string; [entry: BrandB]: string; }",
        &format!(
            "{BRANDS}
declare let table: {{ [entry: BrandA | BrandB]: string }};
declare let loose: string;
table[loose];
"
        ),
    );
}

// --- (C) String-literal excess-property name keeps its quotes --------------

#[test]
fn string_literal_excess_property_name_keeps_quotes() {
    assert_message(
        TS2353,
        "''myKey''",
        r#"
type Pat = `x:${string}`;
type Decl = { [key in Pat]: string };
const decl: Decl = { 'myKey': 'value' };
"#,
    );
}

#[test]
fn identifier_excess_property_name_has_no_inner_quotes() {
    // Control: an identifier-named excess property is rendered bare.
    let diags = check(
        r#"
type Pat = `x:${string}`;
type Decl = { [key in Pat]: string };
const decl: Decl = { plainKey: 'value' };
"#,
    );
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == TS2353 && m.contains("'plainKey'") && !m.contains("''plainKey''")),
        "Expected bare 'plainKey'. Got: {diags:?}"
    );
}
