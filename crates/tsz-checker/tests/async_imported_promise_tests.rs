use std::sync::Arc;

use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::{TypeId, TypeInterner};

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

#[test]
fn promise_subclass_heritage_unwrap_uses_declaring_file_arena() {
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

    let inner = checker
        .promise_like_type_argument_from_class(
            task_sym,
            &[TypeId::STRING],
            &mut AliasCycleTracker::new(),
        )
        .expect("Task<string> should unwrap through extends Promise<T>");

    assert_eq!(inner, TypeId::STRING);
}
