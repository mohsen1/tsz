use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use tsz_common::ScriptTarget;

fn emit_system(source: &str) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target: ScriptTarget::ESNext,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// When a `var` is declared inside a conditional block and re-exported via
// `export { name }`, tsc emits `exports_1("name", name)` ONLY at the
// assignment site inside the block — not as an additional unconditional
// call at the execute-body level.
#[test]
fn system_conditional_var_no_duplicate_export() {
    let output = emit_system(
        r#"if (false) {
    var y = 1;
}
export { y };
"#,
    );
    let expected = r#"System.register([], function (exports_1, context_1) {
    "use strict";
    var y;
    var __moduleName = context_1 && context_1.id;
    return {
        setters: [],
        execute: function () {
            if (false) {
                y = 1;
                exports_1("y", y);
            }
        }
    };
});
"#;
    assert_eq!(
        output, expected,
        "Conditional var export must not emit a duplicate unconditional exports_1 call.\nOutput:\n{output}"
    );
}

// Same structural rule with a different iteration-variable name to confirm
// the fix is not tied to the spelling `y`.
#[test]
fn system_conditional_var_no_duplicate_export_different_name() {
    let output = emit_system(
        r#"if (false) {
    var count = 42;
}
export { count };
"#,
    );
    assert!(
        !output.contains("exports_1(\"count\", count);\n        }\n    };"),
        "No unconditional exports_1 call should follow the closing brace of the if block.\nOutput:\n{output}"
    );
    let execute_exports_count = output.matches("exports_1(\"count\", count)").count();
    assert_eq!(
        execute_exports_count, 1,
        "exports_1 for `count` should appear exactly once (inside the if block).\nOutput:\n{output}"
    );
}

// A var assigned in multiple branches should still only be exported at each
// assignment site, not additionally at the end of execute.
#[test]
fn system_multi_branch_var_no_duplicate_export() {
    let output = emit_system(
        r#"if (true) {
    var z = 1;
} else {
    var z = 2;
}
export { z };
"#,
    );
    let execute_exports_count = output.matches("exports_1(\"z\", z)").count();
    assert!(
        execute_exports_count <= 2,
        "exports_1 for `z` should not appear more than twice (once per branch).\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports_1(\"z\", z);\n        }\n    };"),
        "No trailing unconditional exports_1 call after the if/else block.\nOutput:\n{output}"
    );
}
