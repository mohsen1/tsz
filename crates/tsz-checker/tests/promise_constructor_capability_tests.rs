use std::path::Path;
use std::sync::Arc;

use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut loaded = Vec::new();

    for name in names {
        let candidates = [
            manifest_dir.join(format!("../../scripts/node_modules/typescript/lib/{name}")),
            manifest_dir.join(format!(
                "../../scripts/conformance/node_modules/typescript/lib/{name}"
            )),
            manifest_dir.join(format!("../../TypeScript/lib/{name}")),
        ];

        for path in candidates {
            if path.exists()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                loaded.push(Arc::new(LibFile::from_source((*name).to_string(), content)));
                break;
            }
        }
    }

    loaded
}

fn check_with_libs(source: &str, lib_names: &[&str]) -> Vec<Diagnostic> {
    let lib_files = load_lib_files(lib_names);

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn es2015_target_with_es5_lib_still_reports_missing_promise_constructor() {
    let diagnostics = check_with_libs(
        r#"
const loadAsync = async () => {
    await import("./dep");
};
"#,
        &["lib.es5.d.ts"],
    );

    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2705),
        "Expected TS2705 for async function with ES5-only libs, got: {diagnostics:#?}"
    );
    assert!(
        codes.contains(&2712),
        "Expected TS2712 for dynamic import with ES5-only libs, got: {diagnostics:#?}"
    );
}

#[test]
fn reports_ts2712_for_each_dynamic_import_site_in_conformance_shape() {
    let source = r#"
declare var console: any;
class C {
    private myModule = import("./0");
    method() {
        const loadAsync = import("./0");
        this.myModule.then(Zero => {
            console.log(Zero.foo());
        }, async err => {
            console.log(err.message);
        });
        const loadAsync2 = import("./0");
        const loadAsync3 = import("./0");
    }
}
"#;

    let diagnostics = check_with_libs(source, &["lib.es5.d.ts"]);
    let ts2712_count = diagnostics.iter().filter(|d| d.code == 2712).count();

    assert_eq!(
        ts2712_count, 4,
        "Expected 4 TS2712 errors (one per import site), got: {ts2712_count}",
    );
}

#[test]
fn preserves_type_parameter_from_custom_promise_like_type() {
    // Test that we preserve type parameters from custom Promise-like types
    // even when complex Promise unwrapping fails.
    // This tests the fix for: async example<T>(): Task<T> { return; }
    // where Task<T> extends Promise<T>
    let diagnostics = check_with_libs(
        r#"
class Task<T> extends Promise<T> { }

class Test {
    async example<T>(): Task<T> { return; }
}
"#,
        &["lib.es2015.full.d.ts"],
    );

    // We expect TS2322 for bare return statement
    // The key is that we check against the unwrapped type parameter 'T',
    // not the full Task<T> type.
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for bare return with custom Promise type, got: {diagnostics:#?}"
    );
}
