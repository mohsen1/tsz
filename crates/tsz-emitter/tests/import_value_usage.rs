//! Tests for the binder-backed import value-usage facts.
//!
//! The adjacent-case matrix mirrors the elision families the text-based
//! scanner historically mis-handled: keyword-named locals, multi-line type
//! annotations, shadowing, use-before-import, `typeof` type queries vs
//! runtime `typeof`, export re-exports, `import =` alias chains, and
//! external const-enum qualified accesses. Binder names vary across tests on
//! purpose (anti-hardcoding gate).

use super::{ImportValueUsageInputs, compute_import_value_usage_facts};
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_scanner::SyntaxKind;

fn parse_and_bind(source: &str) -> (ParserState, NodeIndex, BinderState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    (parser, root, binder)
}

/// Find the import-binding name node for `name` (specifier local name,
/// default/namespace clause name, or `import X = ...` alias).
fn binding_name_node(parser: &ParserState, name: &str) -> NodeIndex {
    for idx in 0..parser.arena.nodes.len() {
        let node_idx = NodeIndex(idx as u32);
        let Some(node) = parser.arena.get(node_idx) else {
            continue;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            continue;
        }
        let Some(ident) = parser.arena.get_identifier(node) else {
            continue;
        };
        if ident.escaped_text != name {
            continue;
        }
        let Some(parent_idx) = parser.arena.parent_of(node_idx) else {
            continue;
        };
        let Some(parent) = parser.arena.get(parent_idx) else {
            continue;
        };
        match parent.kind {
            syntax_kind_ext::IMPORT_SPECIFIER => {
                if parser
                    .arena
                    .get_specifier(parent)
                    .is_some_and(|spec| spec.name == node_idx)
                {
                    return node_idx;
                }
            }
            syntax_kind_ext::IMPORT_CLAUSE | syntax_kind_ext::NAMESPACE_IMPORT => {
                return node_idx;
            }
            syntax_kind_ext::NAMED_IMPORTS => {
                if parser
                    .arena
                    .get_named_imports(parent)
                    .is_some_and(|named| named.name == node_idx)
                {
                    return node_idx;
                }
            }
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if parser
                    .arena
                    .get_import_decl(parent)
                    .is_some_and(|import| import.import_clause == node_idx)
                {
                    return node_idx;
                }
            }
            _ => {}
        }
    }
    panic!("no import binding named `{name}` in test source");
}

/// Compute facts and return whether the binding named `name` is value-used.
fn binding_used(source: &str, name: &str) -> bool {
    binding_used_with_inputs(source, name, ImportValueUsageInputs::default())
}

fn binding_used_with_inputs(source: &str, name: &str, inputs: ImportValueUsageInputs<'_>) -> bool {
    let (parser, _root, binder) = parse_and_bind(source);
    let facts = compute_import_value_usage_facts(&parser.arena, &binder, inputs);
    let name_node = binding_name_node(&parser, name);
    facts
        .binding_value_used(name_node)
        .expect("binding should be known to the facts")
}

// =============================================================================
// Positive cases: bindings that must stay alive
// =============================================================================

#[test]
fn call_expression_is_value_usage() {
    assert!(binding_used(
        "import { alpha } from \"./m\";\nalpha();\n",
        "alpha"
    ));
}

#[test]
fn keyword_named_local_does_not_hide_usage() {
    // The text scanner whites out lines starting with `type `, so the
    // assignment `type = gamma;` (a variable named `type`) hid the use of
    // `gamma` and the import was falsely elided.
    assert!(binding_used(
        "import { gamma } from \"./m\";\nlet type = 5;\ntype = gamma;\n",
        "gamma"
    ));
}

#[test]
fn use_before_import_is_value_usage() {
    assert!(binding_used(
        "boot();\nimport { boot } from \"./m\";\n",
        "boot"
    ));
}

#[test]
fn runtime_typeof_is_value_usage() {
    assert!(binding_used(
        "import { conf } from \"./m\";\nconsole.log(typeof conf);\n",
        "conf"
    ));
}

#[test]
fn local_export_is_value_usage() {
    assert!(binding_used(
        "import { item } from \"./m\";\nexport { item };\n",
        "item"
    ));
}

#[test]
fn renamed_local_export_is_value_usage() {
    assert!(binding_used(
        "import { item } from \"./m\";\nexport { item as out };\n",
        "item"
    ));
}

#[test]
fn export_default_is_value_usage() {
    assert!(binding_used(
        "import { picked } from \"./m\";\nexport default picked;\n",
        "picked"
    ));
}

#[test]
fn class_extends_is_value_usage() {
    assert!(binding_used(
        "import { Base } from \"./m\";\nclass Sub extends Base {}\n",
        "Base"
    ));
}

#[test]
fn decorator_is_value_usage() {
    assert!(binding_used(
        "import { dec } from \"./m\";\n@dec\nclass Decorated {}\n",
        "dec"
    ));
}

#[test]
fn shorthand_property_is_value_usage() {
    assert!(binding_used(
        "import { short } from \"./m\";\nconst bag = { short };\n",
        "short"
    ));
}

#[test]
fn default_import_value_usage() {
    assert!(binding_used(
        "import widget from \"./m\";\nwidget.run();\n",
        "widget"
    ));
}

#[test]
fn namespace_import_value_usage() {
    assert!(binding_used(
        "import * as ns from \"./m\";\nns.go();\n",
        "ns"
    ));
}

