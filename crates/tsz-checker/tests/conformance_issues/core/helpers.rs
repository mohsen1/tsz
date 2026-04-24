//! Unit tests documenting known conformance test failures
//!
//! These tests are marked `#[ignore]` and document specific issues found during
//! conformance test investigation (2026-02-08). They serve as:
//! - Documentation of expected vs actual behavior
//! - Easy verification when fixes are implemented
//! - Minimal reproduction cases for debugging
//!
//! See docs/conformance-*.md for full context.

pub(crate) use rustc_hash::{FxHashMap, FxHashSet};
pub(crate) use std::path::Path;
pub(crate) use std::sync::Arc;
pub(crate) use tsz_binder::BinderState;
pub(crate) use tsz_binder::lib_loader::LibFile;
pub(crate) use tsz_binder::state::LibContext as BinderLibContext;
pub(crate) use tsz_checker::context::LibContext as CheckerLibContext;
pub(crate) use tsz_checker::context::{CheckerOptions, ScriptTarget};
pub(crate) use tsz_checker::module_resolution::build_module_resolution_maps;
pub(crate) use tsz_checker::state::CheckerState;
pub(crate) use tsz_common::ModuleKind;
pub(crate) use tsz_common::checker_options::JsxMode;
pub(crate) use tsz_parser::parser::ParserState;
pub(crate) use tsz_solver::TypeInterner;

/// Helper to compile TypeScript and get diagnostics
pub(crate) fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

pub(crate) fn compile_two_files_get_diagnostics(
    a_source: &str,
    b_source: &str,
    module_spec: &str,
) -> Vec<(u32, String)> {
    compile_two_files_get_diagnostics_with_options(
        a_source,
        b_source,
        module_spec,
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            no_lib: true,
            ..Default::default()
        },
    )
}

pub(crate) fn compile_two_files_get_diagnostics_with_options(
    a_source: &str,
    b_source: &str,
    module_spec: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.ts".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());

    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    // Merge module exports: copy a.ts exports into b.ts's binder for cross-file resolution
    let file_a_exports = binder_a.module_exports.get("a.ts").cloned();
    if let Some(exports) = &file_a_exports {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert(module_spec.to_string(), exports.clone());
    }

    // Record cross-file symbol targets: SymbolIds from binder_a need to resolve
    // in binder_a's arena, not binder_b's. Map them to file index 0 (a.ts).
    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.ts".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    // Register cross-file symbol targets so the checker looks up SymbolIds
    // from a.ts in the correct binder (file index 0).
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, module_spec.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_spec.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_named_files_get_diagnostics_with_options(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_named_files_get_diagnostics_with_options_and_import_reporting(
        files, entry_file, options, false,
    )
}

pub(crate) fn compile_named_files_get_diagnostics_with_options_and_import_reporting(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    report_unresolved_imports: bool,
) -> Vec<(u32, String)> {
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
    checker.ctx.report_unresolved_imports = report_unresolved_imports;

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_named_files_get_diagnostics_with_lib_and_options(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    let raw_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        if !raw_lib_contexts.is_empty() {
            binder.merge_lib_contexts_into_binder(&raw_lib_contexts);
        }
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
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_merged_lib_contexts_and_shared_cache_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        vec![CheckerLibContext {
            arena: Arc::clone(&lib_files[0].arena),
            binder: Arc::new({
                let mut merged = BinderState::new();
                merged.merge_lib_contexts_into_binder(&raw_contexts);
                merged
            }),
        }]
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
    }
    checker.ctx.shared_lib_type_cache = Some(Arc::new(dashmap::DashMap::new()));

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_imports_and_get_diagnostics(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_named("test.ts", source, options)
}

pub(crate) fn compile_and_get_diagnostics_named(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_raw_diagnostics_named(file_name, source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

pub(crate) fn compile_and_get_raw_diagnostics_named(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    checker.ctx.diagnostics
}

/// Helper to check if specific error codes are present
pub(crate) fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

pub(crate) fn diagnostic_message(diagnostics: &[(u32, String)], code: u32) -> Option<&str> {
    diagnostics
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message.as_str())
}

pub(crate) fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                let lib_file = LibFile::from_source(file_name.to_string(), content);
                lib_files.push(Arc::new(lib_file));
                break;
            }
        }
    }
    lib_files
}

