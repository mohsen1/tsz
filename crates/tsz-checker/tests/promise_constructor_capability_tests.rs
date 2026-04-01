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

fn line_col_for_offset(source: &str, offset: u32) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx as u32 >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
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

    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(&2468),
        "Expected TS2468 when Promise constructor is unavailable, got: {diagnostics:#?}"
    );
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
#[ignore] // TODO: dynamic import inside async arrow missing TS2712
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
            console.log(err);
            let one = await import("./1");
            console.log(one.backup());
        });
    }
}
"#;

    let diagnostics = check_with_libs(source, &["lib.es5.d.ts"]);
    let ts2712_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2712)
        .map(|diag| line_col_for_offset(source, diag.start))
        .collect();

    assert_eq!(
        ts2712_positions,
        vec![(4, 24), (6, 27), (11, 29)],
        "Expected TS2712 at all dynamic import sites, got diagnostics: {diagnostics:#?}"
    );
}
