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

/// Returns the byte offset of `needle` in `haystack`, asserting it is present.
fn index_of(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("expected to find {needle:?} in:\n{haystack}"))
}

// Structural rule: in a `System.register` factory body, the `exportStar_1`
// re-export helper (and its `exportedNames_1` exclusion map) must be emitted
// AFTER the hoisted `var` declarations and the `__moduleName` binding, just
// above `return { setters, execute }` — matching tsc. It must NOT precede the
// hoisted bindings.
#[test]
fn export_star_helper_after_hoisted_vars() {
    let output = emit_system(
        r#"export * from "./other";
export const x = 1;
"#,
    );

    // The helper block exists.
    let export_star_idx = index_of(&output, "function exportStar_1(m)");
    let module_name_idx = index_of(&output, "var __moduleName = context_1 && context_1.id;");
    let return_idx = index_of(&output, "return {");

    assert!(
        export_star_idx > module_name_idx,
        "exportStar_1 helper must come after the __moduleName binding (hoisted vars).\nOutput:\n{output}"
    );
    assert!(
        export_star_idx < return_idx,
        "exportStar_1 helper must come before the `return {{` object.\nOutput:\n{output}"
    );

    // The exclusion map likewise follows the hoisted bindings.
    let exported_names_idx = index_of(&output, "var exportedNames_1 = {");
    assert!(
        exported_names_idx > module_name_idx,
        "exportedNames_1 map must come after the __moduleName binding.\nOutput:\n{output}"
    );
}

// Same structural rule with a hoisted function declaration present. tsc emits
// the hoisted `function ...` + its `exports_1(...)` call BEFORE the
// `exportStar_1` helper. Varying the export/local names proves the ordering is
// keyed on statement shape, not on a particular spelling.
#[test]
fn export_star_helper_after_hoisted_function() {
    let output = emit_system(
        r#"export * from "./dep";
export function compute() {}
"#,
    );

    let func_decl_idx = index_of(&output, "function compute() {");
    let func_export_idx = index_of(&output, "exports_1(\"compute\", compute);");
    let export_star_idx = index_of(&output, "function exportStar_1(m)");

    assert!(
        func_decl_idx < export_star_idx,
        "Hoisted function declaration must precede the exportStar_1 helper.\nOutput:\n{output}"
    );
    assert!(
        func_export_idx < export_star_idx,
        "Hoisted function's exports_1 registration must precede the exportStar_1 helper.\nOutput:\n{output}"
    );
}

// A module with NO export-star re-export must not emit the helper at all,
// proving the placement change did not introduce the helper unconditionally.
#[test]
fn no_export_star_helper_without_reexport() {
    let output = emit_system(
        r#"export const value = 7;
"#,
    );
    assert!(
        !output.contains("function exportStar_1"),
        "exportStar_1 helper must not appear when there is no `export *` re-export.\nOutput:\n{output}"
    );
}