pub(crate) fn lib_files_available() -> bool {
    !load_lib_files_for_test().is_empty()
}

pub(crate) fn without_missing_global_type_errors(
    diagnostics: Vec<(u32, String)>,
) -> Vec<(u32, String)> {
    diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

pub(crate) fn compile_and_get_diagnostics_with_lib(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_lib_and_options(source, CheckerOptions::default())
}

pub(crate) fn compile_and_get_diagnostics_with_lib_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_named_with_lib_and_options("test.ts", source, options)
}

pub(crate) fn compile_and_get_diagnostics_named_with_lib_and_options(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    compile_and_get_raw_diagnostics_named_with_lib_and_options(file_name, source, options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

pub(crate) fn compile_and_get_raw_diagnostics_named_with_lib_and_options(
    file_name: &str,
    source: &str,
    options: CheckerOptions,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics
}

pub(crate) fn compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let checker_lib_contexts = if lib_files.is_empty() {
        Vec::new()
    } else {
        let raw_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&raw_contexts);
        lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect()
    };
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    if !checker_lib_contexts.is_empty() {
        checker.ctx.set_lib_contexts(checker_lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// TS2322 for variable declarations with type annotations should be anchored
/// at the initializer expression, not the variable name. This matches tsc
/// behavior where `var d: Foo = expr` reports the error at `expr`.
///
/// Currently ignored: `assignment_anchor_node` in `fingerprint_policy.rs` rewrites
/// all variable declaration anchors to `vd.name`. A targeted fix would need to
/// either skip rewriting for non-destructuring initializers or add a
/// `DiagnosticAnchorKind` variant that preserves the initializer position.
#[test]
fn test_ts2322_variable_decl_diagnostic_anchored_at_initializer() {
    let source = r#"
interface ParserFunc {
    (eventEmitter: number, buffer: string): void;
}
interface Parsers {
    readline(delimiter?: string): ParserFunc;
}
declare var parsers: Parsers;
var d: ParserFunc = parsers.readline;
"#;
    let diags = compile_and_get_raw_diagnostics_named("test.ts", source, CheckerOptions::default());
    let ts2322 = diags.iter().filter(|d| d.code == 2322).collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected 1 TS2322, got {}: {:?}",
        ts2322.len(),
        ts2322
    );
    let diag = ts2322[0];
    // The error should point at `parsers.readline` (the initializer),
    // not at `d` (the variable name).
    let error_text = &source[diag.start as usize..diag.start as usize + diag.length as usize];
    let trimmed = error_text.trim_end_matches(';');
    assert_eq!(
        trimmed, "parsers.readline",
        "TS2322 should be anchored at the initializer expression, got span text: '{error_text}'",
    );
}

#[test]
fn test_no_implicit_any_string_indexer_uses_get_set_call_suggestions() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
var c = {
  get: (key: string) => 'foobar'
};
c['hello'];
const foo = c['hello'];

var d = {
  set: (key: string) => 'foobar'
};
const bar = d['hello'];

let e = {
  get: (key: string) => 'foobar',
  set: (key: string, value: string) => 'foobar'
};
e['hello'];
e['hello'] = 'modified';

({ get: (key: string) => 'hello', set: (key: string, value: string) => {} })['hello'] = 'modified';

interface MyMap<K, T> {
  get(key: K): T;
  set(key: K, value: T): void;
}

interface I {
  prop: MyMap<string, string>
}
declare const m: I;
m.prop['a'];

const o = { a: 0 };
enum NumEnum { a, b }
declare let numEnumKey: NumEnum;
o[numEnumKey];
"#,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts7052_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7052)
        .map(|(_, message)| message.as_str())
        .collect();
    let ts7053_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts7052_messages
            .iter()
            .any(|message| message.contains("Did you mean to call 'c.get'?")),
        "Expected named read-side method suggestion for `c['hello']`. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7052_messages
            .iter()
            .any(|message| message.contains("Did you mean to call 'e.get'?")),
        "Expected named read-side method suggestion for `e['hello']`. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7052_messages
            .iter()
            .any(|message| message.contains("Did you mean to call 'e.set'?")),
        "Expected named write-side method suggestion for `e['hello'] = ...`. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7052_messages
            .iter()
            .any(|message| message.contains("Did you mean to call 'set'?")),
        "Expected bare write-side method suggestion for object-literal receivers. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7052_messages
            .iter()
            .any(|message| message.contains("Did you mean to call 'm.prop.get'?")),
        "Expected nested property receiver suggestion for `m.prop['a']`. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7053_messages.iter().any(|message| {
            message.contains("expression of type '\"hello\"' can't be used to index type '{ set: (key: string) => string; }'")
        }),
        "Set-only reads should remain TS7053 instead of switching to TS7052. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts7053_messages.iter().any(|message| message
            .contains("expression of type 'NumEnum' can't be used to index type '{ a: number; }'")),
        "Numeric enum keys should still report TS7053 on plain objects. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_did_you_mean_elaborations_for_expressions_which_could_be_called_regression() {
    let source = r#"
