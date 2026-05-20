use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

struct NamespaceCheckResult {
    formatted: String,
    display_props: Vec<(String, u32)>,
    shape_props: Vec<(String, u32)>,
    diagnostics: FxHashSet<String>,
}

fn check_namespace_symbol(
    files: &[(&str, &str)],
    entry_file: &str,
    symbol_name: &str,
    options: CheckerOptions,
) -> NamespaceCheckResult {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(roots[entry_idx]);

    let sym_id = checker
        .ctx
        .binder
        .file_locals
        .get(symbol_name)
        .expect("import symbol should exist");
    let ty = checker.get_type_of_symbol(sym_id);

    NamespaceCheckResult {
        formatted: checker.format_type_diagnostic(ty),
        display_props: checker
            .ctx
            .types
            .get_display_properties(ty)
            .map(|props| {
                props
                    .iter()
                    .map(|prop| {
                        (
                            checker.ctx.types.resolve_atom_ref(prop.name).to_string(),
                            prop.declaration_order,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        shape_props: tsz_solver::type_queries::get_object_shape(checker.ctx.types, ty).map_or(
            Vec::new(),
            |shape| {
                shape
                    .properties
                    .iter()
                    .map(|prop| {
                        (
                            checker.ctx.types.resolve_atom_ref(prop.name).to_string(),
                            prop.declaration_order,
                        )
                    })
                    .collect::<Vec<_>>()
            },
        ),
        diagnostics: checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| d.message_text.clone())
            .collect(),
    }
}

fn checker_options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2020,
        no_lib: true,
        ..CheckerOptions::default()
    }
}

fn assert_default_before_named_exports(message: &str) {
    assert!(
        message.contains("Type '{ default:"),
        "Expected TS2322 source type to render default before named exports. Actual: {message}"
    );
    assert!(
        !message.contains("Type '{ configs:"),
        "Expected TS2322 source type to avoid name-order rendering. Actual: {message}"
    );
}

#[test]
fn export_equals_typeof_import_namespace_preserves_default_before_named_exports() {
    let result = check_namespace_symbol(
        &[
            (
                "/pkg/index.d.ts",
                r#"
declare const pluginImportX: typeof import("./lib/index");
export = pluginImportX;
"#,
            ),
            (
                "/pkg/lib/index.d.ts",
                r#"
interface PluginConfig {
    parser?: string | null;
}
declare const configs: {
    "stage-0": PluginConfig;
};
declare const _default: {
    configs: {
        "stage-0": PluginConfig;
    };
};
export default _default;
export { configs };
"#,
            ),
            (
                "/main.ts",
                r#"
import * as pluginImportX from "./pkg/index";
interface Plugin {
  configs?: { [key: string]: { parser: string | null } };
}
const p: Plugin = pluginImportX;
"#,
            ),
        ],
        "/main.ts",
        "pluginImportX",
        checker_options(),
    );

    assert_eq!(result.formatted, "typeof import(\"lib/index\")");
    assert!(
        result.display_props.is_empty(),
        "Expected no display properties, got: {:?}",
        result.display_props
    );
    assert_eq!(
        result.shape_props,
        vec![("configs".to_string(), 2), ("default".to_string(), 1)],
        "Expected namespace object shape to preserve default-before-configs declaration order"
    );
    assert_eq!(
        result.diagnostics.len(),
        1,
        "Expected one TS2322, got: {:#?}",
        result.diagnostics
    );
    let message = result
        .diagnostics
        .iter()
        .next()
        .expect("TS2322 should exist");
    assert_default_before_named_exports(message);
}

#[test]
fn import_equals_commonjs_value_type_preserves_default_before_named_exports() {
    let result = check_namespace_symbol(
        &[
            (
                "/pkg/index.d.ts",
                r#"
declare const pluginImportX: typeof import("./lib/index");
export = pluginImportX;
"#,
            ),
            (
                "/pkg/lib/index.d.ts",
                r#"
interface PluginConfig {
    parser?: string | null;
}
declare const configs: {
    "stage-0": PluginConfig;
};
declare const _default: {
    configs: {
        "stage-0": PluginConfig;
    };
};
export default _default;
export { configs };
"#,
            ),
            (
                "/main.ts",
                r#"
import pluginImportX = require("./pkg/index");
interface Plugin {
  configs?: { [key: string]: { parser: string | null } };
}
const p: Plugin = pluginImportX;
"#,
            ),
        ],
        "/main.ts",
        "pluginImportX",
        checker_options(),
    );

    assert_eq!(
        result.diagnostics.len(),
        1,
        "Expected one TS2322, got: {:#?}",
        result.diagnostics
    );
    let message = result
        .diagnostics
        .iter()
        .next()
        .expect("TS2322 should exist");
    assert_default_before_named_exports(message);
}

#[test]
fn namespace_export_shape_preserves_source_order_for_three_named_exports() {
    let result = check_namespace_symbol(
        &[
            (
                "/pkg/index.d.ts",
                r#"
declare const pluginImportX: typeof import("./lib/index");
export = pluginImportX;
"#,
            ),
            (
                "/pkg/lib/index.d.ts",
                r#"
declare const zebra: { kind: "zebra" };
declare const alpha: { kind: "alpha" };
declare const middle: { kind: "middle" };
declare const _default: {
    zebra: typeof zebra;
    alpha: typeof alpha;
    middle: typeof middle;
};
export default _default;
export { zebra, alpha, middle };
"#,
            ),
            (
                "/main.ts",
                r#"
import pluginImportX = require("./pkg/index");
interface Plugin {
  zebra: number;
  alpha: number;
  middle: number;
}
const p: Plugin = pluginImportX;
"#,
            ),
        ],
        "/main.ts",
        "pluginImportX",
        checker_options(),
    );

    assert_eq!(result.formatted, "typeof import(\"lib/index\")");
    let mut shape_props = result.shape_props.clone();
    shape_props.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        shape_props,
        vec![
            ("alpha".to_string(), 3),
            ("default".to_string(), 1),
            ("middle".to_string(), 4),
            ("zebra".to_string(), 2),
        ],
        "Expected namespace object shape to keep named exports in source declaration order"
    );
    assert_eq!(
        result.diagnostics.len(),
        1,
        "Expected one TS2322, got: {:#?}",
        result.diagnostics
    );
}
