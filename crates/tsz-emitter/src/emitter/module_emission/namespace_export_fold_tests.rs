use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_commonjs_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        target,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn merged_exported_namespace_iifes_deduplicate_direct_export_slot() {
    let source = r#"export namespace X {
    export namespace Y {
        class A {}
    }
}
export namespace X {
    export namespace Y {
        export class B {}
    }
}
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("})(X || (exports.X = X = {}));"),
        "Merged exported namespace IIFEs should fold the direct export once.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.X = exports.X"),
        "Repeated exported namespace declarations should not duplicate the same export slot.\nOutput:\n{output}"
    );
}

#[test]
fn merged_exported_namespace_iifes_keep_distinct_alias_slots() {
    let source = r#"export namespace M {
    export var x;
}
export namespace M {
    export var y;
}
export { M as Alias };
"#;

    let output = emit_commonjs_with_target(source, ScriptTarget::ES2015);

    assert!(
        output.contains("})(M || (exports.Alias = exports.M = M = {}));"),
        "Distinct namespace export aliases should still fold in source order.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.M = exports.M"),
        "Duplicate direct namespace exports should collapse even when an alias is present.\nOutput:\n{output}"
    );
}
