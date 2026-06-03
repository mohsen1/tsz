use super::cross_file_direct_alias_chain::SourceFileAliasProofContext;
use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

fn parse_bound_source(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

fn with_two_file_state<F, R>(target_source: &str, requester_source: &str, test: F) -> R
where
    F: FnOnce(&mut CheckerState<'_>, &Arc<BinderState>) -> R,
{
    let (target_arena, target_binder, types) = parse_bound_source(target_source);
    let (requester_arena, requester_binder, _) = parse_bound_source(requester_source);
    let ctx = CheckerContext::new(
        requester_arena.as_ref(),
        requester_binder.as_ref(),
        &types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    state.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&requester_arena),
        Arc::clone(&target_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&target_binder),
    ]));
    test(&mut state, &target_binder)
}

#[test]
fn direct_source_file_type_alias_lowers_concrete_local_conditional_alias_chain() {
    with_two_file_state(
        "type Slots = { yes: string; no: number };\ntype PickYes = 'x' extends string ? Slots['yes'] : Slots['no'];\nexport type Result = PickYes;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("concrete conditional aliases should lower through local alias chains");

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Result should stay non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_concrete_conditional_infer_branch() {
    with_two_file_state(
        "type TupleItem = readonly ['value'] extends readonly [infer Chosen] ? Chosen : never;\nexport type Result = TupleItem;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect(
                    "concrete conditional infer aliases should lower through local alias chains",
                );

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Result should stay non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_concrete_conditional_flow_type_query() {
    with_two_file_state(
        "declare const liveValue: string;\ntype Unsafe = 'x' extends string ? typeof liveValue : never;\nexport type Result = Unsafe;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");

            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive typeof branches must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_local_alias_projection_conditional_recursion() {
    with_two_file_state(
        "type Box<Item> = { value: Item };\nexport type UnboxDeep<Input> = Input extends Box<infer Item> ? UnboxDeep<Item> : Input;",
        "import { UnboxDeep } from './target';",
        |state, target_binder| {
            let unbox_sym = target_binder
                .file_locals
                .get("UnboxDeep")
                .expect("UnboxDeep");
            let (ty, params) = state
                .direct_source_file_type_alias_result(unbox_sym, Some(1), true)
                .expect(
                    "local alias projections should guard recursion through inferred components",
                );

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "UnboxDeep should expose Input");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_pair_alias_projection_recursion() {
    with_two_file_state(
        "type PairBox<First, Rest> = { first: First; rest: Rest };\nexport type LastTail<Subject> = Subject extends PairBox<infer Head, infer Tail> ? LastTail<Tail> : Subject;",
        "import { LastTail } from './target';",
        |state, target_binder| {
            let last_tail_sym = target_binder.file_locals.get("LastTail").expect("LastTail");
            let (ty, params) = state
                .direct_source_file_type_alias_result(last_tail_sym, Some(1), true)
                .expect("renamed multi-argument alias projections should guard consumed recursion");

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "LastTail should expose Subject");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_local_alias_projection_original_arg_recursion() {
    with_two_file_state(
        "type Box<Item> = { value: Item };\nexport type Loop<Input> = Input extends Box<infer Item> ? Loop<Input> : Input;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");

            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "local alias projections only guard recursive calls that consume inferred components",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_recursive_mapped_projection_guard() {
    with_two_file_state(
        "type Primitive = string | number | boolean | bigint | symbol | undefined | null;\nexport type DeepReadonly<T> = T extends ((...args: any[]) => any) | Primitive ? T : T extends _DeepReadonlyArray<infer Item> ? _DeepReadonlyArray<Item> : T extends _DeepReadonlyObject<infer Shape> ? _DeepReadonlyObject<Shape> : T;\nexport interface _DeepReadonlyArray<Item> extends ReadonlyArray<DeepReadonly<Item>> {}\nexport type _DeepReadonlyObject<Shape> = { readonly [Key in keyof Shape]: DeepReadonly<Shape[Key]> };\nexport type ReadOnly<Input extends object> = DeepReadonly<Input>;",
        "import { ReadOnly } from './target';",
        |state, target_binder| {
            let object_sym = target_binder
                .file_locals
                .get("_DeepReadonlyObject")
                .expect("_DeepReadonlyObject");
            let object_symbol = target_binder
                .get_symbol(object_sym)
                .expect("_DeepReadonlyObject symbol");
            let target_arena = state.ctx.get_arena_for_file(1);
            let global_type_is_lowerable = |_: &BinderState, _: &str| true;
            let global_value_is_lowerable = |_: &BinderState, _: &str| true;
            let proof = SourceFileAliasProofContext {
                current_file_idx: Some(1),
                global_type_is_lowerable: &global_type_is_lowerable,
                global_value_is_lowerable: &global_value_is_lowerable,
                import_alias_target: None,
            };

            assert!(
                !CheckerState::source_file_local_type_alias_application_is_projection_lowerable(
                    target_arena,
                    target_binder,
                    object_symbol,
                    1,
                    &proof,
                ),
                "recursive mapped aliases are not transparent projection guards",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_template_prefix_infer_recursion() {
    with_two_file_state(
        "export type TrimSpaces<Input> = Input extends ` ${infer Rest}` ? TrimSpaces<Rest> : Input;",
        "import { TrimSpaces } from './target';",
        |state, target_binder| {
            let trim_sym = target_binder
                .file_locals
                .get("TrimSpaces")
                .expect("TrimSpaces");
            let (ty, params) = state
                .direct_source_file_type_alias_result(trim_sym, Some(1), true)
                .expect(
                    "template literal prefixes should guard recursion through inferred suffixes",
                );

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "TrimSpaces should expose Input");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_template_suffix_infer_recursion() {
    with_two_file_state(
        "export type StripExtension<Path> = Path extends `${infer Stem}.ts` ? StripExtension<Stem> : Path;",
        "import { StripExtension } from './target';",
        |state, target_binder| {
            let strip_sym = target_binder
                .file_locals
                .get("StripExtension")
                .expect("StripExtension");
            let (ty, params) = state
                .direct_source_file_type_alias_result(strip_sym, Some(1), true)
                .expect(
                    "template literal suffixes should guard recursion through inferred prefixes",
                );

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "StripExtension should expose Path");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_named_tuple_member_keyof_local_alias_chain() {
    with_two_file_state(
        "type Ledger = { one: [slot: number, next: keyof Ledger]; two: [slot: number, next: 'one'] };\nexport type Cursor = [value: number, next: keyof Ledger];",
        "import { Cursor } from './target';",
        |state, target_binder| {
            let cursor_sym = target_binder.file_locals.get("Cursor").expect("Cursor");
            let (ty, params) = state
                .direct_source_file_type_alias_result(cursor_sym, Some(1), true)
                .expect("named tuple members should recurse through their type nodes");

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Cursor should stay non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_generic_named_tuple_members() {
    with_two_file_state(
        "export type PairSlots<Item, Rest> = [front: Item, back?: Rest];",
        "import { PairSlots } from './target';",
        |state, target_binder| {
            let pair_sym = target_binder
                .file_locals
                .get("PairSlots")
                .expect("PairSlots");
            let (ty, params) = state
                .direct_source_file_type_alias_result(pair_sym, Some(1), true)
                .expect("generic named tuple members should be transparent to direct lowering");

            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "PairSlots should expose Item and Rest");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_bare_template_infer_recursion() {
    with_two_file_state(
        "export type Loop<Input> = Input extends `${infer Same}` ? Loop<Same> : Input;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");

            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "bare template literal inference does not prove recursive progress",
            );
        },
    );
}
