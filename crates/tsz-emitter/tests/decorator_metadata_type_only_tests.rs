//! `design:type` / `design:paramtypes` / `design:returntype` decorator metadata
//! must not serialize a type-only reference (interface, type alias, type
//! parameter, type-only import) to its bare erased spelling — that binding does
//! not exist at runtime and reading the metadata throws a `ReferenceError`.
//!
//! Structural rule: when a metadata type reference resolves to a declaration
//! with no runtime value (an interface or type parameter), `tsc` serializes it
//! as `Object`; when it resolves to a type alias, `tsc` resolves the alias to
//! its target type and serializes *that* type (`type S = string` -> `String`,
//! `type O = {a: number}` -> `Object`, `type R = SomeClass` -> `Object`, since
//! `tsc` never chases an alias to a constructor value). Only a reference that
//! names a runtime value usable as a type (a class) serializes to its binding.
//!
//! Tests vary the declaration names so they pin the structural rule rather than
//! a particular spelling.

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
        target: ScriptTarget::ES2017,
        ..Default::default()
    }
}

fn assert_design_type(output: &str, member: &str, serialized: &str) {
    let needle = format!("__metadata(\"design:type\", {serialized})\n], C.prototype, \"{member}\"");
    assert!(
        output.contains(&needle),
        "expected `{member}` design:type to serialize to `{serialized}`.\nOutput:\n{output}"
    );
}

/// The repro from the issue: an interface and four type aliases plus an alias to
/// a class. None may emit the erased type-only name.
#[test]
fn type_only_references_do_not_emit_erased_names() {
    let source = r#"declare const dec: any;
interface IFace { x: number; }
type StrAlias = string;
type NumAlias = number;
type ObjAlias = { a: number };
type UnionAlias = string | number;
class Real {}
type RealAlias = Real;
class C {
  @dec a: IFace;
  @dec b: StrAlias;
  @dec c: NumAlias;
  @dec d: ObjAlias;
  @dec e: UnionAlias;
  @dec f: RealAlias;
  @dec g: Real;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_design_type(&output, "a", "Object"); // interface
    assert_design_type(&output, "b", "String"); // alias -> string
    assert_design_type(&output, "c", "Number"); // alias -> number
    assert_design_type(&output, "d", "Object"); // alias -> object literal
    assert_design_type(&output, "e", "Object"); // alias -> string | number
    assert_design_type(&output, "f", "Object"); // alias -> class (not chased to value)
    assert_design_type(&output, "g", "Real"); // direct class reference (positive control)

    // No erased type-only name may appear as a metadata argument.
    for erased in [
        "IFace",
        "StrAlias",
        "NumAlias",
        "ObjAlias",
        "UnionAlias",
        "RealAlias",
    ] {
        let bad = format!("__metadata(\"design:type\", {erased})");
        assert!(
            !output.contains(&bad),
            "erased type-only name `{erased}` leaked into metadata.\nOutput:\n{output}"
        );
    }
}

/// Renamed binders prove the fix is structural, not keyed on a spelling.
#[test]
fn renamed_binders_classify_the_same_way() {
    let source = r#"declare const deco: any;
interface Shape { x: number; }
type Aliased = boolean;
class Widget {}
type WidgetAlias = Widget;
class C {
  @deco p: Shape;
  @deco q: Aliased;
  @deco r: WidgetAlias;
  @deco s: Widget;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_design_type(&output, "p", "Object"); // interface
    assert_design_type(&output, "q", "Boolean"); // alias -> boolean
    assert_design_type(&output, "r", "Object"); // alias -> class
    assert_design_type(&output, "s", "Widget"); // direct class
}

/// A generic alias resolves to its target's structure (`Box<T> = T[]` -> Array),
/// and an alias chain follows transitively to the eventual primitive.
#[test]
fn generic_alias_and_alias_chain() {
    let source = r#"declare const dec: any;
type Box<T> = T[];
type First = Second;
type Second = string;
class C {
  @dec a: Box<number>;
  @dec b: First;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_design_type(&output, "a", "Array"); // generic alias -> array
    assert_design_type(&output, "b", "String"); // alias chain -> string
}

/// `design:paramtypes` and `design:returntype` use the same serializer, so a
/// type-only parameter/return type must also be `Object`, never the erased name.
#[test]
fn paramtypes_and_returntype_classify_type_only_as_object() {
    let source = r#"declare const dec: any;
interface IFace { x: number; }
type StrAlias = string;
class C {
  @dec method(a: IFace, b: StrAlias): IFace { return {} as any; }
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert!(
        output.contains("__metadata(\"design:paramtypes\", [Object, String])"),
        "paramtypes should classify interface->Object and alias->String.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:returntype\", Object)"),
        "returntype of an interface should be Object.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("IFace") && !output.contains("StrAlias"),
        "no erased type-only name may leak into method metadata.\nOutput:\n{output}"
    );
}

/// The ES5 downlevel decorator path uses a separate serializer, so it must
/// classify type-only references the same way.
#[test]
fn es5_target_classifies_type_only_references() {
    let source = r#"declare const dec: any;
interface IFace { x: number; }
type StrAlias = string;
class Real {}
type RealAlias = Real;
class C {
  @dec a: IFace;
  @dec b: StrAlias;
  @dec f: RealAlias;
  @dec g: Real;
}
"#;
    let options = PrinterOptions {
        legacy_decorators: true,
        emit_decorator_metadata: true,
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let output = emit_source(source, options);
    assert!(
        output.contains("__metadata(\"design:type\", Object)")
            && output.contains("__metadata(\"design:type\", String)"),
        "ES5 metadata should classify interface->Object and alias->String.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__metadata(\"design:type\", Real)"),
        "ES5 metadata should keep a direct class reference.\nOutput:\n{output}"
    );
    for erased in ["IFace", "StrAlias", "RealAlias"] {
        let bad = format!("__metadata(\"design:type\", {erased})");
        assert!(
            !output.contains(&bad),
            "ES5: erased type-only name `{erased}` leaked into metadata.\nOutput:\n{output}"
        );
    }
}

/// A cyclic alias must not loop forever; it resolves to `Object`.
#[test]
fn cyclic_alias_terminates_as_object() {
    let source = r#"declare const dec: any;
type A = B;
type B = A;
class C {
  @dec a: A;
}
"#;
    let output = emit_source(source, legacy_metadata_options());
    assert_design_type(&output, "a", "Object");
}
