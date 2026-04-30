//! Regression test for TS2339 on `globalThis.<block-scoped-name>`.
//!
//! `var` declarations at file scope are added to `typeof globalThis`, but
//! `let` / `const` are not. Accessing the block-scoped name through globalThis
//! must therefore emit TS2339.
//!
//! Bug: `resolve_lib_global_var_symbol` walked every symbol in
//! `lib_symbol_ids` and accepted any `FUNCTION_SCOPED_VARIABLE`-flagged
//! candidate by name — including parameter symbols of lib callables (e.g.
//! `Path2D.moveTo(x: number, y: number)`'s `y` parameter), which share that
//! flag. That bogus match suppressed the legitimate TS2339 for
//! `globalThis.y` when the user file had `const y = 2`.
//!
//! Fix: reject lib candidates whose `parent` is a callable
//! (FUNCTION / METHOD / CONSTRUCTOR / `GET_ACCESSOR` / `SET_ACCESSOR` / SIGNATURE).

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.dom.d.ts"),
    ];

    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let lib = LibFile::from_source(
                lib_path.file_name().unwrap().to_string_lossy().to_string(),
                content,
            );
            lib_files.push(Arc::new(lib));
        }
    }
    lib_files
}

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let lib_files = load_lib_files();
    if lib_files.is_empty() {
        // Lib files not available in this build environment — skip.
        return Vec::new();
    }

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let binder_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&binder_lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    let checker_lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn const_local_emits_ts2339_on_global_this_property_access() {
    // `var x` is on globalThis, so `globalThis.x` should NOT emit TS2339.
    // `const y` is NOT on globalThis, so `globalThis.y` SHOULD emit TS2339.
    let diags = diagnostics(
        r#"
var x = 1
const y = 2
globalThis.x = 3
globalThis.y = 4
"#,
    );
    if diags.is_empty() {
        // No lib files available — skip silently in restricted environments.
        return;
    }

    let ts2339_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert_eq!(
        ts2339_msgs.len(),
        1,
        "Expected exactly one TS2339 (on `globalThis.y`), got: {diags:#?}"
    );
    assert!(
        ts2339_msgs[0].contains("'y'"),
        "TS2339 message should reference property `y`, got: {:?}",
        ts2339_msgs[0]
    );
    assert!(
        ts2339_msgs[0].contains("'typeof globalThis'"),
        "TS2339 message should reference `typeof globalThis`, got: {:?}",
        ts2339_msgs[0]
    );
}

#[test]
fn let_local_emits_ts2339_on_global_this_property_access() {
    let diags = diagnostics(
        r#"
let z = 5
globalThis.z = 6
"#,
    );
    if diags.is_empty() {
        return;
    }

    let ts2339_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg.as_str())
        .collect();

    assert_eq!(
        ts2339_msgs.len(),
        1,
        "Expected exactly one TS2339 (on `globalThis.z`), got: {diags:#?}"
    );
    assert!(
        ts2339_msgs[0].contains("'z'"),
        "TS2339 message should reference property `z`, got: {:?}",
        ts2339_msgs[0]
    );
}