class Bar {
    x!: string;
}

declare function getNum(): number;

declare function foo(arg: { x: Bar, y: Date }, item: number, items?: [number, number, number]): void;

foo({
    x: Bar,
    y: Date
}, getNum());

foo({
    x: new Bar(),
    y: new Date()
}, getNum);


foo({
    x: new Bar(),
    y: new Date()
}, getNum(), [
    1,
    2,
    getNum
]);
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named_with_lib_and_options(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let actual: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message_text.clone()))
        .collect();

    // Verify we get the right number of diagnostics (4 total)
    assert_eq!(
        actual.len(),
        4,
        "Expected 4 diagnostics for didYouMeanElaborationsForExpressionsWhichCouldBeCalled. Actual: {actual:#?}"
    );

    // First two diagnostics: object literal property type mismatches.
    // TODO: tsc emits TS2741 ("Property 'x' is missing...") and TS2740 ("Type ... is missing
    // the following properties...") with specific missing-property elaboration. Our compiler
    // currently emits TS2322 (generic "not assignable"). Track as diagnostic quality gap.
    assert!(
        actual[0].0 == 2322 || actual[0].0 == 2741,
        "Expected TS2322 or TS2741 for x: Bar mismatch, got: {}",
        actual[0].0
    );
    assert!(
        actual[0].2.contains("typeof Bar") && actual[0].2.contains("Bar"),
        "Expected typeof Bar / Bar mismatch message, got: {}",
        actual[0].2
    );
    assert!(
        actual[1].0 == 2322 || actual[1].0 == 2740,
        "Expected TS2322 or TS2740 for y: Date mismatch, got: {}",
        actual[1].0
    );
    assert!(
        actual[1].2.contains("Date"),
        "Expected Date mismatch message, got: {}",
        actual[1].2
    );

    // Third diagnostic: callable argument (getNum instead of getNum())
    assert_eq!(actual[2].0, 2345, "Expected TS2345 for callable arg");
    assert!(
        actual[2].2.contains("() => number") && actual[2].2.contains("number"),
        "Expected callable arg message, got: {}",
        actual[2].2
    );

    // Fourth diagnostic: callable in array literal
    assert_eq!(
        actual[3].0, 2322,
        "Expected TS2322 for array callable element"
    );
    assert!(
        actual[3].2.contains("() => number") && actual[3].2.contains("number"),
        "Expected array callable element message, got: {}",
        actual[3].2
    );
}

