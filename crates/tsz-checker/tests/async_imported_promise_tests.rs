use std::sync::Arc;

use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::TypeId;
use tsz_solver::construction::TypeInterner;

use crate::context::{CheckerOptions, LibContext as CheckerLibContext};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;

fn load_lib_files(names: &[&str]) -> Vec<Arc<LibFile>> {
    crate::test_utils::load_compiled_lib_files(names)
}

fn parse_and_bind(
    name: &str,
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    NodeIndex,
) {
    let mut parser = ParserState::new(name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (Arc::new(parser.get_arena().clone()), Arc::new(binder), root)
}

fn parse_and_bind_with_libs(
    name: &str,
    source: &str,
    lib_files: &[Arc<LibFile>],
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    NodeIndex,
) {
    let mut parser = ParserState::new(name.to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    (Arc::new(parser.get_arena().clone()), Arc::new(binder), root)
}

/// A user-declared `Promise` class in another file is NOT the lib Promise.
/// `promise_like_type_argument_from_class` must not unwrap through it even when
/// the name matches "Promise" — the identity check rejects non-lib symbols.
///
/// This test also verifies that the cross-file arena lookup path (accessing the
/// declaring file's binder and arena from the current-file checker) is reachable
/// without panicking.
#[test]
fn user_declared_promise_shadow_cross_file_returns_none() {
    let (task_arena, task_binder, _) = parse_and_bind(
        "./task.ts",
        r#"
declare class Promise<T> { }
export class Task<T> extends Promise<T> { }
"#,
    );
    let task_sym = task_binder
        .file_locals
        .get("Task")
        .expect("Task should be bound in task.ts");

    let (test_arena, test_binder, _) = parse_and_bind("./test.ts", "export {};");
    let all_arenas = Arc::new(vec![task_arena, test_arena]);
    let all_binders = Arc::new(vec![task_binder, test_binder]);
    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "./test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.register_symbol_file_target(task_sym, 0);

    // Without lib loaded, the user-declared `Promise` in task.ts is not the lib
    // Promise. The identity check must reject it → None.
    let result = checker.promise_like_type_argument_from_class(
        task_sym,
        &[TypeId::STRING],
        &mut AliasCycleTracker::new(),
    );
    assert!(
        result.is_none(),
        "User-declared Promise shadow should not be unwrapped; got: {result:?}"
    );
}

/// A same-file user declaration named `Promise` must not become the standard
/// library Promise identity. The lib lookup used by `sym_id_is_lib_promise`
/// must skip current-file `file_locals`, otherwise a shadowed Promise class is
/// incorrectly treated as the lib Promise.
#[test]
fn same_file_user_promise_shadow_is_not_lib_promise_identity() {
    let lib_files = load_lib_files(&["lib.es2015.promise.d.ts", "lib.es5.d.ts"]);
    if lib_files.is_empty() {
        return;
    }

    let (arena, binder, _) = parse_and_bind_with_libs(
        "./shadow.ts",
        r#"
declare class Promise<T> { }
export class Task<T> extends Promise<T> { }
"#,
        &lib_files,
    );
    let promise_sym = binder
        .file_locals
        .get("Promise")
        .expect("same-file Promise shadow should be bound");
    let task_sym = binder
        .file_locals
        .get("Task")
        .expect("Task should be bound");
    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "./shadow.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let lib_count = lib_contexts.len();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_count);

    assert!(
        !checker.ctx.sym_id_is_lib_promise(promise_sym),
        "same-file Promise shadow must not be treated as the standard-library Promise"
    );

    let result = checker.promise_like_type_argument_from_class(
        task_sym,
        &[TypeId::STRING],
        &mut AliasCycleTracker::new(),
    );
    assert!(
        result.is_none(),
        "Task extends a same-file Promise shadow, not lib Promise; got: {result:?}"
    );
}

/// Structural rule: when a class in a *different* file extends the lib `Promise<T>`,
/// `promise_like_type_argument_from_class` must traverse the cross-file heritage
/// clause and return the resolved type argument `T`.
///
/// This covers the case where the string name "Promise" in the declaring file's
/// `file_locals` is absent (no local shadow) so we fall back to `has_name_in_lib`.
#[test]
fn lib_promise_subclass_cross_file_unwraps_type_arg() {
    let lib_files = load_lib_files(&["lib.es2015.promise.d.ts", "lib.es5.d.ts"]);
    if lib_files.is_empty() {
        // Lib files not available in this environment; skip.
        return;
    }

    // Deferred<T> extends Promise<T> — uses the global (lib) Promise.
    let (task_arena, task_binder, _) = parse_and_bind_with_libs(
        "./deferred.ts",
        r#"
export class Deferred<T> extends Promise<T> {
    constructor() { super(() => {}); }
}
"#,
        &lib_files,
    );
    let deferred_sym = task_binder
        .file_locals
        .get("Deferred")
        .expect("Deferred should be bound");

    let (test_arena, test_binder, _) =
        parse_and_bind_with_libs("./test.ts", "export {};", &lib_files);
    let all_arenas = Arc::new(vec![task_arena, test_arena]);
    let all_binders = Arc::new(vec![task_binder, test_binder]);
    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "./test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.register_symbol_file_target(deferred_sym, 0);

    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let lib_count = lib_contexts.len();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_count);

    let inner = checker
        .promise_like_type_argument_from_class(
            deferred_sym,
            &[TypeId::STRING],
            &mut AliasCycleTracker::new(),
        )
        .expect("Deferred<string> should unwrap through extends Promise<T>");

    assert_eq!(inner, TypeId::STRING);
}

/// Renamed type parameter: `class Future<U> extends Promise<U>` must work
/// identically to `class Deferred<T> extends Promise<T>` — the fix is keyed
/// on symbol identity, not on the type-parameter spelling.
#[test]
fn lib_promise_subclass_renamed_type_param_unwraps_correctly() {
    let lib_files = load_lib_files(&["lib.es2015.promise.d.ts", "lib.es5.d.ts"]);
    if lib_files.is_empty() {
        return;
    }

    let (task_arena, task_binder, _) = parse_and_bind_with_libs(
        "./future.ts",
        r#"
export class Future<U> extends Promise<U> {
    constructor() { super(() => {}); }
}
"#,
        &lib_files,
    );
    let future_sym = task_binder
        .file_locals
        .get("Future")
        .expect("Future should be bound");

    let (test_arena, test_binder, _) =
        parse_and_bind_with_libs("./test.ts", "export {};", &lib_files);
    let all_arenas = Arc::new(vec![task_arena, test_arena]);
    let all_binders = Arc::new(vec![task_binder, test_binder]);
    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        all_arenas[1].as_ref(),
        all_binders[1].as_ref(),
        &types,
        "./test.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.register_symbol_file_target(future_sym, 0);

    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    let lib_count = lib_contexts.len();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_count);

    let inner = checker
        .promise_like_type_argument_from_class(
            future_sym,
            &[TypeId::NUMBER],
            &mut AliasCycleTracker::new(),
        )
        .expect("Future<number> should unwrap through extends Promise<U>");

    assert_eq!(inner, TypeId::NUMBER);
}
