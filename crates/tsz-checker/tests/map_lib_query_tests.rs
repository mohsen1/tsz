use std::path::Path;
use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es2015.d.ts"),
    ];

    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            lib_files.push(Arc::new(LibFile::from_source(file_name, content)));
        }
    }
    lib_files
}

#[test]
fn map_method_access_no_error_with_default_libs() {
    let lib_files = load_lib_files_for_test();
    if lib_files.is_empty() {
        return;
    }

    let source =
        "const store = new Map<string, number>();\nstore.set(\"x\", 1);\nstore.get(\"x\");";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(root);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        relevant.is_empty(),
        "Map method access should not emit diagnostics with lib.d.ts loaded, got: {relevant:?}"
    );
}