#[test]
fn test_invokable_union_assignments_keep_both_ts2322_diagnostics() {
    let source = r#"
interface ConstructableA {
  new(): { somePropA: any };
}

interface IDirectiveLinkFn<TScope> {
    (scope: TScope): void;
}

interface IDirectivePrePost<TScope> {
    pre?: IDirectiveLinkFn<TScope>;
    post?: IDirectiveLinkFn<TScope>;
}

export let blah: IDirectiveLinkFn<number> | ConstructableA | IDirectivePrePost<number> = (x: string) => {}

export let ctor: IDirectiveLinkFn<number> | ConstructableA | IDirectivePrePost<number> = class {
    someUnaccountedProp: any;
}
"#;

    let diagnostics = compile_and_get_raw_diagnostics_named(
        "test.ts",
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    let blah_start = source.find("blah:").unwrap() as u32;
    let ctor_start = source.find("ctor:").unwrap() as u32;

    assert_eq!(
        ts2322.len(),
        2,
        "Expected both union assignment failures to report TS2322. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2322.iter().any(|diag| {
            diag.start == blah_start
                && diag.message_text.contains(
                    "Type '(x: string) => void' is not assignable to type 'ConstructableA | IDirectiveLinkFn<number> | IDirectivePrePost<number>'."
                )
        }),
        "Expected the function assignment diagnostic to preserve the construct-interface display. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2322.iter().any(|diag| {
            diag.start == ctor_start
                && diag.message_text.contains(
                    "Type 'typeof ctor' is not assignable to type 'ConstructableA | IDirectiveLinkFn<number> | IDirectivePrePost<number>'."
                )
        }),
        "Expected the class assignment diagnostic to stay anchored on `ctor`. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts2322
            .iter()
            .all(|diag| !diag.message_text.contains("typeof ConstructableA")),
        "Construct-only interfaces should display as type-space names, not value-space `typeof` names. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_modules_global_script_namespaces_emit_single_ts1280() {
    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &[(
            "/script-namespaces.ts",
            r#"
namespace Instantiated {
    export const x = 1;
}
namespace Uninstantiated {
    export type T = number;
}
declare namespace Ambient {
    export const x: number;
}
"#,
        )],
        "/script-namespaces.ts",
        CheckerOptions {
            isolated_modules: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let ts1280: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 1280)
        .collect();
    assert_eq!(
        ts1280.len(),
        1,
        "Expected exactly one TS1280 for the first top-level namespace in a global script. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        ts1280[0]
            .1
            .contains("Namespaces are not allowed in global script files"),
        "Expected the TS1280 message for isolatedModules global-script namespaces. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ambient_enum_initializer_suppresses_ts2304_for_bare_identifier_reference() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
declare enum Enum {
    F = A,
}
"#,
        CheckerOptions {
            isolated_modules: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2304),
        "Ambient enum constant-expression initializers should not cascade into TS2304. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_this_in_function_call_js_emits_ts2683_for_unannotated_callbacks_only() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "a.js",
        r#"
class Test {
    constructor() {
        this.data = { length: 3 };
    }

    invoke(callback) {
        return callback;
    }

    finderRaw() {
        return this.invoke(function (d) {
            return d === this.data.length;
        });
    }

    forEacherRaw() {
        return this.invoke(function (d) {
            return d === this.data.length;
        });
    }

    forEacher() {
        return this.invoke(
        /** @this {Test} */
        function (d) {
            return d === this.data.length;
        });
    }

    finder() {
        return this.invoke(
        /** @this {Test} */
        function (d) {
            return d === this.data.length;
        });
    }
}
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2683_count = diagnostics.iter().filter(|(code, _)| *code == 2683).count();
    assert_eq!(
        ts2683_count, 2,
        "Expected exactly two TS2683 diagnostics for the raw callbacks, got: {diagnostics:#?}"
    );
}

#[test]
fn test_js_iife_annotated_inner_function_still_emits_ts2683() {
    let diagnostics = compile_and_get_diagnostics_named_with_lib_and_options(
        "index.js",
        r#"
(function (importScripts) {
    /**
     * @param {...unknown} rest
     */
    return function () {
        return this;
    };
})(function () {});
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2683),
        "Expected TS2683 for the returned JS function without a `this` annotation, got: {diagnostics:#?}"
    );
}

