/// Tests for CJS emit of named import re-exports.
///
/// When TypeScript compiles `import { v1 as v } from "./mod"; export { v }` to
/// CommonJS, tsc emits a live-binding `Object.defineProperty` getter rather than
/// a static `exports.v = v` assignment. This file verifies that tsz matches that
/// behavior across different name spellings and numbers of specifiers, and that
/// checker-supplied type-only annotations cause specifiers to be elided.
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use tsz_parser::parser::NodeIndex;

use super::parse_test_source;

fn collect_all_export_specifier_indices(
    source: &str,
) -> (
    tsz_parser::ParserState,
    tsz_parser::parser::NodeIndex,
    rustc_hash::FxHashSet<NodeIndex>,
) {
    let (parser, root) = parse_test_source(source);
    let mut specifier_indices = rustc_hash::FxHashSet::default();
    if let Some(sf_node) = parser.arena.get(root)
        && let Some(sf) = parser.arena.get_source_file(sf_node)
    {
        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt) = parser.arena.get(stmt_idx) else {
                continue;
            };
            let Some(export_decl) = parser.arena.get_export_decl(stmt) else {
                continue;
            };
            let Some(clause_node) = parser.arena.get(export_decl.export_clause) else {
                continue;
            };
            let Some(named_exports) = parser.arena.get_named_imports(clause_node) else {
                continue;
            };
            specifier_indices.extend(named_exports.elements.nodes.iter().copied());
        }
    }
    (parser, root, specifier_indices)
}

fn emit_commonjs(source: &str) -> String {
    emit_commonjs_with_type_only(source, rustc_hash::FxHashSet::default())
}

fn emit_commonjs_with_type_only(
    source: &str,
    type_only_nodes: rustc_hash::FxHashSet<NodeIndex>,
) -> String {
    let (parser, root) = parse_test_source(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        type_only_nodes: std::sync::Arc::new(type_only_nodes),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// ── Object.defineProperty getter for named import re-exports ────────────────

/// When `import { origName as localName } from "mod"; export { localName }`,
/// tsc emits a live-binding getter so the exported value always reflects the
/// current module binding, not a snapshot at module-load time.
/// This test uses alias spelling "v1 → v".
#[test]
fn named_import_reexport_emits_define_property_getter_v_alias() {
    let source = r#"import { v1 as v } from "./mod";
export { v };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains(r#"Object.defineProperty(exports, "v", { enumerable: true, get: function () { return mod_1.v1; } });"#),
        "Named import re-export should emit a live-binding getter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.v = v;"),
        "Named import re-export must not emit a static assignment.\nOutput:\n{output}"
    );
}

/// Same rule with a different alias spelling: "first1 → first".
/// Verifies the fix is not keyed to a specific identifier name.
#[test]
fn named_import_reexport_emits_define_property_getter_first_alias() {
    let source = r#"import { first1 as first } from "./util";
export { first };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains(r#"Object.defineProperty(exports, "first", { enumerable: true, get: function () { return util_1.first1; } });"#),
        "Named import re-export with 'first' alias should emit a live-binding getter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.first = first;"),
        "Named import re-export must not emit a static assignment.\nOutput:\n{output}"
    );
}

/// Multiple named-import re-exports in a single export clause each get their
/// own Object.defineProperty getter with the correct source module reference.
#[test]
fn multiple_named_import_reexports_each_emit_define_property_getter() {
    let source = r#"import { alpha1 as alpha, beta1 as beta } from "./pkg";
export { alpha, beta };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains(r#"Object.defineProperty(exports, "alpha", { enumerable: true, get: function () { return pkg_1.alpha1; } });"#),
        "alpha re-export should emit a getter.\nOutput:\n{output}"
    );
    assert!(
        output.contains(r#"Object.defineProperty(exports, "beta", { enumerable: true, get: function () { return pkg_1.beta1; } });"#),
        "beta re-export should emit a getter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.alpha = alpha;") && !output.contains("exports.beta = beta;"),
        "Named import re-exports must not emit static assignments.\nOutput:\n{output}"
    );
}

/// A local declaration export (not a named import) still emits a static assignment,
/// not an Object.defineProperty getter.  This verifies the getter path only fires
/// when the local name has a CommonJS named-import substitution.
#[test]
fn local_var_export_still_emits_static_assignment() {
    let source = r#"var myVar = 42;
export { myVar };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains("exports.myVar = myVar;"),
        "Local variable re-export should still emit a static assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("Object.defineProperty(exports, \"myVar\""),
        "Local variable re-export should not emit a getter.\nOutput:\n{output}"
    );
}

// ── Type-only specifier elision in export-from path ─────────────────────────

