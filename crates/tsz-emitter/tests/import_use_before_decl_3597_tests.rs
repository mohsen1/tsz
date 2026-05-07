//! Regression test for issue #3597: emitter elides a value import when the
//! imported binding is only used BEFORE the import declaration.
//!
//! ES import declarations are module-scoped; a top-level use before the
//! import is still a value use that must keep the generated binding.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::output::printer::{PrintOptions, lower_and_print};
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrintOptions) -> String {
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    lower_and_print(&parser.arena, root, opts).code
}

#[test]
fn named_import_used_before_decl_keeps_binding_in_cjs() {
    let source = "callIt();\n\nimport { callIt } from \"./dep\";\n\nexport {};\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    // tsc emits a `require("./dep")` binding (typically named like dep_1)
    // that the call site dereferences. At minimum the require call must be
    // present — pre-fix tsz dropped it entirely.
    assert!(
        output.contains("require(\"./dep\")"),
        "import used before decl must still emit require(\"./dep\").\nOutput:\n{output}"
    );
}

#[test]
fn default_import_used_before_decl_keeps_binding_in_cjs() {
    let source = "callIt();\n\nimport callIt from \"./dep\";\n\nexport {};\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        output.contains("require(\"./dep\")"),
        "default import used before decl must still emit require(\"./dep\").\nOutput:\n{output}"
    );
}

// Sanity: a fully-unused import must STILL be elided. The pre-import-use
// fix must not turn the elision check into a no-op.
#[test]
fn fully_unused_named_import_is_still_elided_in_cjs() {
    let source = "import { unused } from \"./dep\";\n\nexport {};\n";
    let opts = PrintOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    // A type-only or fully-unused named binding has no value use anywhere,
    // so the require should be elided. tsc drops the import entirely.
    assert!(
        !output.contains("require(\"./dep\")"),
        "fully-unused import must still be elided.\nOutput:\n{output}"
    );
}