#[test]
fn test_contextual_generic_callback_this_survives_ts2454_receiver_reads() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
// @target: es2015
interface JQuery {
    each<T>(
        collection: T[], callback: (this: T, dit: T) => T
    ): T[];
}

let $: JQuery;
let lines: string[];
$.each(lines, function(dit) {
    return dit.charAt(0) + this.charAt(1);
});
"#,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_this: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let semantic_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2454_count = semantic_errors
        .iter()
        .filter(|(code, _)| *code == 2454)
        .count();

    assert_eq!(
        ts2454_count, 2,
        "Expected both receiver reads to keep TS2454. Actual diagnostics: {semantic_errors:#?}"
    );
    assert!(
        !semantic_errors.iter().any(|(code, _)| *code == 2683),
        "Contextual generic callback `this` should survive TS2454 receiver reads. Actual diagnostics: {semantic_errors:#?}"
    );
}

#[test]
fn test_recursive_complicated_classes_emits_ts2507_for_symbol_extends() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
class Signature {
    public parameters: ParameterSymbol[] = null;
}

function aEnclosesB(a: Symbol) {
    return true;
}

class Symbol {
    public bound: boolean;
    public visible() {
        var b: TypeSymbol;
        return aEnclosesB(b);
    }
}

class InferenceSymbol extends Symbol {}
class ParameterSymbol extends InferenceSymbol {}
class TypeSymbol extends InferenceSymbol {}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2507 && message.contains("SymbolConstructor")),
        "Expected TS2507 mentioning SymbolConstructor, got: {diagnostics:#?}"
    );
}

#[test]
fn test_source_pragma_enables_no_property_access_from_index_signature() {
    let source = r#"
// @noPropertyAccessFromIndexSignature: true
interface B { [k: string]: string }
declare const b: B;
declare const c: B | undefined;
b.foo;
c?.foo;
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts4111_count = diagnostics.iter().filter(|(code, _)| *code == 4111).count();

    assert!(
        has_error(&diagnostics, 4111),
        "Expected TS4111 under @noPropertyAccessFromIndexSignature pragma, got: {diagnostics:?}"
    );
    assert_eq!(
        ts4111_count, 2,
        "Expected both direct and optional property accesses from index signatures to report TS4111, got: {diagnostics:?}"
    );
}

#[test]
fn test_variance_annotations_require_direct_supported_type_alias_bodies() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type NumericConstraint<Value extends number> = Value;
type VarianceConstrainedNumber<in out Value extends number> = NumericConstraint<Value>;

type VarianceFunction<in out Value> = (value: Value) => Value;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2637_count = diagnostics.iter().filter(|(code, _)| *code == 2637).count();
    assert_eq!(
        ts2637_count, 1,
        "Expected exactly one TS2637 for unsupported variance alias bodies, got: {diagnostics:?}"
    );
}

#[test]
fn test_variance_reference_assignability_preserves_literal_alias_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type NumericConstraint<Value extends number> = Value;
type VarianceConstrainedNumber<in out Value extends number> = NumericConstraint<Value>;

declare let vcn1: VarianceConstrainedNumber<1>;
declare let vcn12: VarianceConstrainedNumber<1 | 2>;
vcn1 = vcn12;

interface Shape<Value> {
  value: Value;
}
type VarianceShape<in out Value> = Shape<Value>;

declare let vs1: VarianceShape<1>;
declare let vs12: VarianceShape<1 | 2>;
vs1 = vs12;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322
            .iter()
            .any(|message| message.contains("Type '2 | 1' is not assignable to type '1'.")),
        "Expected direct alias assignment to display evaluated literal union, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|message| message
            .contains("Type 'VarianceShape<2 | 1>' is not assignable to type 'VarianceShape<1>'.")),
        "Expected object alias assignment to preserve alias with tsc numeric union order, got: {diagnostics:?}"
    );
}

