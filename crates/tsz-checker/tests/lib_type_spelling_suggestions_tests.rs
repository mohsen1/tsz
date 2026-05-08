//! Issue #3282: TYPE-position lookups must surface spelling suggestions
//! for core lib globals (`Array`, `Promise`, `Map`, ...). tsz used to
//! suppress every lib-origin candidate for TYPE-only lookups, so typos
//! like `Arrray`, `Prommise`, `Mapp` reported plain TS2304 instead of
//! tsc's TS2552 with a "Did you mean 'Array'?" suggestion.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_es2015_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.collection.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.core.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.iterable.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.promise.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.symbol.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.symbol.wellknown.d.ts"),
    ];
    let mut out = Vec::new();
    for path in &candidates {
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(path)
        {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            out.push(Arc::new(LibFile::from_source(name, content)));
        }
    }
    out
}

fn check_with_es2015(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_es2015_lib_files();
    assert!(
        !lib_files.is_empty(),
        "expected lib.es*.d.ts to be available"
    );

    let mut parser = ParserState::new("repro.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "repro.ts".to_string(),
        CheckerOptions::default(),
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn finds_suggestion(diags: &[Diagnostic], typo: &str, suggestion: &str) -> bool {
    diags.iter().any(|d| {
        d.code == 2552
            && d.message_text.contains(&format!("'{typo}'"))
            && d.message_text.contains(&format!("'{suggestion}'"))
    })
}

/// `Arrray` should suggest `Array` from the core lib.
#[test]
fn type_position_arrray_suggests_array() {
    let diags = check_with_es2015("let a: Arrray;\n");
    assert!(
        finds_suggestion(&diags, "Arrray", "Array"),
        "expected TS2552 'Arrray' -> 'Array', got: {diags:?}"
    );
}

/// `Prommise` should suggest `Promise`.
#[test]
fn type_position_prommise_suggests_promise() {
    let diags = check_with_es2015("let p: Prommise<string>;\n");
    assert!(
        finds_suggestion(&diags, "Prommise", "Promise"),
        "expected TS2552 'Prommise' -> 'Promise', got: {diags:?}"
    );
}

/// `Mapp` should suggest `Map`.
#[test]
fn type_position_mapp_suggests_map() {
    let diags = check_with_es2015("let m: Mapp<string, number>;\n");
    assert!(
        finds_suggestion(&diags, "Mapp", "Map"),
        "expected TS2552 'Mapp' -> 'Map', got: {diags:?}"
    );
}
