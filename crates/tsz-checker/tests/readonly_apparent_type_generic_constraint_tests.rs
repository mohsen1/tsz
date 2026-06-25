//! Readonly classification through a generic type parameter's *apparent type*.
//!
//! `tsc` resolves the deleted/assigned property's symbol against
//! `getApparentType`, so a property reached through a type parameter's
//! constraint (`T extends { readonly a }`, `T extends RA`, `T extends RA & RB`,
//! `T & { readonly a }`) carries the constraint member's `readonly` modifier:
//!
//! - `delete t.a`  -> TS2704 ("operand of 'delete' cannot be read-only")
//! - `t.a = v`     -> TS2540 ("cannot assign to read-only property")
//!
//! Before the apparent-type resolution the type parameter stayed opaque, so the
//! `readonly` modifier was invisible: `delete` fell through to TS2790 ("must be
//! optional") and the assignment was silently accepted.
//!
//! Binder names are varied across cases so the classification cannot be keyed
//! on any identifier (anti-hardcoding gate).

use tsz_checker::test_utils::check_source_strict_codes;

fn count(source: &str, code: u32) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|c| *c == code)
        .count()
}

// ---------------------------------------------------------------------------
// delete -> TS2704 through the apparent type
// ---------------------------------------------------------------------------

#[test]
fn delete_readonly_named_property_via_inline_object_constraint() {
    let source = r#"
function clear<Box extends { readonly slot: number }>(box: Box) {
    delete box.slot;
}
"#;
    assert_eq!(count(source, 2704), 1, "expected TS2704");
    assert_eq!(
        count(source, 2790),
        0,
        "TS2790 must be suppressed by TS2704"
    );
}

#[test]
fn delete_readonly_named_property_via_interface_constraint() {
    let source = r#"
interface Frozen { readonly value: string }
function drop<Cell extends Frozen>(cell: Cell) {
    delete cell.value;
}
"#;
    assert_eq!(
        count(source, 2704),
        1,
        "expected TS2704 for interface constraint"
    );
    assert_eq!(count(source, 2790), 0);
}

#[test]
fn delete_readonly_named_property_via_intersection_constraint() {
    // Only `Locked` declares `key`, and it is readonly: the intersection's
    // synthesized property is readonly even though `Tag` lacks the property.
    let source = r#"
interface Locked { readonly key: number }
interface Tag { label: string }
function unset<Rec extends Locked & Tag>(rec: Rec) {
    delete rec.key;
}
"#;
    assert_eq!(
        count(source, 2704),
        1,
        "expected TS2704 for intersection constraint"
    );
    assert_eq!(count(source, 2790), 0);
}

#[test]
fn delete_readonly_named_property_via_intersection_with_free_type_parameter() {
    // `Base & { readonly id }`: the free type parameter `Base` does not declare
    // `id`, so only the readonly literal member contributes -> readonly.
    let source = r#"
function strip<Base>(value: Base & { readonly id: number }) {
    delete value.id;
}
"#;
    assert_eq!(
        count(source, 2704),
        1,
        "expected TS2704 for intersection with free type parameter"
    );
    assert_eq!(count(source, 2790), 0);
}

#[test]
fn delete_readonly_takes_precedence_over_optional_via_constraint() {
    // `readonly a?` is both readonly and optional; `tsc` reports the readonly
    // error (TS2704), not the must-be-optional one, and never accepts it.
    let source = r#"
function wipe<Holder extends { readonly maybe?: number }>(holder: Holder) {
    delete holder.maybe;
}
"#;
    assert_eq!(count(source, 2704), 1, "readonly wins over optional");
    assert_eq!(count(source, 2790), 0);
}

// ---------------------------------------------------------------------------
// assignment -> TS2540 through the apparent type
// ---------------------------------------------------------------------------

#[test]
fn assign_readonly_named_property_via_inline_object_constraint() {
    let source = r#"
function setIt<Node extends { readonly count: number }>(node: Node) {
    node.count = 5;
}
"#;
    assert_eq!(
        count(source, 2540),
        1,
        "expected TS2540 for write through constraint"
    );
}

#[test]
fn assign_readonly_named_property_via_interface_intersection_constraint() {
    let source = r#"
interface ReadView { readonly total: number }
interface Named { name: string }
function bump<Agg extends ReadView & Named>(agg: Agg) {
    agg.total = 9;
}
"#;
    assert_eq!(
        count(source, 2540),
        1,
        "expected TS2540 for intersection constraint write"
    );
}

// ---------------------------------------------------------------------------
// Negative / non-regression cases
// ---------------------------------------------------------------------------

#[test]
fn delete_mutable_named_property_via_constraint_is_must_be_optional_not_readonly() {
    // A mutable, required property still reports TS2790 (must be optional), and
    // never the readonly error.
    let source = r#"
function remove<Bag extends { open: number }>(bag: Bag) {
    delete bag.open;
}
"#;
    assert_eq!(count(source, 2704), 0, "mutable property is not readonly");
    assert_eq!(
        count(source, 2790),
        1,
        "mutable required property -> TS2790"
    );
}

#[test]
fn assign_mutable_named_property_via_constraint_is_accepted() {
    let source = r#"
function assign<Obj extends { writable: number }>(obj: Obj) {
    obj.writable = 7;
}
"#;
    assert_eq!(
        count(source, 2540),
        0,
        "writable property must not report TS2540"
    );
}

#[test]
fn write_through_readonly_index_signature_constraint_is_not_a_named_readonly() {
    // A property reached only through the constraint's index signature resolves
    // no named symbol on the type parameter, so the write must NOT gain a
    // spurious TS2542 / TS2540 (it is TS2339 in `tsc`).
    let source = r#"
function put<Dict extends { readonly [k: string]: number }>(dict: Dict) {
    dict.entry = 1;
}
"#;
    assert_eq!(
        count(source, 2542),
        0,
        "no spurious readonly-index-signature error"
    );
    assert_eq!(count(source, 2540), 0, "no spurious named-readonly error");
}
