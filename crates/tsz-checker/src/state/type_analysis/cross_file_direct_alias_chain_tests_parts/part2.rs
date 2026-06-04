#[test]
fn direct_source_file_type_alias_lowers_array_element_conditional_recursion() {
    with_two_file_state(
        "type Exact<Left, Right> = [Left] extends [readonly any[]] ? [Right] extends [readonly any[]] ? [Left, Right] extends [readonly (infer LeftElement)[], readonly (infer RightElement)[]] ? Exact<LeftElement, RightElement> extends LeftElement ? Left : never : never : never : Left;\nexport type Result<Items extends readonly unknown[], Shape extends readonly unknown[]> = Exact<Items, Shape>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (result_ty, result_params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("array element inference structurally guards recursive aliases");
            assert_ne!(result_ty, TypeId::UNKNOWN);
            assert_ne!(result_ty, TypeId::ERROR);
            assert_eq!(result_params.len(), 2, "Result should expose both params");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_array_conditional_original_arg_recursion() {
    with_two_file_state(
        "export type Loop<Items> = [Items] extends [readonly (infer Element)[]] ? Loop<Items> : Items;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "array-element conditionals only guard recursive calls that consume the inferred element",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_global_projection_conditional_recursion() {
    with_two_file_state_with_libs(
        "export type DeepMap<Input> = Input extends Map<infer Key, infer Value> ? Map<DeepMap<Key>, DeepMap<Value>> : Input;",
        "import { DeepMap } from './target';",
        &["es5.d.ts", "es2015.collection.d.ts"],
        |state, target_binder| {
            let deep_map_sym = target_binder.file_locals.get("DeepMap").expect("DeepMap");
            let (deep_map_ty, deep_map_params) = state
                .direct_source_file_type_alias_result(deep_map_sym, Some(1), true)
                .expect("global generic projection inference should guard recursive aliases");
            assert_ne!(deep_map_ty, TypeId::UNKNOWN);
            assert_ne!(deep_map_ty, TypeId::ERROR);
            assert_eq!(deep_map_params.len(), 1, "DeepMap should expose Input");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_global_projection_original_arg_recursion() {
    with_two_file_state_with_libs(
        "export type Loop<Input> = Input extends Map<infer Key, infer Value> ? Loop<Input> : Input;",
        "import { Loop } from './target';",
        &["es5.d.ts", "es2015.collection.d.ts"],
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "global generic projection conditionals only guard recursive calls that consume inferred components",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_subtractive_infer_recursion() {
    with_two_file_state_with_libs(
        "export type Result<Whole, Accumulator extends string[] = []> = Whole extends infer Part ? Part extends string ? Result<Exclude<Whole, Part>, [...Accumulator, Part]> : never : never;",
        "import { Result } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (result_ty, result_params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("global Exclude over an inferred branch param should guard recursion");
            assert_ne!(result_ty, TypeId::UNKNOWN);
            assert_ne!(result_ty, TypeId::ERROR);
            assert_eq!(result_params.len(), 2, "Result should expose both params");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_subtractive_infer_recursion_through_local_helper() {
    with_two_file_state_with_libs(
        "type Without<All, Part> = Exclude<All, Part>;\nexport type Result<Whole, Accumulator extends string[] = []> = Whole extends infer Part ? Part extends string ? Result<Without<Whole, Part>, [...Accumulator, Part]> : never : never;",
        "import { Result } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (result_ty, result_params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("local helper aliases over global Exclude should preserve the subtractive guard");
            assert_ne!(result_ty, TypeId::UNKNOWN);
            assert_ne!(result_ty, TypeId::ERROR);
            assert_eq!(result_params.len(), 2, "Result should expose both params");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_swapped_subtractive_helper() {
    with_two_file_state_with_libs(
        "type Without<All, Part> = Exclude<Part, All>;\nexport type Loop<Input> = Input extends infer Part ? Loop<Without<Input, Part>> : Input;",
        "import { Loop } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "transparent subtractive helpers must remove the second argument from the first",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_union_to_tuple_subtractive_recursion() {
    with_program_state_with_libs(
        &[
            (
                "union-to-intersection.ts",
                "export type UnionToIntersection<Union> = (Union extends any ? (arg: Union) => void : never) extends (arg: infer Intersection) => void ? Intersection & Union : never;",
            ),
            (
                "union-to-tuple.ts",
                "import { UnionToIntersection } from './union-to-intersection';\ntype LastOfUnion<UnionType> = UnionToIntersection<UnionType extends unknown ? (arg: UnionType) => unknown : never> extends (arg: infer LastUnionElement) => unknown ? LastUnionElement : never;\nexport type UnionToTuple<UnionType, Accumulator extends string[] = []> = [UnionType] extends [never] ? Accumulator : LastOfUnion<UnionType> extends infer LastUnionElement ? LastUnionElement extends string ? UnionToTuple<Exclude<UnionType, LastUnionElement>, [...Accumulator, LastUnionElement]> : never : never;",
            ),
            (
                "requester.ts",
                "import { UnionToTuple } from './union-to-tuple';",
            ),
        ],
        "requester.ts",
        "union-to-tuple.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let tuple_sym = target_binder
                .file_locals
                .get("UnionToTuple")
                .expect("UnionToTuple");
            let (tuple_ty, tuple_params) = state
                .direct_source_file_type_alias_result(tuple_sym, Some(target_idx), true)
                .expect("UnionToTuple subtracts the inferred last union element before recursing");
            assert_ne!(tuple_ty, TypeId::UNKNOWN);
            assert_ne!(tuple_ty, TypeId::ERROR);
            assert_eq!(
                tuple_params.len(),
                2,
                "UnionToTuple should expose both params"
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_plain_infer_recursion() {
    with_two_file_state_with_libs(
        "export type Loop<Input> = Input extends infer Part ? Loop<Part> : Input;",
        "import { Loop } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "plain inferred params do not structurally guard recursive calls",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_subtractive_recursion_with_local_exclude() {
    with_two_file_state_with_libs(
        "type Exclude<A, B> = A;\nexport type Loop<Input> = Input extends infer Part ? Loop<Exclude<Input, Part>> : Input;",
        "import { Loop } from './target';",
        &["es5.d.ts"],
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "local Exclude aliases must not prove subtractive recursion",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_unguarded_direct_self_alias() {
    with_two_file_state(
        "export type Loop = Loop | string;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "unguarded direct self aliases must stay on the child-checker path",
            );
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_unguarded_generic_self_alias() {
    with_two_file_state(
        "export type Loop<Item> = Loop<Item> | Item;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "unguarded generic self aliases must stay on the child-checker path",
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
fn direct_source_file_type_alias_lowers_well_known_symbol_iterator_type_query() {
    with_two_file_state_with_libs(
        "type DeepObject<T> = { [K in keyof T]: K extends typeof Symbol.iterator ? T[K] extends () => Iterator<infer Item, infer Return, infer Next> ? () => Iterator<Deep<Item>, Deep<Return>, Deep<Next>> : Deep<T[K]> : Deep<T[K]> };\nexport type Deep<T> = T extends object ? DeepObject<T> : T;",
        "import { Deep } from './target';",
        &[
            "es5.d.ts",
            "es2015.symbol.d.ts",
            "es2015.symbol.wellknown.d.ts",
            "es2015.iterable.d.ts",
        ],
        |state, target_binder| {
            let deep_sym = target_binder.file_locals.get("Deep").expect("Deep");
            let target_arena = state.ctx.all_arenas.as_ref().expect("arenas")[1].clone();
            let deep_symbol = target_binder.get_symbol(deep_sym).expect("Deep symbol");
            let deep_decl = deep_symbol.declarations[0];
            let deep_node = target_arena.get(deep_decl).expect("Deep decl");
            let deep_alias = target_arena.get_type_alias(deep_node).expect("Deep alias");
            assert!(
                !CheckerState::source_file_type_node_contains_disallowed_type_query(
                    target_arena.as_ref(),
                    target_binder.as_ref(),
                    deep_alias.type_node,
                ),
                "well-known Symbol.iterator should be the only type query",
            );
            assert!(
                state.source_file_type_node_type_queries_are_direct_lowerable(
                    target_arena.as_ref(),
                    deep_alias.type_node,
                ),
                "well-known Symbol.iterator should resolve to a lib unique symbol",
            );
            let (ty, params) = state
                .direct_source_file_type_alias_result(deep_sym, Some(1), true)
                .expect("well-known Symbol.iterator type queries should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Deep should expose T");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_shadowed_symbol_iterator_type_query() {
    with_two_file_state_with_libs(
        "declare const Symbol: { iterator: unique symbol };\nexport type Shadowed<T> = T extends typeof Symbol.iterator ? T : never;",
        "import { Shadowed } from './target';",
        &[
            "es5.d.ts",
            "es2015.symbol.d.ts",
            "es2015.symbol.wellknown.d.ts",
        ],
        |state, target_binder| {
            let shadowed_sym = target_binder.file_locals.get("Shadowed").expect("Shadowed");
            assert!(
                state
                    .direct_source_file_type_alias_result(shadowed_sym, Some(1), true)
                    .is_none(),
                "local Symbol shadows must stay on the child-checker path",
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
