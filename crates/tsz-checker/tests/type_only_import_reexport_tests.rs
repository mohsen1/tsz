//! Tests that `export { X }` of a local `import type` (or `import { type X }`)
//! binding marks the export specifier for emit elision.
//!
//! Structural rule: when `export { X }` references a local name `X` whose
//! binder symbol has `is_type_only = true` (declared via `import type`,
//! `import { type X }`, `import type default`, or `import type * as X`),
//! the local binding has no runtime form. The export specifier must land in
//! `ctx.type_only_nodes` so the CommonJS / preserve emitter elides it from
//! the `exports.X = void 0` preamble and from any `Object.defineProperty`
//! re-export wiring — matching tsc's behavior of dropping the binding from
//! emit even though the source module's export may itself be a runtime value.
//!
//! Each test varies the binding spelling and import shape to verify the rule
//! lives at the structural level, not on a specific identifier name.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_solver::construction::TypeInterner;

/// Run the checker over a two-file project (`./mod` and `entry.ts`) and
/// return the set of export-specifier `NodeIndex` values inside `entry.ts`
/// along with the checker's `type_only_nodes` set.
fn check_two_files_collect_export_specifiers(
    mod_source: &str,
    entry_source: &str,
) -> (Vec<(String, NodeIndex)>, FxHashSet<NodeIndex>) {
    let mod_name = "mod.ts";
    let entry_name = "entry.ts";
    let module_specifier = "./mod";

    let mut parser_a = ParserState::new(mod_name.to_string(), mod_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new(entry_name.to_string(), entry_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // `import_binding_is_type_only` follows the module specifier into
    // `binder_a`'s export table, so seed `binder_b.module_exports` for the
    // re-export path. No `register_symbol_file_target` is needed: the
    // assertion only inspects `ctx.type_only_nodes`, which is populated
    // before any cross-file type construction.
    if let Some(exports) = binder_a.module_exports.get(mod_name).cloned() {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert(module_specifier.to_string(), exports);
    }

    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![
        Arc::new(parser_a.get_arena().clone()),
        Arc::clone(&arena_b),
    ]);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::new(binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        entry_name.to_string(),
        CheckerOptions {
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, module_specifier.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_specifier.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    let mut specifiers: Vec<(String, NodeIndex)> = Vec::new();
    let arena = arena_b.as_ref();
    if let Some(sf_node) = arena.get(root_b)
        && let Some(sf) = arena.get_source_file(sf_node)
    {
        for &stmt_idx in &sf.statements.nodes {
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = arena.get_export_decl(stmt) else {
                continue;
            };
            let Some(clause_node) = arena.get(export_decl.export_clause) else {
                continue;
            };
            let Some(named_exports) = arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named_exports.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };
                let name_node_idx = spec.name;
                let Some(name_node) = arena.get(name_node_idx) else {
                    continue;
                };
                let Some(ident) = arena.get_identifier(name_node) else {
                    continue;
                };
                specifiers.push((ident.escaped_text.to_string(), spec_idx));
            }
        }
    }

    (specifiers, checker.ctx.type_only_nodes.clone())
}

fn assert_spec_marked(specs: &[(String, NodeIndex)], type_only: &FxHashSet<NodeIndex>, name: &str) {
    let Some((_, idx)) = specs.iter().find(|(n, _)| n == name) else {
        panic!("export specifier {name:?} not found; got {specs:?}");
    };
    assert!(
        type_only.contains(idx),
        "export {{ {name} }} should be marked type-only, but was not. \
         Specs={specs:?} type_only={type_only:?}"
    );
}

fn assert_spec_not_marked(
    specs: &[(String, NodeIndex)],
    type_only: &FxHashSet<NodeIndex>,
    name: &str,
) {
    let Some((_, idx)) = specs.iter().find(|(n, _)| n == name) else {
        panic!("export specifier {name:?} not found; got {specs:?}");
    };
    assert!(
        !type_only.contains(idx),
        "export {{ {name} }} should NOT be marked type-only, but was. \
         Specs={specs:?} type_only={type_only:?}"
    );
}

// ── Type-only IMPORT, value SOURCE ──────────────────────────────────────────
//
// In each of these cases the source module's export is a runtime value
// (`class K {}`), but the local binding is decorated as type-only. tsc drops
// the re-export from emit because the local has no runtime slot.

/// `import type { K } from "./mod"; export { K };`
/// Bound variable name `K`.
#[test]
fn type_only_named_import_reexport_marks_specifier_k() {
    let mod_src = "export class K {}\n";
    let entry_src = "import type { K } from \"./mod\";\nexport { K };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "K");
}

/// Same rule, different identifier spelling: `Widget`.
/// Verifies the fix is not gated on a specific name.
#[test]
fn type_only_named_import_reexport_marks_specifier_widget() {
    let mod_src = "export class Widget {}\n";
    let entry_src = "import type { Widget } from \"./mod\";\nexport { Widget };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "Widget");
}

/// `import { type K } from "./mod"; export { K };`
/// Specifier-level `type` modifier — the binder also flips `sym.is_type_only`.
#[test]
fn inline_type_named_import_reexport_marks_specifier() {
    let mod_src = "export class K {}\n";
    let entry_src = "import { type K } from \"./mod\";\nexport { K };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "K");
}

/// `import type { Origin as Local } from "./mod"; export { Local };`
/// Renaming on the import side — the local name `Local` is what `export`
/// references; the binder still flags `Local` as type-only.
#[test]
fn renamed_type_only_import_reexport_marks_specifier() {
    let mod_src = "export class Origin {}\n";
    let entry_src = "import type { Origin as Local } from \"./mod\";\nexport { Local };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "Local");
}

/// `import type { K } from "./mod"; export { K as Out };`
/// Renaming on the export side — the local-name lookup still resolves to the
/// type-only-imported `K`, so the rule should fire.
#[test]
fn type_only_import_with_renaming_export_marks_specifier() {
    let mod_src = "export class K {}\n";
    let entry_src = "import type { K } from \"./mod\";\nexport { K as Out };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "Out");
}

/// `import type * as N from "./mod"; export { N };`
/// Namespace import is also type-only.
#[test]
fn type_only_namespace_import_reexport_marks_specifier() {
    let mod_src = "export class A {}\nexport class B {}\n";
    let entry_src = "import type * as N from \"./mod\";\nexport { N };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "N");
}

/// `import type Default from "./mod"; export { Default };`
/// Default import is also type-only.
#[test]
fn type_only_default_import_reexport_marks_specifier() {
    let mod_src = "export default class M {}\n";
    let entry_src = "import type Default from \"./mod\";\nexport { Default };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "Default");
}

// ── Mixed type-only + value imports re-exported together ────────────────────

/// `import type { T } from "./mod"; import { V } from "./mod";`
/// `export { T }; export { V };`
/// `T` is elided; `V` survives. Two separate export clauses.
#[test]
fn mixed_type_and_value_imports_two_export_clauses() {
    let mod_src = "export class T {}\nexport class V {}\n";
    let entry_src = "import type { T } from \"./mod\";\nimport { V } from \"./mod\";\nexport { T };\nexport { V };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "T");
    assert_spec_not_marked(&specs, &type_only, "V");
}

/// Same as above but inside a single export clause:
/// `import { type T, V } from "./mod"; export { T, V };`
#[test]
fn mixed_inline_type_and_value_single_export_clause() {
    let mod_src = "export class T {}\nexport class V {}\n";
    let entry_src = "import { type T, V } from \"./mod\";\nexport { T, V };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_marked(&specs, &type_only, "T");
    assert_spec_not_marked(&specs, &type_only, "V");
}

// ── Negative cases ──────────────────────────────────────────────────────────

/// A plain value import is NOT type-only, even when the consumer never uses
/// it locally. `export { V }` must survive emit as a real re-export.
#[test]
fn plain_value_import_reexport_not_marked() {
    let mod_src = "export class V {}\n";
    let entry_src = "import { V } from \"./mod\";\nexport { V };\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_not_marked(&specs, &type_only, "V");
}

/// An `export type { X }` declaration is itself fully erased by the emitter
/// based on the declaration-level marker; the per-specifier marking in
/// `type_only_nodes` is allowed but not required here. We only assert that
/// a plain value re-export sitting next to it survives.
#[test]
fn explicit_export_type_does_not_mark_neighboring_value_export() {
    let mod_src = "export class V {}\nexport class T {}\n";
    let entry_src = "export type { T } from \"./mod\";\nexport { V } from \"./mod\";\n";
    let (specs, type_only) = check_two_files_collect_export_specifiers(mod_src, entry_src);
    assert_spec_not_marked(&specs, &type_only, "V");
}
