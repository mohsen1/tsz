use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::{CheckerOptions, LibContext as CheckerLibContext, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{load_compiled_lib_files, load_lib_files};
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::construction::{TypeDatabase, TypeInterner};

fn parse_test_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn load_array_to_locale_libs() -> Vec<Arc<LibFile>> {
    load_lib_files(&["es5.d.ts", "es2015.core.d.ts"])
}

fn load_array_es2020_libs() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&[
        "lib.es2020.full.d.ts",
        "lib.es2020.d.ts",
        "lib.dom.d.ts",
        "lib.webworker.importscripts.d.ts",
        "lib.scripthost.d.ts",
        "lib.dom.iterable.d.ts",
        "lib.dom.asynciterable.d.ts",
        "lib.es2019.d.ts",
        "lib.es2020.bigint.d.ts",
        "lib.es2020.date.d.ts",
        "lib.es2020.number.d.ts",
        "lib.es2020.promise.d.ts",
        "lib.es2020.sharedmemory.d.ts",
        "lib.es2020.string.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2020.intl.d.ts",
        "lib.es2015.d.ts",
        "lib.es2018.asynciterable.d.ts",
        "lib.es2018.d.ts",
        "lib.es2019.array.d.ts",
        "lib.es2019.object.d.ts",
        "lib.es2019.string.d.ts",
        "lib.es2019.symbol.d.ts",
        "lib.es2019.intl.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2015.symbol.d.ts",
        "lib.es2018.intl.d.ts",
        "lib.es5.d.ts",
        "lib.es2015.core.d.ts",
        "lib.es2015.collection.d.ts",
        "lib.es2015.generator.d.ts",
        "lib.es2015.promise.d.ts",
        "lib.es2015.proxy.d.ts",
        "lib.es2015.reflect.d.ts",
        "lib.es2015.symbol.wellknown.d.ts",
        "lib.es2017.d.ts",
        "lib.es2018.asyncgenerator.d.ts",
        "lib.es2018.promise.d.ts",
        "lib.es2018.regexp.d.ts",
        "lib.decorators.d.ts",
        "lib.decorators.legacy.d.ts",
        "lib.es2016.d.ts",
        "lib.es2017.arraybuffer.d.ts",
        "lib.es2017.date.d.ts",
        "lib.es2017.intl.d.ts",
        "lib.es2017.object.d.ts",
        "lib.es2017.sharedmemory.d.ts",
        "lib.es2017.string.d.ts",
        "lib.es2017.typedarrays.d.ts",
        "lib.es2016.array.include.d.ts",
        "lib.es2016.intl.d.ts",
    ])
}

fn prime_array_display_props(lib_files: Vec<Arc<LibFile>>) -> Vec<String> {
    let (parser, root) = parse_test_source("const marker = 1;");
    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2020,
            emit_declarations: true,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.prime_boxed_types();

    let array_base =
        TypeDatabase::get_array_base_type(checker.ctx.types).expect("expected Array<T> base");
    checker
        .ctx
        .types
        .get_display_properties(array_base)
        .expect("expected Array<T> display properties")
        .iter()
        .map(|prop| checker.ctx.types.resolve_atom_ref(prop.name).to_string())
        .collect()
}

#[test]
fn declaration_array_display_summary_merges_to_locale_string_overloads() {
    let lib_files = load_array_to_locale_libs();
    if lib_files.is_empty() {
        return;
    }

    let (parser, root) = parse_test_source("const marker = 1;");
    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2020,
            emit_declarations: true,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.prime_boxed_types();

    let array_base =
        TypeDatabase::get_array_base_type(checker.ctx.types).expect("expected Array<T> base");
    let display_props = checker
        .ctx
        .types
        .get_display_properties(array_base)
        .expect("expected Array<T> display properties");
    let to_locale = display_props
        .iter()
        .find(|prop| checker.ctx.types.resolve_atom_ref(prop.name).as_ref() == "toLocaleString")
        .expect("expected Array<T> display summary to include toLocaleString");
    let shape = tsz_solver::type_queries::get_callable_shape(checker.ctx.types, to_locale.type_id)
        .expect("expected toLocaleString display type to be callable");

    assert_eq!(
        shape.call_signatures.len(),
        2,
        "expected Array<T>.toLocaleString display summary to merge ES5 and ES2015 overloads"
    );
    assert_eq!(shape.call_signatures[0].params.len(), 0);
    assert_eq!(shape.call_signatures[1].params.len(), 2);
}

#[test]
fn declaration_array_display_summary_keeps_late_es_array_members() {
    let lib_files = load_array_es2020_libs();
    if lib_files.is_empty() {
        return;
    }

    let prop_names = prime_array_display_props(lib_files);
    for expected in [
        "includes",
        "flatMap",
        "flat",
        "[Symbol.iterator]",
        "[Symbol.unscopables]",
    ] {
        assert!(
            prop_names.iter().any(|name| name == expected),
            "expected Array<T> display summary to include {expected}; got {prop_names:#?}"
        );
    }
}

#[test]
fn declaration_array_display_summary_snapshots_well_known_symbol_names() {
    let lib_files = load_array_es2020_libs();
    if lib_files.is_empty() {
        return;
    }

    let (parser, root) = parse_test_source("const marker = 1;");
    let mut binder = BinderState::new();
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let raw_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&raw_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2020,
            emit_declarations: true,
            ..Default::default()
        },
    );
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.prime_boxed_types();

    let array_base =
        TypeDatabase::get_array_base_type(checker.ctx.types).expect("expected Array<T> base");
    let prop_names: Vec<_> = checker
        .ctx
        .types
        .get_display_properties(array_base)
        .expect("expected Array<T> display properties")
        .iter()
        .map(|prop| checker.ctx.types.resolve_atom_ref(prop.name).to_string())
        .collect();
    let cache = checker.extract_cache();
    for expected in ["[Symbol.iterator]", "[Symbol.unscopables]"] {
        assert!(
            cache.well_known_symbol_names.contains_key(expected),
            "expected declaration cache to snapshot {expected}; props={prop_names:#?}; cache={:#?}",
            cache.well_known_symbol_names.keys().collect::<Vec<_>>()
        );
    }
}
