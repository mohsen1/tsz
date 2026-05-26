use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::{CheckerOptions, LibContext as CheckerLibContext, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::load_lib_files;
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