/// When the checker marks all specifiers in `export { A, B } from "./mod"` as
/// type-only (simulating interface / type alias / const enum elision), none of
/// them should appear in the JavaScript output.
#[test]
fn type_only_marked_reexport_from_elides_all_specifiers() {
    let source = r#"export { InterfaceX, TypeAliasY } from "./types";"#;
    let (parser, root, all_spec_indices) = collect_all_export_specifier_indices(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        type_only_nodes: std::sync::Arc::new(all_spec_indices),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("InterfaceX") && !output.contains("TypeAliasY"),
        "Checker-marked type-only specifiers in re-export-from must be elided.\nOutput:\n{output}"
    );
}

/// Same elision rule with different specifier names, confirming the fix is
/// not gated on particular identifier spellings.
#[test]
fn type_only_marked_reexport_from_elides_differently_named_specifiers() {
    let source = r#"export { SomeKind, AnotherAlias } from "./defs";"#;
    let (parser, root, all_spec_indices) = collect_all_export_specifier_indices(source);
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        type_only_nodes: std::sync::Arc::new(all_spec_indices),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("SomeKind") && !output.contains("AnotherAlias"),
        "Type-only specifiers with different names must also be elided.\nOutput:\n{output}"
    );
    // The __esModule defineProperty is always present; verify the source module's
    // require() and any specifier-specific defineProperty calls are absent.
    assert!(
        !output.contains("require(\"./defs\")"),
        "No require for the source module should be emitted when all specifiers are type-only.\nOutput:\n{output}"
    );
}

/// Mixed case: only some specifiers are type-only.  Value specifier survives;
/// type-only specifier is elided.
#[test]
fn type_only_marked_mixed_reexport_elides_only_type_only_specifiers() {
    let source = r#"export { ValueA, TypeB } from "./mixed";"#;
    let (parser, root, all_spec_indices) = collect_all_export_specifier_indices(source);
    // Mark only the second specifier (TypeB) as type-only.
    // Collect them in order so we can target just the second one.
    let (parser2, root2) = parse_test_source(source);
    let mut type_only_nodes = rustc_hash::FxHashSet::default();
    if let Some(sf_node) = parser2.arena.get(root2)
        && let Some(sf) = parser2.arena.get_source_file(sf_node)
    {
        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt) = parser2.arena.get(stmt_idx) else {
                continue;
            };
            let Some(export_decl) = parser2.arena.get_export_decl(stmt) else {
                continue;
            };
            let Some(clause_node) = parser2.arena.get(export_decl.export_clause) else {
                continue;
            };
            let Some(named_exports) = parser2.arena.get_named_imports(clause_node) else {
                continue;
            };
            // Mark only the second element (TypeB) as type-only.
            if let Some(&second) = named_exports.elements.nodes.get(1) {
                type_only_nodes.insert(second);
            }
        }
    }
    let options = PrinterOptions {
        module: ModuleKind::CommonJS,
        type_only_nodes: std::sync::Arc::new(type_only_nodes),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser2.arena, options);
    printer.set_source_text(source);
    printer.emit(root2);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("ValueA"),
        "Value specifier should survive when only the other specifier is type-only.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("TypeB"),
        "Type-only specifier should be elided from the re-export.\nOutput:\n{output}"
    );
    let _ = (parser, root, all_spec_indices); // unused from first parse
}

// ── Plain assignment for default import re-exports ──────────────────────────
//
// A **default import clause** binding (`import x from "mod"`) is not a live
// binding the way a named import specifier is: tsc re-exports it with a plain
// `exports.x = mod_1.default;` assignment, not an `Object.defineProperty`
// getter. These tests pin that distinction (getter vs assignment) so the two
// import forms do not converge on one emit shape.

/// `import data from "./mod"; export { data }` must emit a plain assignment
/// `exports.data = mod_1.default;`, never a live-binding getter.
#[test]
fn default_import_reexport_emits_plain_assignment() {
    let source = r#"import data from "./mod";
export { data };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains("exports.data = mod_1.default;"),
        "Default import re-export should emit a plain assignment.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("get: function () { return mod_1.default; }"),
        "Default import re-export must NOT emit a live-binding getter.\nOutput:\n{output}"
    );
}

/// A renamed default re-export (`export { data as renamed }`) still assigns.
/// Binder name varied (`payload`) to keep the decision structural, not literal.
#[test]
fn default_import_reexport_renamed_emits_plain_assignment() {
    let source = r#"import payload from "./mod";
export { payload as renamed };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains("exports.renamed = mod_1.default;"),
        "Renamed default import re-export should assign the default.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("get: function"),
        "Renamed default import re-export must NOT emit a getter.\nOutput:\n{output}"
    );
}

// ── Position: the re-export binding follows its own import's require() ─────
//
// tsc's CommonJS transform emits a bare `export { x }` re-export of an import
// binding immediately after that import's own `require(...)` line, not at the
// `export { }` statement's source position. #16924.