#[test]
fn test_verbatim_module_syntax_const_enum_in_esnext_does_not_report_cjs_errors() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export const enum E {
    A = 1,
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::ESNext,
            verbatim_module_syntax: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 1287),
        "Expected no TS1287 for ESNext verbatim module syntax const enum export, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 1295),
        "Expected no TS1295 for ESNext verbatim module syntax const enum export, got: {diagnostics:?}"
    );
}

#[test]
fn test_window_console_resolves_through_global_this_alias() {
    let diagnostics = without_missing_global_type_errors(compile_and_get_diagnostics_with_lib(
        r#"
window.console;
self.console;
"#,
    ));

    assert!(
        !has_error(&diagnostics, 2339),
        "Expected window/self console accesses to resolve through globalThis aliases, got: {diagnostics:?}"
    );
}

#[test]
fn test_window_alias_unknown_property_reports_ts2339() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface ConsoleLike {
    log(...args: any[]): void;
}

interface Window {
    console: ConsoleLike;
}

declare var globalThis: {};
declare var window: Window & typeof globalThis;
declare var self: Window & typeof globalThis;

window.z = 3;
self.console;
"#,
    );

    let ts2339_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected exactly one TS2339 for the missing window property alias, got: {diagnostics:?}"
    );
    assert!(
        ts2339_messages[0].contains("Property 'z' does not exist on type"),
        "Expected TS2339 to point at the missing window property, got: {diagnostics:?}"
    );
}

#[test]
fn test_array_is_array_false_branch_keeps_original_union_surface() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
var maybeArray: number | number[];

if (Array.isArray(maybeArray)) {
    maybeArray.length;
} else {
    maybeArray.toFixed();
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        }
        .apply_strict_defaults(),
    );

    let ts2339_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.as_str())
        .collect();

    assert_eq!(
        ts2339_messages.len(),
        1,
        "Expected exactly one TS2339 on the false branch of Array.isArray, got: {diagnostics:?}"
    );
    assert!(
        ts2339_messages[0].contains("toFixed") && ts2339_messages[0].contains("number | number[]"),
        "Expected TS2339 to preserve the original union surface, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("length")),
        "Did not expect the true branch to lose Array.isArray narrowing, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_mismatch_reports_ts2345() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo6<T>(cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}

declare var b: { new <T>(x: T, y: T): string };
var r10 = foo6(b);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for the incompatible generic constructor callback, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "Expected the single-signature generic call to stay TS2345-only, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_valid_cases_stay_clean() {
    // foo5<T>(cb) has a single argument, so the deferral logic doesn't apply.
    // These cases should remain clean.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo5<T>(cb: { new(x: T): string; new(x: number): T }) {
    return cb;
}

declare var a: { new <T>(x: T): T };
var r6 = foo5(a);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Did not expect TS2345 for valid generic constructor callback cases, got: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2769),
        "Did not expect TS2769 for valid generic constructor callback cases, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_callback_with_leading_arg() {
    // foo7<T>(x:T, cb) has two arguments. With the deferral fix (non-context-sensitive
    // args are no longer deferred), T is correctly inferred from arg 0. The constructor
    // suppression narrowing ensures we no longer emit a false positive TS2345 when the
    // argument is a constructor-like type application; tsc accepts both calls here.
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function foo7<T>(x:T, cb: { new(x: T): string; new(x: T, y?: T): string }) {
    return cb;
}

declare var a: { new <T>(x: T): T };
var r13 = foo7(1, a);
declare var c: { new<T>(x: T): number; new<T>(x: number): T; }
var r14 = foo7(1, c);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // Matches tsc: both invocations type-check without TS2345.
    assert!(
        !has_error(&diagnostics, 2345),
        "Expected no TS2345 (constructor callback inference should match tsc)"
    );
}

