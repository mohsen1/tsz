//! Property type of a set-only accessor in an object literal expression.
//!
//! Structural rule (matches tsc `getTypeOfSetAccessor`): when an object literal
//! defines a property with ONLY a setter (no matching getter), that property's
//! type is the setter's first parameter type — not `undefined`. A getter-only
//! property is the getter return type (and readonly); a getter+setter pair
//! reads as the getter return type. The class, interface, and type-literal
//! accessor paths already implement `getter.or(setter)`; the object-literal
//! expression path was the outlier — it hard-coded the set-only read type to
//! `undefined`, so `const z = obj.x` was typed `undefined` and DTS emitted
//! `x: undefined` (witnessed by the `declFileObjectLiteralWithOnlySetter`
//! declaration-emit baseline).
//!
//! Owner: `crates/tsz-checker/src/types/computation/object_literal/accessor_element.rs`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", CheckerOptions::default())
        .iter()
        .map(|d| d.code)
        .collect()
}

fn ts2322(source: &str) -> usize {
    codes(source).iter().filter(|&&c| c == 2322).count()
}

/// The reported repro: a set-only accessor property is the setter parameter
/// type, so reading it into a `number` is clean (it was `undefined` -> TS2322).
#[test]
fn set_only_accessor_property_reads_as_setter_param_type() {
    let source = r#"
function makePoint(start: number) {
    return {
        b: 10,
        set x(value: number) { this.b = value; }
    };
}
const point = makePoint(2);
const read: number = point.x;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "set-only accessor property must read as its setter parameter type (number), got: {:?}",
        codes(source)
    );
}

/// Object spread copies the runtime getter value. A set-only accessor has no
/// getter, so the copied property value is `undefined` even though direct
/// property lookup on the source uses the setter parameter type.
#[test]
fn direct_object_literal_spread_copies_set_only_accessor_as_undefined() {
    let source = r#"
const target: { foo: number, renamed: undefined } = {
    foo: 1,
    ...{ set renamed(value: number) { void value; } }
};
"#;
    assert_eq!(
        ts2322(source),
        0,
        "spreading a direct set-only accessor should copy an undefined value, got: {:?}",
        codes(source)
    );
}

/// If a direct object-literal spread overwrites an earlier property with a
/// set-only accessor, the spread value wins and is still `undefined`.
#[test]
fn direct_object_literal_spread_set_only_accessor_overwrites_with_undefined() {
    let source = r#"
const target: { renamed: undefined } = {
    renamed: 1,
    ...{ set renamed(value: number) { void value; } }
};
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a later set-only accessor spread should overwrite with undefined, got: {:?}",
        codes(source)
    );
}

/// Negative control: the property type is genuinely the setter parameter type,
/// so reading it into an incompatible annotation still errors. (A blanket
/// `undefined`/`any` fallback would wrongly silence this.)
#[test]
fn set_only_accessor_property_type_is_checked_against_target() {
    let source = r#"
const box = {
    set label(text: number) { void text; }
};
const wrong: string = box.label;
"#;
    assert!(
        ts2322(source) >= 1,
        "reading a number-typed set-only property into a string must error TS2322, got: {:?}",
        codes(source)
    );
}

/// A set-only property is writable (not readonly): assigning the setter's type
/// is clean. Uses a different binder/property name to keep the rule structural,
/// not identifier-driven.
#[test]
fn set_only_accessor_property_is_writable() {
    let source = r#"
const widget = {
    set size(px: number) { void px; }
};
widget.size = 42;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a set-only accessor property is writable with its parameter type, got: {:?}",
        codes(source)
    );
}

/// An unannotated setter parameter yields `any` (tsc: any when not annotated),
/// so any read/write is accepted — never a spurious mismatch.
#[test]
fn set_only_accessor_unannotated_param_is_any() {
    let source = r#"
const node = {
    set value(v) { void v; }
};
const a: number = node.value;
const b: string = node.value;
node.value = { nested: true };
"#;
    assert_eq!(
        ts2322(source),
        0,
        "an unannotated set-only accessor property is `any`, got: {:?}",
        codes(source)
    );
}

/// Adjacent shape (unchanged): a getter+setter pair still reads as the getter
/// return type, independent of the setter parameter type.
#[test]
fn getter_setter_pair_reads_as_getter_type() {
    let source = r#"
const cell = {
    _v: 0,
    get data(): number { return this._v; },
    set data(next: number) { this._v = next; }
};
const out: number = cell.data;
"#;
    assert_eq!(
        ts2322(source),
        0,
        "a getter+setter pair reads as the getter return type, got: {:?}",
        codes(source)
    );
}