/// A statement (`export const a = 1;`) sitting between the import and the
/// bare re-export must not push the getter to the tail — it stays right after
/// the `require(...)` it belongs to.
#[test]
fn import_reexport_getter_follows_its_own_require_not_trailing_export_clause() {
    let source = r#"import { readFile } from "fs";
export const a = 1;
export function f() { return a; }
export { readFile };
"#;
    let output = emit_commonjs(source);
    let require_pos = output
        .find("require(\"fs\")")
        .expect("require(\"fs\") should be emitted");
    let getter_pos = output
        .find(r#"Object.defineProperty(exports, "readFile""#)
        .expect("readFile getter should be emitted");
    let a_pos = output
        .find("exports.a = 1;")
        .expect("exports.a assignment should be emitted");
    assert!(
        getter_pos < a_pos,
        "readFile getter must come right after its own require(), before the \
         unrelated `exports.a = 1;` that sits between the import and the \
         export clause in source order.\nOutput:\n{output}"
    );
    assert!(
        require_pos < getter_pos,
        "readFile getter must come after its own require() line.\nOutput:\n{output}"
    );
}

/// Two imports, each re-exported: each binding follows its OWN `require()`,
/// not one trailing block after both requires.
#[test]
fn two_import_reexports_each_follow_their_own_require() {
    let source = r#"import { x1 as x } from "./a";
import { y1 as y } from "./b";
export { x, y };
"#;
    let output = emit_commonjs(source);
    let require_a = output.find("require(\"./a\")").unwrap();
    let require_b = output.find("require(\"./b\")").unwrap();
    let getter_x = output
        .find(r#"Object.defineProperty(exports, "x""#)
        .unwrap();
    let getter_y = output
        .find(r#"Object.defineProperty(exports, "y""#)
        .unwrap();
    assert!(
        require_a < getter_x && getter_x < require_b,
        "x's getter must land between a's require() and b's require(), not \
         after both.\nOutput:\n{output}"
    );
    assert!(
        require_b < getter_y,
        "y's getter must come after b's require().\nOutput:\n{output}"
    );
}

/// A default-import re-export (plain assignment, not a getter) also follows
/// its own `require()`, matching the getter-form rule above.
#[test]
fn default_import_reexport_assignment_follows_its_own_require() {
    let source = r#"import data from "./mod";
export const a = 1;
export { data };
"#;
    let output = emit_commonjs(source);
    let require_pos = output.find("require(\"./mod\")").unwrap();
    let assign_pos = output.find("exports.data = mod_1.default;").unwrap();
    let a_pos = output.find("exports.a = 1;").unwrap();
    assert!(
        require_pos < assign_pos && assign_pos < a_pos,
        "data's re-export assignment must come right after its own require(), \
         before the unrelated `exports.a = 1;`.\nOutput:\n{output}"
    );
}

/// A local (non-import) re-export in the same clause as an import re-export
/// is unaffected: it still emits at the `export { }` statement's own position.
#[test]
fn local_reexport_still_emits_at_export_clause_position_alongside_import_reexport() {
    let source = r#"import { v1 as v } from "./mod";
var local = 2;
export { v, local };
"#;
    let output = emit_commonjs(source);
    let require_pos = output.find("require(\"./mod\")").unwrap();
    let getter_pos = output
        .find(r#"Object.defineProperty(exports, "v""#)
        .unwrap();
    let local_pos = output.find("exports.local = local;").unwrap();
    assert!(
        require_pos < getter_pos && getter_pos < local_pos,
        "v's getter follows its own require(); local's static assignment \
         stays at the export clause's own position, after it.\nOutput:\n{output}"
    );
}

/// A combined `import def, { a, b as bb }` re-exported as `export { def, a, bb as cc }`
/// mixes both forms: the default binding `def` assigns, while the named
/// specifiers `a`/`cc` keep their live-binding getters.
#[test]
fn combined_default_and_named_reexport_mixes_assignment_and_getter() {
    let source = r#"import def, { a, b as bb } from "./mod";
export { def, a, bb as cc };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains("exports.def = mod_1.default;"),
        "Default binding of a combined import should assign.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            r#"Object.defineProperty(exports, "a", { enumerable: true, get: function () { return mod_1.a; } });"#
        ),
        "Named specifier `a` should keep its live-binding getter.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            r#"Object.defineProperty(exports, "cc", { enumerable: true, get: function () { return mod_1.b; } });"#
        ),
        "Renamed named specifier `cc` should keep its live-binding getter.\nOutput:\n{output}"
    );
}

/// A `default`-named specifier (`import { default as d }`) is a named import,
/// not a default-clause binding: it must keep the live-binding getter even
/// though it also resolves to `mod_1.default`. This is the boundary case that
/// separates the two forms.
#[test]
fn default_named_specifier_reexport_keeps_getter() {
    let source = r#"import { default as d } from "./mod";
export { d };
"#;
    let output = emit_commonjs(source);
    assert!(
        output.contains(
            r#"Object.defineProperty(exports, "d", { enumerable: true, get: function () { return mod_1.default; } });"#
        ),
        "A default-named specifier is a named import and keeps its getter.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("exports.d = mod_1.default;"),
        "A `default`-named specifier must not collapse to a plain assignment.\nOutput:\n{output}"
    );
}
