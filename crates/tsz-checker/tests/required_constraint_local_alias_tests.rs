//! Issue #3061: a user-defined `type Required<T> = ...` must NOT
//! receive the lib-`Required<T>` mapped-utility shortcut. The shortcut
//! returned the source `T` as the constraint check target, so any type
//! satisfied a local `Required<Source>` constraint trivially. The
//! actual constraint check should run, which surfaces TS2344 when the
//! arg's shape doesn't match the user's alias body.

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

fn load_es5_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.collection.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.core.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.iterable.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.symbol.d.ts"),
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
    let lib_files = load_es5_lib_files();
    assert!(!lib_files.is_empty());

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
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "repro.ts".to_string(),
        options,
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let n = lib_files.len();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(n);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Local `type Required<T> = { marker: string }` is unrelated to the
/// lib's `Required<T>` mapped utility. The lib-`Required<T>` shortcut
/// must not silently accept `Box<Source>` — either the local alias is
/// reported (the issue's TS2344 surface) or the redeclaration is
/// rejected (TS2300 against the lib alias). The defect this test
/// guards against is the *silent* acceptance produced by the
/// name-only shortcut.
#[test]
fn local_required_alias_constraint_is_not_silently_accepted() {
    let source = r#"
type Required<T> = { marker: string };

interface Source {
  a: number;
}

type Box<T extends Required<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let saw_constraint_or_redecl = diags.iter().any(|d| d.code == 2344 || d.code == 2300);
    assert!(
        saw_constraint_or_redecl,
        "expected TS2344 (constraint failure) or TS2300 (lib redeclaration) for local Required<T>, got: {diags:?}"
    );
}

/// Sanity: a local alias unrelated to `Required` must keep the normal
/// constraint check unchanged.
#[test]
fn local_marker_alias_constraint_emits_ts2344() {
    let source = r#"
type Marker<T> = { marker: string };

interface Source {
  a: number;
}

type Box<T extends Marker<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let ts2344: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "expected TS2344 for unrelated local alias, got: {diags:?}"
    );
}

/// Anchor: the genuine `Required<T>` from lib still benefits from the
/// shortcut — `Box<Source>` where `Source` has the required field
/// satisfies its own `Required<Source>` constraint, no TS2344.
#[test]
fn lib_required_constraint_with_satisfying_arg_emits_no_ts2344() {
    let source = r#"
interface Source {
  a: number;
}

type Box<T extends Required<Source>> = T;
type Use = Box<Source>;
"#;
    let diags = check_with_es2015(source);
    let ts2344: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2344).collect();
    assert!(
        ts2344.is_empty(),
        "lib Required<T> shortcut must keep accepting matching arg, got: {diags:?}"
    );
}
