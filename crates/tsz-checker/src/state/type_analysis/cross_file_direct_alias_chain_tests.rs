use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolTable, symbol_flags};
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

fn with_two_file_state_with_libs<F, R>(
    target_source: &str,
    requester_source: &str,
    libs: &[&str],
    test: F,
) -> R
where
    F: FnOnce(&mut CheckerState<'_>, &Arc<BinderState>) -> R,
{
    let lib_files = load_lib_files(libs);
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
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());
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
fn direct_source_file_type_alias_lowers_mapped_type_with_own_key() {
    with_two_file_state(
        "type Keys<T> = keyof T;\nexport type Box<T> = { [P in Keys<T>]: T[P] };",
        "import { Box } from './target';",
        |state, target_binder| {
            let box_sym = target_binder.file_locals.get("Box").expect("Box");
            let (ty, params) = state
                .direct_source_file_type_alias_result(box_sym, Some(1), true)
                .expect("mapped bodies over safe local alias constraints should lower");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Box should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_mapped_type_with_local_value_alias() {
    with_two_file_state(
        "type KeySet<X> = keyof X;\ntype Val<Obj, Key extends keyof Obj> = Obj[Key];\nexport type Remap<Obj> = { [Name in KeySet<Obj>]: Val<Obj, Name> };",
        "import { Remap } from './target';",
        |state, target_binder| {
            let remap_sym = target_binder.file_locals.get("Remap").expect("Remap");
            let (ty, params) = state
                .direct_source_file_type_alias_result(remap_sym, Some(1), true)
                .expect("renamed mapped type parameters should lower structurally");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Remap should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_indexed_type_literal_with_local_alias_values() {
    with_two_file_state(
        "type Is<T, U> = T extends U ? 1 : 0;\nexport type Select<T, U> = T extends unknown ? { 1: T & U, 0: never }[Is<T, U>] : never;",
        "import { Select } from './target';",
        |state, target_binder| {
            let select_sym = target_binder.file_locals.get("Select").expect("Select");
            let (ty, params) = state
                .direct_source_file_type_alias_result(select_sym, Some(1), true)
                .expect("indexed type literals with safe property values should lower");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(
                params.len(),
                2,
                "Select should preserve its type parameters"
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_type_literal_property_local_alias_application() {
    with_two_file_state(
        "type Leaf<X> = X | null;\ntype Pick<X> = X extends unknown ? 1 : 0;\nexport type Select<T> = { 1: Leaf<T>, 0: never }[Pick<T>];",
        "import { Select } from './target';",
        |state, target_binder| {
            let select_sym = target_binder.file_locals.get("Select").expect("Select");
            let (ty, params) = state
                .direct_source_file_type_alias_result(select_sym, Some(1), true)
                .expect(
                    "type-literal property values with safe local alias applications should lower",
                );
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Select should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_type_literal_with_computed_name() {
    with_two_file_state(
        "declare const key: unique symbol;\nexport type Box<T> = { [key]: T };",
        "import { Box } from './target';",
        |state, target_binder| {
            let box_sym = target_binder.file_locals.get("Box").expect("Box");
            assert!(
                state
                    .direct_source_file_type_alias_result(box_sym, Some(1), true)
                    .is_none(),
                "computed property names must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_type_literal_property_typeof_alias_application() {
    with_two_file_state(
        "const value = 1;\ntype Leaf<X> = X | typeof value;\ntype Pick<X> = X extends unknown ? 1 : 0;\nexport type Select<T> = { 1: Leaf<T>, 0: never }[Pick<T>];",
        "import { Select } from './target';",
        |state, target_binder| {
            let select_sym = target_binder.file_locals.get("Select").expect("Select");
            assert!(
                state
                    .direct_source_file_type_alias_result(select_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive local alias applications in type-literal properties must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_same_binder_export_alias_symbol() {
    let (arena, binder, types) =
        parse_bound_source("type Leaf = string;\nexport type Result = Alias;");
    let mut binder = (*binder).clone();
    let leaf_sym = binder.file_locals.get("Leaf").expect("Leaf");
    let alias_sym = binder
        .symbols
        .alloc(symbol_flags::ALIAS, "Alias".to_string());
    {
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("Alias symbol");
        alias_symbol.import_module = Some("./target".to_string());
        alias_symbol.import_name = Some("Leaf".to_string());
        alias_symbol.is_type_only = true;
    }
    binder.file_locals.set("Alias".to_string(), alias_sym);
    let mut exports = SymbolTable::new();
    exports.set("Leaf".to_string(), leaf_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

    let binder = Arc::new(binder);
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Result } from './target';");
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
        Arc::clone(&arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&binder),
    ]));

    let result_sym = binder.file_locals.get("Result").expect("Result");
    let (ty, params) = state
        .direct_source_file_type_alias_result(result_sym, Some(1), true)
        .expect("same-binder export aliases to safe local type aliases should lower");
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "Result should be non-generic");
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_same_binder_alias_with_type_args() {
    let (arena, binder, types) =
        parse_bound_source("type Wrap<X> = X | null;\nexport type Output<T> = Renamed<T>;");
    let mut binder = (*binder).clone();
    let wrap_sym = binder.file_locals.get("Wrap").expect("Wrap");
    let alias_sym = binder
        .symbols
        .alloc(symbol_flags::ALIAS, "Renamed".to_string());
    {
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("Renamed symbol");
        alias_symbol.import_module = Some("./target".to_string());
        alias_symbol.import_name = Some("Wrap".to_string());
        alias_symbol.is_type_only = true;
    }
    binder.file_locals.set("Renamed".to_string(), alias_sym);
    let mut exports = SymbolTable::new();
    exports.set("Wrap".to_string(), wrap_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

    let binder = Arc::new(binder);
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Output } from './target';");
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
        Arc::clone(&arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&binder),
    ]));

    let output_sym = binder.file_locals.get("Output").expect("Output");
    let (ty, params) = state
        .direct_source_file_type_alias_result(output_sym, Some(1), true)
        .expect("renamed alias symbols with safe type args should lower structurally");
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "Output should preserve its type parameter");
}

#[test]
fn direct_source_file_type_alias_rejects_alias_symbol_to_typeof_body() {
    let (arena, binder, types) = parse_bound_source(
        "const value = 1;\ntype Flow = typeof value;\nexport type Result = Alias;",
    );
    let mut binder = (*binder).clone();
    let flow_sym = binder.file_locals.get("Flow").expect("Flow");
    let alias_sym = binder
        .symbols
        .alloc(symbol_flags::ALIAS, "Alias".to_string());
    {
        let alias_symbol = binder.symbols.get_mut(alias_sym).expect("Alias symbol");
        alias_symbol.import_module = Some("./target".to_string());
        alias_symbol.import_name = Some("Flow".to_string());
        alias_symbol.is_type_only = true;
    }
    binder.file_locals.set("Alias".to_string(), alias_sym);
    let mut exports = SymbolTable::new();
    exports.set("Flow".to_string(), flow_sym);
    Arc::make_mut(&mut binder.module_exports).insert("./target".to_string(), exports);

    let binder = Arc::new(binder);
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Result } from './target';");
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
        Arc::clone(&arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&binder),
    ]));

    let result_sym = binder.file_locals.get("Result").expect("Result");
    assert!(
        state
            .direct_source_file_type_alias_result(result_sym, Some(1), true)
            .is_none(),
        "alias symbols to flow-sensitive type aliases must stay on the child-checker path",
    );
}

#[test]
fn direct_source_file_type_alias_rejects_mapped_type_with_typeof_value() {
    with_two_file_state(
        "const value = 1;\nexport type Box<T> = { [P in keyof T]: typeof value };",
        "import { Box } from './target';",
        |state, target_binder| {
            let box_sym = target_binder.file_locals.get("Box").expect("Box");
            assert!(
                state
                    .direct_source_file_type_alias_result(box_sym, Some(1), true)
                    .is_none(),
                "mapped types with flow-sensitive value types must stay on the child-checker path",
            );
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
fn direct_source_file_type_alias_lowers_generic_function_type_body() {
    with_two_file_state(
        "export type UnionToIntersection<U> = (U extends any ? (k: U) => void : never) extends (k: infer I) => void ? I : never;",
        "import { UnionToIntersection } from './target';",
        |state, target_binder| {
            let result_sym = target_binder
                .file_locals
                .get("UnionToIntersection")
                .expect("UnionToIntersection");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("generic function type alias bodies should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(
                params.len(),
                1,
                "generic alias parameter should be preserved"
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_function_type_own_type_params() {
    with_two_file_state(
        "type Compare<X, Y, A = X, B = never> = (<T>() => T extends X ? 1 : 2) extends (<U>() => U extends Y ? 1 : 2) ? A : B;\nexport type Result<L, R> = Compare<L, R>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("function type local type params should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(
                params.len(),
                2,
                "generic alias parameters should be preserved"
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_constructor_type_body() {
    with_two_file_state(
        "export type Class<T> = new (...args: any[]) => T;",
        "import { Class } from './target';",
        |state, target_binder| {
            let class_sym = target_binder.file_locals.get("Class").expect("Class");
            let (ty, params) = state
                .direct_source_file_type_alias_result(class_sym, Some(1), true)
                .expect("constructor type alias bodies should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(
                params.len(),
                1,
                "generic alias parameter should be preserved"
            );
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
fn direct_source_file_type_alias_rejects_function_type_with_typeof_param() {
    with_two_file_state(
        "const v = 1;\nexport type FromValue<T> = (arg: typeof v) => T;",
        "import { FromValue } from './target';",
        |state, target_binder| {
            let result_sym = target_binder
                .file_locals
                .get("FromValue")
                .expect("FromValue");
            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive function type aliases must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_function_type_param_typeof_constraint() {
    with_two_file_state(
        "const v = 1;\nexport type FromValue<T> = (<U extends typeof v>() => U) extends (() => T) ? T : never;",
        "import { FromValue } from './target';",
        |state, target_binder| {
            let result_sym = target_binder
                .file_locals
                .get("FromValue")
                .expect("FromValue");
            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive function type parameter constraints must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_constructor_type_typeof_param() {
    with_two_file_state(
        "const v = 1;\nexport type FromValue<T> = new (arg: typeof v) => T;",
        "import { FromValue } from './target';",
        |state, target_binder| {
            let result_sym = target_binder
                .file_locals
                .get("FromValue")
                .expect("FromValue");
            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive constructor type parameters must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_omitted_non_default_type_arg() {
    with_two_file_state(
        "type Pair<L, R> = (<T>() => T extends L ? 1 : 2) extends (<U>() => U extends R ? 1 : 2) ? L : R;\nexport type Result<T> = Pair<T>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "omitted non-defaulted alias type args must stay on the child-checker path",
            );
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
fn direct_source_file_type_alias_lowers_unshadowed_global_function_reference() {
    with_two_file_state_with_libs(
        "export type FunctionKeys<T> = { [K in keyof T]-?: T[K] extends Function ? K : never }[keyof T];",
        "import { FunctionKeys } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let function_keys_sym = target_binder
                .file_locals
                .get("FunctionKeys")
                .expect("FunctionKeys");
            let (ty, params) = state
                .direct_source_file_type_alias_result(function_keys_sym, Some(1), true)
                .expect("unshadowed global Function references should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "FunctionKeys should expose T");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_unshadowed_global_generic_reference() {
    with_two_file_state_with_libs(
        "export type Keep<Obj, Key extends keyof Obj> = Pick<Obj, Key>;",
        "import { Keep } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let keep_sym = target_binder.file_locals.get("Keep").expect("Keep");
            let (ty, params) = state
                .direct_source_file_type_alias_result(keep_sym, Some(1), true)
                .expect("unshadowed global generic type references should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "Keep should expose Obj and Key");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_global_generic_reference_with_namespace_shadow() {
    with_two_file_state_with_libs(
        "namespace Pick {}\nexport type Keep<Obj, Key extends keyof Obj> = Pick<Obj, Key>;",
        "import { Keep } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let keep_sym = target_binder.file_locals.get("Keep").expect("Keep");
            let (ty, params) = state
                .direct_source_file_type_alias_result(keep_sym, Some(1), true)
                .expect("namespace-only locals should not shadow global type aliases");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "Keep should expose Obj and Key");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_global_generic_reference_with_value_shadow() {
    with_two_file_state_with_libs(
        "const Pick = 1;\nexport type Keep<Obj, Key extends keyof Obj> = Pick<Obj, Key>;",
        "import { Keep } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let keep_sym = target_binder.file_locals.get("Keep").expect("Keep");
            let (ty, params) = state
                .direct_source_file_type_alias_result(keep_sym, Some(1), true)
                .expect("value-only locals should not shadow global type aliases");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "Keep should expose Obj and Key");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_pick_by_value_shape_with_namespace_shadow() {
    with_two_file_state_with_libs(
        "import { Primitive } from './aliases-and-guards';\nnamespace Pick {}\nexport type PickByValue<T, ValueType> = Pick<T, { [Key in keyof T]-?: T[Key] extends ValueType ? Key : never }[keyof T]>;",
        "import { PickByValue } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let pick_by_value_sym = target_binder
                .file_locals
                .get("PickByValue")
                .expect("PickByValue");
            let (ty, params) = state
                .direct_source_file_type_alias_result(pick_by_value_sym, Some(1), true)
                .expect("utility-style PickByValue aliases should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "PickByValue should expose T and ValueType");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_local_type_alias_namespace_merge_shadow() {
    with_two_file_state_with_libs(
        "namespace Pick {}\ntype Pick<T, K> = T;\nexport type Keep<Obj, Key extends keyof Obj> = Pick<Obj, Key>;",
        "import { Keep } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let keep_sym = target_binder.file_locals.get("Keep").expect("Keep");
            assert!(
                state
                    .direct_source_file_type_alias_result(keep_sym, Some(1), true)
                    .is_none(),
                "local type declarations merged with namespaces must not fall through to globals",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_local_conditional_alias_argument_chain() {
    with_two_file_state_with_libs(
        "type SetDifference<A, B> = A extends B ? never : A;\ntype SetComplement<A, A1 extends A> = SetDifference<A, A1>;\nexport type FlowDiff<T extends U, U extends object> = Pick<T, SetComplement<keyof T, keyof U>>;",
        "import { FlowDiff } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let flow_diff_sym = target_binder.file_locals.get("FlowDiff").expect("FlowDiff");
            let (ty, params) = state
                .direct_source_file_type_alias_result(flow_diff_sym, Some(1), true)
                .expect("local conditional alias argument chains should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "FlowDiff should expose T and U");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_intersection_of_local_and_global_applications() {
    with_two_file_state_with_libs(
        "type SetDifference<A, B> = A extends B ? never : A;\ntype Omit<T, K extends keyof any> = Pick<T, SetDifference<keyof T, K>>;\nexport type AugmentedRequired<T extends object, K extends keyof T = keyof T> = Omit<T, K> & Required<Pick<T, K>>;",
        "import { AugmentedRequired } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let augmented_required_sym = target_binder
                .file_locals
                .get("AugmentedRequired")
                .expect("AugmentedRequired");
            let (ty, params) = state
                .direct_source_file_type_alias_result(augmented_required_sym, Some(1), true)
                .expect("intersections of lowerable local and global generic applications should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "AugmentedRequired should expose T and K");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_shadowed_global_function_reference() {
    with_two_file_state_with_libs(
        "interface Function { local: string }\nexport type FunctionKeys<T> = { [K in keyof T]-?: T[K] extends Function ? K : never }[keyof T];",
        "import { FunctionKeys } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let function_keys_sym = target_binder
                .file_locals
                .get("FunctionKeys")
                .expect("FunctionKeys");
            assert!(
                state
                    .direct_source_file_type_alias_result(function_keys_sym, Some(1), true)
                    .is_none(),
                "local shadows of global lib names must stay on the child-checker path",
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
