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
fn direct_source_file_type_alias_lowers_single_hop_local_alias_chain() {
    with_two_file_state(
        "type Leaf = string | number;\nexport type Alias = Leaf;",
        "import { Alias } from './target';",
        |state, target_binder| {
            let alias_sym = target_binder.file_locals.get("Alias").expect("Alias");
            let (ty, params) = state
                .direct_source_file_type_alias_result(alias_sym, Some(1), true)
                .expect("single-hop alias chain must lower without a child checker");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Alias should be non-generic");
            let def_id = state
                .ctx
                .get_existing_def_id(alias_sym)
                .expect("DefId must be registered");
            assert!(
                state.ctx.definition_store.get_body(def_id).is_some(),
                "alias body must be registered for lazy resolution",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_single_hop_chain() {
    with_two_file_state(
        "type Inner = boolean;\nexport type Outer = Inner;",
        "import { Outer } from './target';",
        |state, target_binder| {
            let outer_sym = target_binder.file_locals.get("Outer").expect("Outer");
            let (ty, params) = state
                .direct_source_file_type_alias_result(outer_sym, Some(1), true)
                .expect("renamed single-hop alias chain must lower without a child checker");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Outer should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_multi_hop_chain() {
    with_two_file_state(
        "type C = string | null;\ntype B = C;\nexport type A = B;",
        "import { A } from './target';",
        |state, target_binder| {
            let a_sym = target_binder.file_locals.get("A").expect("A");
            let (ty, params) = state
                .direct_source_file_type_alias_result(a_sym, Some(1), true)
                .expect("multi-hop alias chain must lower without a child checker");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "A should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_union_of_local_refs() {
    with_two_file_state(
        "type Str = string;\ntype Num = number;\nexport type Both = Str | Num;",
        "import { Both } from './target';",
        |state, target_binder| {
            let both_sym = target_binder.file_locals.get("Both").expect("Both");
            let (ty, params) = state
                .direct_source_file_type_alias_result(both_sym, Some(1), true)
                .expect("composite bodies with safe local alias leaves should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Both should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_wrapped_composite_local_refs() {
    with_two_file_state(
        "type Leaf = string;\ntype Maybe = Leaf | undefined;\nexport type Boxed = (Maybe)[];",
        "import { Boxed } from './target';",
        |state, target_binder| {
            let boxed_sym = target_binder.file_locals.get("Boxed").expect("Boxed");
            let (ty, params) = state
                .direct_source_file_type_alias_result(boxed_sym, Some(1), true)
                .expect("wrapped arrays with composite local alias leaves should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Boxed should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_type_operator_over_local_alias_chain() {
    with_two_file_state(
        "type Leaf = string;\ntype Local = Leaf;\nexport type Keys = keyof Local;",
        "import { Keys } from './target';",
        |state, target_binder| {
            let keys_sym = target_binder.file_locals.get("Keys").expect("Keys");
            let (ty, params) = state
                .direct_source_file_type_alias_result(keys_sym, Some(1), true)
                .expect("keyof over a safe local alias chain should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Keys should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_indexed_access_over_local_alias_chain() {
    with_two_file_state(
        "type ObjectAlias = [number];\ntype KeyAlias = 0;\nexport type Picked = ObjectAlias[KeyAlias];",
        "import { Picked } from './target';",
        |state, target_binder| {
            let picked_sym = target_binder.file_locals.get("Picked").expect("Picked");
            let (ty, params) = state
                .direct_source_file_type_alias_result(picked_sym, Some(1), true)
                .expect("indexed access over safe local alias operands should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Picked should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_composite_with_flow_sensitive_local_ref() {
    with_two_file_state(
        "const value = 1;\ntype Flow = typeof value;\nexport type Alias = Flow | string;",
        "import { Alias } from './target';",
        |state, target_binder| {
            let alias_sym = target_binder.file_locals.get("Alias").expect("Alias");
            assert!(
                state
                    .direct_source_file_type_alias_result(alias_sym, Some(1), true)
                    .is_none(),
                "composites with flow-sensitive local refs must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_indexed_access_with_flow_sensitive_operand() {
    with_two_file_state(
        "const key = 0;\ntype Keys = typeof key;\ntype Shape = [number];\nexport type Picked = Shape[Keys];",
        "import { Picked } from './target';",
        |state, target_binder| {
            let picked_sym = target_binder.file_locals.get("Picked").expect("Picked");
            assert!(
                state
                    .direct_source_file_type_alias_result(picked_sym, Some(1), true)
                    .is_none(),
                "indexed access with a flow-sensitive local operand must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_local_generic_alias_application() {
    with_two_file_state(
        "type Wrap<T> = T | null;\nexport type Concrete = Wrap<string>;",
        "import { Concrete } from './target';",
        |state, target_binder| {
            let concrete_sym = target_binder.file_locals.get("Concrete").expect("Concrete");
            let (ty, params) = state
                .direct_source_file_type_alias_result(concrete_sym, Some(1), true)
                .expect("scope-independent generic alias applications should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Concrete should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_local_generic_alias_application() {
    with_two_file_state(
        "type Box<X> = X[];\nexport type Result = Box<boolean>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("renamed generic alias applications should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Result should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_concrete_generic_alias_with_sibling_leaf() {
    with_two_file_state(
        "type Leaf = string;\ntype Wrap<T> = T | Leaf;\nexport type Concrete = Wrap<number>;",
        "import { Concrete } from './target';",
        |state, target_binder| {
            let concrete_sym = target_binder.file_locals.get("Concrete").expect("Concrete");
            let (ty, params) = state
                .direct_source_file_type_alias_result(concrete_sym, Some(1), true)
                .expect("concrete generic aliases may reference safe sibling leaves");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Concrete should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_concrete_generic_alias_chain() {
    with_two_file_state(
        "type Drop<X> = X extends null ? never : X;\ntype Select<T, K> = Drop<K> | T;\nexport type Result = Select<boolean, null>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("concrete generic alias chains through sibling aliases should lower");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Result should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_generic_body_with_local_alias_application() {
    with_two_file_state(
        "type Box<X> = X | null;\nexport type Result<T> = Box<T>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("generic source aliases may reference structural local alias applications");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Result should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_generic_body_with_non_generic_local_alias_leaf() {
    with_two_file_state(
        "type Leaf = string;\nexport type Result<T> = T | Leaf;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("generic source aliases may reference non-generic local alias leaves");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Result should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_generic_body_with_non_generic_local_alias_leaf() {
    with_two_file_state(
        "type Base = number;\nexport type Output<X> = [Base, X];",
        "import { Output } from './target';",
        |state, target_binder| {
            let output_sym = target_binder.file_locals.get("Output").expect("Output");
            let (ty, params) = state
                .direct_source_file_type_alias_result(output_sym, Some(1), true)
                .expect("renamed generic source aliases may reference safe non-generic leaves");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Output should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_concrete_generic_alias_cycle() {
    with_two_file_state(
        "type Loop<T> = Loop<T> | T;\nexport type Concrete = Loop<string>;",
        "import { Concrete } from './target';",
        |state, target_binder| {
            let concrete_sym = target_binder.file_locals.get("Concrete").expect("Concrete");
            assert!(
                state
                    .direct_source_file_type_alias_result(concrete_sym, Some(1), true)
                    .is_none(),
                "recursive concrete generic aliases must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_generic_alias_application_with_typeof_body() {
    with_two_file_state(
        "const v = 1;\ntype Wrap<T> = T | typeof v;\nexport type Concrete = Wrap<string>;",
        "import { Concrete } from './target';",
        |state, target_binder| {
            let concrete_sym = target_binder.file_locals.get("Concrete").expect("Concrete");
            assert!(
                state
                    .direct_source_file_type_alias_result(concrete_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive generic alias applications must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_mutual_recursion_in_chain() {
    with_two_file_state(
        "type Ping = Pong | string;\nexport type Pong = Ping | number;",
        "import { Pong } from './target';",
        |state, target_binder| {
            let pong_sym = target_binder.file_locals.get("Pong").expect("Pong");
            assert!(
                state
                    .direct_source_file_type_alias_result(pong_sym, Some(1), true)
                    .is_none(),
                "mutual-recursion in chain must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_chain_containing_typeof() {
    with_two_file_state(
        "const v = 1;\ntype Base = typeof v;\nexport type Alias = Base;",
        "import { Alias } from './target';",
        |state, target_binder| {
            let alias_sym = target_binder.file_locals.get("Alias").expect("Alias");
            assert!(
                state
                    .direct_source_file_type_alias_result(alias_sym, Some(1), true)
                    .is_none(),
                "chain with typeof in a referenced alias must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_chain_when_alias_guard_limit_is_hit() {
    let mut target_source = String::from("type A130 = string;\n");
    for index in (1..130).rev() {
        target_source.push_str(&format!("type A{index} = A{};\n", index + 1));
    }
    target_source.push_str("export type Alias = A1;\n");

    with_two_file_state(
        &target_source,
        "import { Alias } from './target';",
        |state, target_binder| {
            let alias_sym = target_binder.file_locals.get("Alias").expect("Alias");
            assert!(
                state
                    .direct_source_file_type_alias_result(alias_sym, Some(1), true)
                    .is_none(),
                "alias chains that exceed the recursion guard must stay on the child-checker path",
            );
        },
    );
}
