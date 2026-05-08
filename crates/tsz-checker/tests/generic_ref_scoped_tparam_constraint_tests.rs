//! Issue #3063: a generic application like `Array<U>`, `Promise<U>`,
//! `Record<string, U>` used as the type argument to a constraint
//! `T extends string` (or any primitive) must report TS2344 even
//! though the inner `U` is a scoped type parameter. tsz used to skip
//! the constraint check whenever the arg held a scoped type param,
//! which silently accepted these object-vs-primitive mismatches.

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

fn check(source: &str) -> Vec<Diagnostic> {
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

fn ts2344_messages(diags: &[Diagnostic]) -> Vec<&str> {
    diags
        .iter()
        .filter(|d| d.code == 2344)
        .map(|d| d.message_text.as_str())
        .collect()
}

/// `type BadArray<U> = Box<Array<U>>` — `Array<U>` cannot satisfy
/// `T extends string`, regardless of the value of `U`. Required by
/// the issue's repro.
#[test]
fn generic_array_arg_fails_primitive_string_constraint() {
    let diags = check(
        r#"
type Box<T extends string> = T;
type BadArray<U> = Box<Array<U>>;
"#,
    );
    let msgs = ts2344_messages(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("does not satisfy")
            && (m.contains("Array<U>") || m.contains("U[]"))
            && m.contains("'string'")),
        "expected TS2344 for Array<U> vs string, got: {diags:?}"
    );
}

/// `type BadPromise<U> = Box<Promise<U>>` — same rule via `Promise<U>`.
#[test]
fn generic_promise_arg_fails_primitive_string_constraint() {
    let diags = check(
        r#"
type Box<T extends string> = T;
type BadPromise<U> = Box<Promise<U>>;
"#,
    );
    let msgs = ts2344_messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Promise<U>") && m.contains("'string'")),
        "expected TS2344 for Promise<U> vs string, got: {diags:?}"
    );
}

/// `type BadRecord<U> = Box<Record<string, U>>` — same rule via
/// `Record<string, U>`.
#[test]
fn generic_record_arg_fails_primitive_string_constraint() {
    let diags = check(
        r#"
type Box<T extends string> = T;
type BadRecord<U> = Box<Record<string, U>>;
"#,
    );
    let msgs = ts2344_messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Record<string, U>") && m.contains("'string'")),
        "expected TS2344 for Record<string, U> vs string, got: {diags:?}"
    );
}

/// Sanity: when the constraint is itself a generic type (not a
/// primitive), the existing deferral must still apply — instantiation
/// time can prove satisfaction.
#[test]
fn generic_constraint_keeps_deferral_for_scoped_arg() {
    let diags = check(
        r#"
type Wrap<T> = { value: T };
type Box<T extends Wrap<unknown>> = T;
type Use<U> = Box<Wrap<U>>;
"#,
    );
    let ts2344 = ts2344_messages(&diags);
    assert!(
        ts2344.is_empty(),
        "did not expect TS2344 for Wrap<U> vs Wrap<unknown> (generic constraint, deferred), got: {diags:?}"
    );
}