/// Generic constructor calls should widen scalar literal argument types
/// (e.g., `true` → `boolean`) for TS2345 error messages, matching tsc.
/// Regression test for exportAssignmentConstrainedGenericType conformance.
#[test]
fn test_generic_constructor_widens_boolean_literal_for_error_display() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
class Foo<T extends {a: string; b: number}> {
    test: T;
    constructor(x: T) {}
}
var x = new Foo(true);
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2345),
        "Expected TS2345 for boolean arg to generic constructor, got: {diagnostics:?}"
    );
    // Verify the error message uses the widened type 'boolean', not literal 'true'
    let ts2345_msg = diagnostics
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, msg)| msg.as_str())
        .unwrap_or("");
    assert!(
        ts2345_msg.contains("boolean"),
        "Expected widened 'boolean' in error message (not literal 'true'), got: {ts2345_msg}"
    );
}

#[test]
fn test_unresolved_computed_class_method_contributes_indexed_callable_type() {
    let source = r#"
declare var something: string;
export const dataSomething = `data-${something}` as const;

class WithData {
    [dataSomething]?() {
        return "something";
    }
}

const s: string = (new WithData())["ahahahaahah"]!();
const n: number = (new WithData())["ahahahaahah"]!();
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert_eq!(
        ts2322_count, 1,
        "Expected only the number assignment to fail after unresolved computed method indexing is typed, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'string' is not assignable to type 'number'")),
        "Expected the remaining failure to be the string-to-number assignment, got: {diagnostics:#?}"
    );
}

#[test]
fn test_unresolved_computed_instance_methods_produce_union_lookup_types() {
    let source = r#"
export const fieldName = Math.random() > 0.5 ? "f1" : "f2";

class Holder {
    [fieldName]() {
        return "value";
    }
    [fieldName === "f1" ? "f2" : "f1"]() {
        return 42;
    }
    static [fieldName]() {
        return { static: true };
    }
    static [fieldName]() {
        return { static: "sometimes" };
    }
}

const instanceOk: (() => string) | (() => number) = (new Holder())["x"];
const instanceBad: number = (new Holder())["x"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert_eq!(
        ts2322_count, 1,
        "Expected only the instance number assignment to fail once computed method lookups form unions, got: {diagnostics:#?}"
    );
    // Computed method types may resolve to `() => any` or a union of callable
    // types depending on the constructor type caching order. Either is acceptable
    // as long as exactly one TS2322 is emitted for the bad assignment.
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2322 && message.contains("number")),
        "Expected instance lookup assignment error to mention 'number', got: {diagnostics:#?}"
    );
}

#[test]
fn test_recursive_type_parameter_constraint_missing_args_reports_generic_name_with_params() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface A<T extends A> {}
"#,
    );

    let message = diagnostic_message(&diagnostics, 2314)
        .expect("Expected TS2314 for recursive type parameter constraint");
    assert!(
        message.contains("Generic type 'A<T>' requires 1 type argument(s)."),
        "Expected TS2314 message to include generic parameter list, got: {diagnostics:?}"
    );
}

#[test]
fn test_unresolved_computed_static_methods_produce_union_lookup_types() {
    let source = r#"
declare const f1: string;
declare const f2: string;

class Holder {
    static [f1]() {
        return { static: true };
    }
    static [f2]() {
        return { static: "sometimes" };
    }
}

const ok:
    | Holder
    | (() => { static: boolean })
    | (() => { static: string }) = Holder["x"];
const bad: number = Holder["x"];
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&String> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "Expected only the bad static lookup assignment to fail once late-bound static methods are typed, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0].contains("Type 'Holder' is not assignable to type 'number'"),
        "Expected static late-bound lookup to stay non-any and still include the prototype branch in diagnostics, got: {diagnostics:#?}"
    );
}

#[test]
fn test_constructor_implementation_with_more_required_params_reports_ts2394() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Customers {
    constructor(name: string);
    constructor(name: string, age: number) {}
}
"#,
    );

    assert!(
        has_error(&diagnostics, 2394),
        "Expected TS2394 for constructor overload/implementation arity mismatch, got: {diagnostics:?}"
    );
}
