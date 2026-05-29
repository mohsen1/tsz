//! `design:type` decorator metadata for a property whose type refers to a
//! numeric enum serializes to `Number`.
//!
//! Structural rule: when a metadata type reference (a bare identifier, a
//! qualified enum-member reference, or a union of such) resolves to a
//! homogeneously numeric enum declaration, tsc serializes the design type as
//! `Number`. This change makes tsz do the same by resolving the reference's
//! declaration to an enum and confirming no member has a string initializer,
//! rather than emitting the enum's own name. The "all members agree" rule in
//! the union serializer then folds `E.B | E.C` and `E | number` to `Number`.
//!
//! Tests vary the enum and member names so they pin the structural rule rather
//! than a single spelling.

use tsz_common::ScriptTarget;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_parser::ParserState;

fn emit_source(source: &str, options: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

fn legacy_metadata_options() -> PrinterOptions {
    PrinterOptions {
        legacy_decorators: true,
        emit_decorator_metadata: true,
        target: ScriptTarget::ES2015,
        ..Default::default()
    }
}

fn assert_metadata(output: &str, member: &str, serialized: &str) {
    let needle = format!("__metadata(\"design:type\", {serialized})\n], D.prototype, \"{member}\"");
    assert!(
        output.contains(&needle),
        "expected `{member}` metadata to serialize to `{serialized}`.\nOutput:\n{output}"
    );
}

/// A bare numeric-enum reference, a single enum-member reference, a union of
/// enum members, and a union of the enum with `number` all serialize to
/// `Number`. The enum here uses non-default member names to prove the rule is
/// not keyed on any particular spelling.
#[test]
fn numeric_enum_references_serialize_as_number() {
    let source = r#"declare const PropDeco: PropertyDecorator;
enum Color {
    Red,
    Green,
    Blue,
}
class D {
    @PropDeco
    a: Color.Red;
    @PropDeco
    b: Color.Green | Color.Blue;
    @PropDeco
    c: Color;
    @PropDeco
    d: Color | number;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_metadata(&output, "a", "Number");
    assert_metadata(&output, "b", "Number");
    assert_metadata(&output, "c", "Number");
    assert_metadata(&output, "d", "Number");
}

/// A const numeric enum is also serialized as `Number`, and an explicitly
/// numeric-initialized enum stays numeric regardless of member names.
#[test]
fn const_and_initialized_numeric_enum_serialize_as_number() {
    let source = r#"declare const PropDeco: PropertyDecorator;
const enum Flag {
    On = 1,
    Off = 0,
}
class D {
    @PropDeco
    a: Flag;
    @PropDeco
    b: Flag.On;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_metadata(&output, "a", "Number");
    assert_metadata(&output, "b", "Number");
}

/// A string (or heterogeneous) enum must NOT serialize to `Number`: its
/// members are not all numeric. A string enum reference falls back to the enum
/// object reference (not `Number`), matching tsc.
#[test]
fn string_enum_reference_does_not_serialize_as_number() {
    let source = r#"declare const PropDeco: PropertyDecorator;
enum Dir {
    Up = "UP",
    Down = "DOWN",
}
class D {
    @PropDeco
    a: Dir;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert!(
        !output.contains("__metadata(\"design:type\", Number)\n], D.prototype, \"a\""),
        "A string enum reference must not serialize as Number.\nOutput:\n{output}"
    );
}