#[test]
fn import_equals_chain_keeps_root_alive() {
    let source = "import lib = require(\"./lib\");\nimport sub = lib.child;\nsub();\n";
    assert!(binding_used(source, "sub"));
    assert!(binding_used(source, "lib"));
}

#[test]
fn exported_import_equals_keeps_root_alive() {
    let source = "import lib = require(\"./lib\");\nexport import sub = lib.child;\n";
    assert!(binding_used(source, "lib"));
}

// =============================================================================
// Negative cases: bindings tsc elides
// =============================================================================

#[test]
fn type_annotation_only_is_not_value_usage() {
    assert!(!binding_used(
        "import { Beta } from \"./m\";\nlet holder: Beta;\n",
        "Beta"
    ));
}

#[test]
fn multi_line_generic_annotation_is_not_value_usage() {
    // The per-line text stripper cannot follow annotations that span lines;
    // the AST walk can.
    assert!(!binding_used(
        "import { Delta } from \"./m\";\nconst v: Map<\n    Delta,\n    string\n> = new Map();\n",
        "Delta"
    ));
}

#[test]
fn shadowed_reference_is_not_value_usage() {
    assert!(!binding_used(
        "import { omega } from \"./m\";\nfunction f(omega: number) {\n    return omega + 1;\n}\n",
        "omega"
    ));
}

#[test]
fn typeof_type_query_is_not_value_usage() {
    assert!(!binding_used(
        "import { conf } from \"./m\";\ntype Conf = typeof conf;\n",
        "conf"
    ));
}

#[test]
fn type_only_export_clause_is_not_value_usage() {
    assert!(!binding_used(
        "import { item } from \"./m\";\nexport type { item };\n",
        "item"
    ));
}

#[test]
fn reexport_from_other_module_is_not_value_usage() {
    assert!(!binding_used(
        "import { thing } from \"./m\";\nexport { thing } from \"./other\";\n",
        "thing"
    ));
}

#[test]
fn property_name_positions_are_not_value_usage() {
    assert!(!binding_used(
        "import { prop } from \"./m\";\nconst holder = { prop: 1 };\nholder.prop;\n",
        "prop"
    ));
}

#[test]
fn implements_clause_is_not_value_usage() {
    assert!(!binding_used(
        "import { Contract } from \"./m\";\nclass Impl implements Contract {}\n",
        "Contract"
    ));
}

#[test]
fn interface_body_reference_is_not_value_usage() {
    assert!(!binding_used(
        "import { Shape } from \"./m\";\ninterface Box {\n    inner: Shape;\n}\n",
        "Shape"
    ));
}

#[test]
fn ambient_declaration_is_not_value_usage() {
    assert!(!binding_used(
        "import { Kind } from \"./m\";\ndeclare const probe: Kind;\n",
        "Kind"
    ));
}

#[test]
fn unused_import_equals_chain_is_not_value_usage() {
    let source = "import lib = require(\"./lib\");\nimport sub = lib.child;\n";
    assert!(!binding_used(source, "sub"));
    assert!(!binding_used(source, "lib"));
}

#[test]
fn unused_specifier_beside_used_one() {
    let source = "import { used, unused } from \"./m\";\nused();\n";
    assert!(binding_used(source, "used"));
    assert!(!binding_used(source, "unused"));
}

#[test]
fn external_const_enum_qualified_access_is_not_value_usage() {
    let mut const_enums = FxHashSet::default();
    const_enums.insert("Palette".to_string());
    let inputs = ImportValueUsageInputs {
        external_const_enum_bindings: Some(&const_enums),
        type_only_nodes: None,
    };
    assert!(!binding_used_with_inputs(
        "import { Palette } from \"./m\";\nconst c = Palette.Red;\n",
        "Palette",
        inputs,
    ));
    // A bare (non-qualified) use still keeps the binding.
    assert!(binding_used_with_inputs(
        "import { Palette } from \"./m\";\nconst c = Palette;\n",
        "Palette",
        inputs,
    ));
}

// =============================================================================
// End-to-end: facts threaded through the printer
// =============================================================================

fn emit_with_facts(source: &str, module: ModuleKind) -> String {
    let (parser, root, binder) = parse_and_bind(source);
    let facts =
        compute_import_value_usage_facts(&parser.arena, &binder, ImportValueUsageInputs::default());
    let options = PrinterOptions {
        module,
        import_usage_facts: Some(Arc::new(facts)),
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn es_emit_keeps_import_used_through_keyword_named_local() {
    let source = "import { used, unused } from \"./m\";\nlet type = 1;\ntype = used;\n";
    let output = emit_with_facts(source, ModuleKind::ESNext);
    assert!(
        output.contains("used"),
        "binding referenced through a keyword-named local must survive.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("unused"),
        "unreferenced specifier must be elided.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_emit_elides_import_without_value_usage() {
    let source = "import { Marker } from \"./m\";\nlet probe: Marker;\nprobe;\n";
    let output = emit_with_facts(source, ModuleKind::CommonJS);
    assert!(
        !output.contains("require(\"./m\")"),
        "type-only-used import must not emit a require.\nOutput:\n{output}"
    );
}

#[test]
fn cjs_emit_keeps_import_with_value_usage() {
    let source = "import { runner } from \"./m\";\nrunner();\n";
    let output = emit_with_facts(source, ModuleKind::CommonJS);
    assert!(
        output.contains("require(\"./m\")"),
        "value-used import must emit a require.\nOutput:\n{output}"
    );
}
