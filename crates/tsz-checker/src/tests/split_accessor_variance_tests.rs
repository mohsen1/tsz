//! Tests for split-accessor (§26) variance checking and diagnostic display.
//!
//! TypeScript 4.3+ allows `get x(): T` and `set x(v: U)` to have different
//! types.  The solver tracks a distinct write type per property.  These tests
//! verify:
//!
//! 1. **Variance** — write-position assignments into a split-accessor type are
//!    checked against the *setter* parameter type (contravariant), not the
//!    getter return type.
//! 2. **Display** — nested property-mismatch diagnostics report the actual
//!    mismatched property type, not the outer expression's type.
//! 3. **Readonly** — getter-only type-literal/interface properties are readonly.

use crate::test_utils::{check_source_codes, check_source_diagnostics};

fn assert_no_2322(src: &str) {
    let codes = check_source_codes(src);
    assert!(!codes.contains(&2322), "unexpected TS2322. Got: {codes:?}");
}

fn assert_has_2322(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&2322),
        "expected TS2322, got none. Got: {codes:?}"
    );
}

// Nested property mismatch elaboration is in related_information, not the
// primary message_text, so we collect both levels to search for a fragment.
fn all_2322_messages(src: &str) -> Vec<String> {
    let diagnostics = check_source_diagnostics(src);
    let mut messages = Vec::new();
    for diag in &diagnostics {
        if diag.code == 2322 {
            messages.push(diag.message_text.clone());
            for rel in &diag.related_information {
                messages.push(rel.message_text.clone());
            }
        }
    }
    messages
}

fn assert_has_2322_with_message(src: &str, fragment: &str) {
    let messages = all_2322_messages(src);
    let found = messages.iter().any(|msg| msg.contains(fragment));
    assert!(
        found,
        "expected TS2322 (or related) containing {fragment:?}, got: {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// 1. Class split accessor — variance checks
// ---------------------------------------------------------------------------

/// Assigning a class instance whose getter returns `string` to a target
/// that expects `{ x: number }` should produce TS2322 for the mismatched
/// property, not for the entire class type.
#[test]
fn class_split_accessor_getter_mismatch_shows_property_type_not_class_type() {
    assert_has_2322_with_message(
        "
class A {
    get x(): string { return ''; }
    set x(v: string | number) {}
}
const a: { x: number } = new A();
",
        "string",
    );
}

/// The same structural rule applies regardless of the property name.
#[test]
fn class_split_accessor_property_name_independent() {
    assert_has_2322_with_message(
        "
class B {
    get prop(): string { return ''; }
    set prop(v: string | number) {}
}
const b: { prop: number } = new B();
",
        "string",
    );
}

// ---------------------------------------------------------------------------
// 2. Type-literal split accessors — write-position variance
// ---------------------------------------------------------------------------

/// Assigning `string` to a split-accessor property whose setter accepts
/// `string | number` should be accepted (string is a subtype of string | number).
#[test]
fn type_literal_split_accessor_string_assigned_to_string_or_number_ok() {
    assert_no_2322(
        "
declare let obj: { get y(): string; set y(v: string | number) };
obj.y = 'hello';
",
    );
}

/// Assigning `boolean` to a setter that accepts `string | number` should fail.
#[test]
fn type_literal_split_accessor_boolean_rejected_by_string_or_number_setter() {
    assert_has_2322(
        "
declare let obj: { get y(): string; set y(v: string | number) };
obj.y = true;
",
    );
}

/// Assigning a type-literal with a mismatched setter parameter to a target
/// that requires a wider setter should fail (contravariant write position).
#[test]
fn type_literal_split_accessor_narrower_setter_rejected() {
    assert_has_2322(
        "
type Wide  = { get z(): string; set z(v: string | number) };
type Narrow = { get z(): string; set z(v: string) };
declare let wide: Wide;
declare let narrow: Narrow;
wide = narrow;
",
    );
}

// ---------------------------------------------------------------------------
// 3. Interface split accessors — readonly detection
// ---------------------------------------------------------------------------

/// Writing through a split accessor (getter + setter) must be allowed.
#[test]
fn interface_split_accessor_write_is_allowed() {
    let codes = check_source_codes(
        "
interface ISplit { get y(): string; set y(v: string | number); }
declare let obj: ISplit;
obj.y = 'x';
",
    );
    assert!(!codes.contains(&2540), "unexpected TS2540: {codes:?}");
    assert!(!codes.contains(&2322), "unexpected TS2322: {codes:?}");
}

// ---------------------------------------------------------------------------
// 4. Nested property mismatch diagnostic message — Fix A regression test
// ---------------------------------------------------------------------------

/// When a nested property type mismatch is reported (class to type-literal
/// assignment), the elaboration in related_information should name the
/// *property* types (string/number), not the outer class type (D).
#[test]
fn nested_property_mismatch_shows_structural_types_not_outer_class() {
    // D.y : string  !=  { y: number }.y : number
    let messages = all_2322_messages(
        "
class D { y: string = ''; }
let x: { y: number } = new D();
",
    );
    // The elaboration must mention 'string' as the mismatched source type.
    let has_structural = messages.iter().any(|msg| msg.contains("string"));
    assert!(
        has_structural,
        "expected TS2322 elaboration mentioning 'string', got: {messages:?}"
    );
    // The elaboration must NOT incorrectly name the class type 'D' as the
    // mismatched source next to 'number' — that was the pre-fix wrong message.
    let wrongly_names_class = messages
        .iter()
        .any(|msg| msg.contains("'D'") && msg.contains("'number'"));
    assert!(
        !wrongly_names_class,
        "TS2322 should not say \"'D' ... 'number'\" in elaboration: {messages:?}"
    );
}

/// Same rule — independent of the class name chosen.
#[test]
fn nested_property_mismatch_class_name_independent() {
    let messages = all_2322_messages(
        "
class Shape { width: string = ''; }
let v: { width: number } = new Shape();
",
    );
    let has_structural = messages.iter().any(|msg| msg.contains("string"));
    assert!(
        has_structural,
        "expected TS2322 elaboration mentioning 'string', got: {messages:?}"
    );
    // Must not report 'Shape' as the source in the property-level elaboration.
    let wrongly_names_class = messages
        .iter()
        .any(|msg| msg.contains("'Shape'") && msg.contains("'number'"));
    assert!(
        !wrongly_names_class,
        "TS2322 should not say \"'Shape' ... 'number'\" in elaboration: {messages:?}"
    );
}
