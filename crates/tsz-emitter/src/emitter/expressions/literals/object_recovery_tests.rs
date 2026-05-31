use crate::emitter::{Printer, PrinterOptions};
use tsz_common::ScriptTarget;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn object_literal_recovered_class_member_emits_empty_object_tail() {
    let source = "var box = {\n    class Renamed {\n    }\n}\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var box = {\n    class: Renamed\n}, {};"),
        "Recovered object class member should keep tsc's trailing empty object.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_real_class_property_does_not_emit_empty_object_tail() {
    let source = "var box = {\n    class: Renamed\n};\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var box = {\n    class: Renamed\n};"),
        "Ordinary class-named properties should not use recovery tail emit.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}, {};"),
        "Ordinary class-named properties must not get the recovery empty object.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_bare_class_shorthand_does_not_emit_empty_object_tail() {
    let source = "var box = { class };\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("var box = { class:  };"),
        "Bare invalid class shorthand should recover as a class-named property.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("}, {};"),
        "Bare invalid class shorthand must not get the recovered class-body tail.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_recovery_keeps_property_access_tail_with_shorthand() {
    let source = "var h = {\n    alpha.beta,\n    renamed.gamma,\n};\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("alpha, : .beta,"),
        "Recovered property-access member should stay attached to its shorthand base.\nOutput:\n{output}"
    );
    assert!(
        output.contains("renamed, : .gamma,"),
        "Recovery should not depend on a specific identifier spelling.\nOutput:\n{output}"
    );
}

#[test]
fn object_literal_recovery_keeps_element_access_tail_with_shorthand() {
    let source = "var h = {\n    alpha[\"beta\"],\n    renamed[1],\n};\n";

    let (parser, root) = parse_test_source(source);

    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("alpha, [\"beta\"]: ,"),
        "Recovered element-access member should stay attached to its shorthand base.\nOutput:\n{output}"
    );
    assert!(
        output.contains("renamed, [1]: ,"),
        "Recovery should also handle numeric computed names without name-specific logic.\nOutput:\n{output}"
    );
}
