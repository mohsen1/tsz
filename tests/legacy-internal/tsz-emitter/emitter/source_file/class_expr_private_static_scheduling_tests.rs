//! Structural emit tests for class expressions that combine private names with
//! static-field lowering.
//!
//! Structural rule: when the target supports native static blocks, a class
//! expression whose fields lower to static blocks can stay in direct class
//! expression position. When private names lower to helper storage, the helper
//! storage is part of the class-expression comma list and must be scheduled
//! before lowered static elements that can observe it.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str, target: ScriptTarget, use_define_for_class_fields: bool) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target,
        use_define_for_class_fields,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn esnext_class_expression_static_field_keeps_native_private_self_reference() {
    let source = "class Host {\n    #prop = 0;\n    static box = class Inner {\n        #foo = 1;\n        static child = class {\n            m() { new Inner().#foo; }\n        };\n    };\n}\n";
    let output = emit(source, ScriptTarget::ESNext, false);

    assert!(
        output.contains("static { this.box = class Inner {"),
        "ESNext useDefine=false should lower the static field to a static block without wrapping the class expression.\nOutput:\n{output}"
    );
    assert!(
        output.contains("new Inner().#foo;"),
        "Native private-name emit should keep the named class expression self-reference.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("(_a = class Inner") && !output.contains("new _a().#foo"),
        "Native private names must not synthesize a temp alias solely for the class-expression self-reference.\nOutput:\n{output}"
    );
}

#[test]
fn es2020_class_expression_private_storage_precedes_lowered_static_field() {
    let source = "class Host {\n    #prop = 0;\n    static box = class Inner {\n        #foo = 1;\n        static child = class {\n            m() { new Inner().#foo; }\n        };\n    };\n}\n";
    let output = emit(source, ScriptTarget::ES2020, false);

    let private_storage = output
        .find("_Inner_foo = new WeakMap()")
        .unwrap_or_else(|| {
            panic!("Expected lowered private storage for the nested class.\nOutput:\n{output}")
        });
    let static_child = output.find("_a.child = class").unwrap_or_else(|| {
        panic!(
            "Expected the lowered static field to use the class-expression temp.\nOutput:\n{output}"
        )
    });

    assert!(
        private_storage < static_child,
        "Lowered private storage must be scheduled before static field work that can observe it.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__classPrivateFieldGet(new _a(), _Inner_foo, \"f\")"),
        "Lowered references to the class-expression private field should use the class-expression temp.\nOutput:\n{output}"
    );
}

#[test]
fn lowered_private_access_preserves_asi_trailing_comment() {
    let source = "class C {\n    #x = 1;\n    m() {\n        new C().#x // keep\n    }\n}\n";
    let output = emit(source, ScriptTarget::ES2020, false);

    assert!(
        output.contains("__classPrivateFieldGet(new C(), _C_x, \"f\"); // keep"),
        "Lowering a private access expression statement must preserve same-line trailing comments even when the source used ASI.\nOutput:\n{output}"
    );
}
