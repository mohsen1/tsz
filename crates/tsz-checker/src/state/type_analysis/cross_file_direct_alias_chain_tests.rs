use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::module_resolution::build_module_resolution_maps;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolTable, symbol_flags};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

pub(super) fn parse_bound_source(
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

fn parse_bound_named_source(
    file_name: &str,
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
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

pub(super) fn with_program_state_with_libs<F, R>(
    files: &[(&str, &str)],
    requester_file: &str,
    target_file: &str,
    libs: &[&str],
    test: F,
) -> R
where
    F: FnOnce(&mut CheckerState<'_>, &Arc<BinderState>, usize) -> R,
{
    let lib_files = load_lib_files(libs);
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut file_names = Vec::with_capacity(files.len());
    let mut types = None;
    for (file_name, source) in files {
        let (arena, binder, file_types) = parse_bound_named_source(file_name, source);
        arenas.push(arena);
        binders.push(binder);
        file_names.push((*file_name).to_string());
        if types.is_none() {
            types = Some(file_types);
        }
    }
    let requester_idx = file_names
        .iter()
        .position(|name| name == requester_file)
        .unwrap_or_else(|| panic!("requester_file {requester_file:?} not found"));
    let target_idx = file_names
        .iter()
        .position(|name| name == target_file)
        .unwrap_or_else(|| panic!("target_file {target_file:?} not found"));
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = types.unwrap_or_else(TypeInterner::new);
    let ctx = CheckerContext::new(
        all_arenas[requester_idx].as_ref(),
        all_binders[requester_idx].as_ref(),
        &types,
        requester_file.to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    state.ctx.set_all_arenas(Arc::clone(&all_arenas));
    state.ctx.set_all_binders(Arc::clone(&all_binders));
    state.ctx.set_current_file_idx(requester_idx);
    state
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    state.ctx.set_resolved_modules(resolved_modules);
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());
    test(&mut state, &all_binders[target_idx], target_idx)
}

pub(super) fn with_two_file_state_with_libs<F, R>(
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
fn direct_source_file_type_alias_lowers_imported_conditional_alias_argument_chain() {
    with_program_state_with_libs(
        &[
            (
                "mapped-types.ts",
                "export type SetDifference<A, B> = A extends B ? never : A;\nexport type SetComplement<A, A1 extends A> = SetDifference<A, A1>;",
            ),
            (
                "utility-types.ts",
                "import { SetComplement } from './mapped-types';\nexport type FlowDiff<T extends U, U extends object> = Pick<T, SetComplement<keyof T, keyof U>>;",
            ),
            (
                "requester.ts",
                "import { FlowDiff } from './utility-types';",
            ),
        ],
        "requester.ts",
        "utility-types.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let flow_diff_sym = target_binder.file_locals.get("FlowDiff").expect("FlowDiff");
            let (ty, params) = state
                .direct_source_file_type_alias_result(flow_diff_sym, Some(target_idx), true)
                .expect("resolved imported conditional alias arguments should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "FlowDiff should expose T and U");
        },
    );
}

#[test]
fn direct_source_file_type_alias_caches_cross_file_symbol_result() {
    with_program_state_with_libs(
        &[
            (
                "helpers.ts",
                "export type Keep<Source, Target> = Source extends Target ? Source : never;",
            ),
            (
                "target.ts",
                "import { Keep } from './helpers';\nexport type PickString<Value> = Keep<Value, string>;",
            ),
            ("requester.ts", "import { PickString } from './target';"),
        ],
        "requester.ts",
        "target.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            state.ctx.share_owner_symbol_type_results = true;
            let pick_string_sym = target_binder
                .file_locals
                .get("PickString")
                .expect("PickString");
            let (ty, params) = state
                .direct_source_file_type_alias_result(pick_string_sym, Some(target_idx), true)
                .expect("direct source-file aliases should lower");
            let (cached_ty, cached_params) = state
                .ctx
                .cached_cross_file_symbol_type(pick_string_sym, target_idx as u32)
                .expect("successful direct source-file lowering should seed cross-file cache");

            assert_eq!(cached_ty, ty);
            assert_eq!(cached_params.len(), params.len());
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_imported_alias_argument_chain() {
    with_program_state_with_libs(
        &[
            (
                "filters.ts",
                "export type Without<All, Some> = All extends Some ? never : All;\nexport type Remainder<All, Subset extends All> = Without<All, Subset>;",
            ),
            (
                "tools.ts",
                "import { Remainder as DropKeys } from './filters';\nexport type DiffShape<Left extends Right, Right extends object> = Pick<Left, DropKeys<keyof Left, keyof Right>>;",
            ),
            ("requester.ts", "import { DiffShape } from './tools';"),
        ],
        "requester.ts",
        "tools.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let diff_shape_sym = target_binder
                .file_locals
                .get("DiffShape")
                .expect("DiffShape");
            let (ty, params) = state
                .direct_source_file_type_alias_result(diff_shape_sym, Some(target_idx), true)
                .expect("renamed imported alias argument chains should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "DiffShape should expose Left and Right");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_imported_defaulted_helper_chain() {
    with_program_state_with_libs(
        &[
            (
                "primitive.ts",
                "export type Primitive = string | number | boolean | null | undefined;",
            ),
            (
                "array.ts",
                "export type AnyArray<Item = any> = Array<Item> | ReadonlyArray<Item>;",
            ),
            (
                "function.ts",
                "export type AnyFunction<Args extends any[] = any[], Result = any> = (...args: Args) => Result;",
            ),
            (
                "value-of.ts",
                "import { AnyArray } from './array';\nimport { AnyFunction } from './function';\nimport { Primitive } from './primitive';\nexport type ValueOf<Type> = Type extends Primitive ? Type : Type extends AnyArray ? Type[number] : Type extends AnyFunction ? ReturnType<Type> : Type[keyof Type];",
            ),
            ("requester.ts", "import { ValueOf } from './value-of';"),
        ],
        "requester.ts",
        "value-of.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let value_of_sym = target_binder.file_locals.get("ValueOf").expect("ValueOf");
            let (ty, params) = state
                .direct_source_file_type_alias_result(value_of_sym, Some(target_idx), true)
                .expect("imported defaulted helper aliases should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "ValueOf should expose Type");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_imported_builtin_union_helper_chain() {
    with_program_state_with_libs(
        &[
            (
                "primitive.ts",
                "export type Primitive = string | number | boolean | null | undefined;",
            ),
            (
                "built-in.ts",
                "import { Primitive } from './primitive';\nexport type Builtin = Primitive | Function | Date | Error | RegExp;",
            ),
            (
                "deep.ts",
                "import { Builtin } from './built-in';\nexport type Deep<T> = T extends Exclude<Builtin, Error> ? T : Partial<T>;",
            ),
            ("requester.ts", "import { Deep } from './deep';"),
        ],
        "requester.ts",
        "deep.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let deep_sym = target_binder.file_locals.get("Deep").expect("Deep");
            let (ty, params) = state
                .direct_source_file_type_alias_result(deep_sym, Some(target_idx), true)
                .expect("imported builtin union helper aliases should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Deep should expose T");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_type_literal_property_alias_chain() {
    with_two_file_state(
        "type Leaf<U> = { item: U };\ntype Box<T> = { value: Leaf<T> };\ntype Wrap<T, U> = T | U;\nexport type Result<T> = Wrap<T, Box<T>>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("type literal property alias chains should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Result should expose T");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_template_literal_type_alias_chain() {
    with_two_file_state(
        "type Accessor = `${number}`;\ntype Options = { depth: 7; accessor: Accessor };\ntype Wrap<T, U> = T | U;\nexport type Result<T> = Wrap<T, Options>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("template literal type alias chains should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Result should expose T");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_imported_mapped_options_alias_chain() {
    with_program_state_with_libs(
        &[
            (
                "create-type-options.ts",
                "export type CreateTypeOptions<Options extends Required<Options>, OverrideOptions extends Partial<Options>, DefaultOptions extends Required<Options>> = { [Key in keyof Options]: OverrideOptions[Key] extends Options[Key] ? OverrideOptions[Key] : DefaultOptions[Key]; };",
            ),
            (
                "paths.ts",
                "import { CreateTypeOptions } from './create-type-options';\ntype Options = { depth: number; accessor: string };\ntype Defaults = { depth: 7; accessor: `${number}` };\ntype Unsafe<T, O extends Required<Options>> = T | O;\nexport type Result<T, Override extends Partial<Options> = {}> = Unsafe<T, CreateTypeOptions<Options, Override, Defaults>>;",
            ),
            ("requester.ts", "import { Result } from './paths';"),
        ],
        "requester.ts",
        "paths.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(target_idx), true)
                .expect("imported mapped option aliases should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "Result should expose T and Override");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_imported_keyset_range_leaf() {
    with_program_state_with_libs(
        &[
            ("list.ts", "export type List = readonly unknown[];"),
            (
                "union-of.ts",
                "import { List } from './list';\nexport type UnionOf<L extends List> = L[number];",
            ),
            ("internal.ts", "export type Way = '->' | '<-';"),
            (
                "range.ts",
                "import { Way } from './internal';\nexport const Range = null as never;\ntype Step<From extends number, To extends number, Mode extends Way> = Step<From, To, Mode> | From | To | Mode;\nexport type Range<From extends number, To extends number, Mode extends Way = '->'> = From extends unknown ? To extends unknown ? Step<From, To, Mode>[] : never : never;",
            ),
            (
                "key-set.ts",
                "import { Range } from './range';\nimport { UnionOf } from './union-of';\nexport type KeySet<From extends number, To extends number> = UnionOf<Range<From, To, '->'>>;",
            ),
            ("requester.ts", "import { KeySet } from './key-set';"),
        ],
        "requester.ts",
        "key-set.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            let keyset_sym = target_binder.file_locals.get("KeySet").expect("KeySet");
            let (ty, params) = state
                .direct_source_file_type_alias_result(keyset_sym, Some(target_idx), true)
                .expect("imported range alias applications can remain lazy leaves");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 2, "KeySet should expose From and To");
        },
    );
}

#[test]
fn imported_alias_shortcut_caches_direct_source_target_for_second_alias() {
    with_program_state_with_libs(
        &[
            (
                "target.ts",
                "type Pad0 = unknown;\ntype Pad1 = unknown;\nexport type Shared<T> = { readonly value: T };",
            ),
            (
                "requester.ts",
                "import { Shared as FirstShared } from './target';\nimport { Shared as SecondShared } from './target';",
            ),
        ],
        "requester.ts",
        "target.ts",
        &["es5.d.ts"],
        |state, target_binder, target_idx| {
            state.ctx.share_owner_symbol_type_results = true;
            let requester_idx = state.ctx.current_file_idx;
            let first_alias = state
                .ctx
                .binder
                .file_locals
                .get("FirstShared")
                .expect("first import alias");
            let second_alias = state
                .ctx
                .binder
                .file_locals
                .get("SecondShared")
                .expect("second import alias");
            let target_sym = target_binder
                .file_locals
                .get("Shared")
                .expect("target alias");
            state
                .ctx
                .register_symbol_file_target(first_alias, requester_idx);
            state
                .ctx
                .register_symbol_file_target(second_alias, requester_idx);
            assert_eq!(
                state
                    .ctx
                    .resolve_import_target_from_file(requester_idx, "./target"),
                Some(target_idx),
                "fixture module resolution should find target.ts",
            );
            assert_eq!(
                state.resolve_cross_file_export_from_file(
                    "./target",
                    "Shared",
                    Some(requester_idx)
                ),
                Some(target_sym),
                "fixture export lookup should find Shared",
            );

            assert_eq!(
                state
                    .ctx
                    .cached_cross_file_symbol_type(target_sym, target_idx as u32),
                None,
                "target cache should start empty",
            );

            let (first_ty, first_params) = state
                .try_resolve_cross_arena_named_alias_without_child(first_alias)
                .expect("first import alias should resolve through the shortcut");
            assert_ne!(first_ty, TypeId::UNKNOWN);
            assert_ne!(first_ty, TypeId::ERROR);

            let (cached_target_ty, cached_target_params) = state
                .ctx
                .cached_cross_file_symbol_type(target_sym, target_idx as u32)
                .expect("direct-source target result should be cached for sibling import aliases");
            assert_eq!(cached_target_ty, first_ty);
            assert_eq!(cached_target_params.len(), first_params.len());
            assert_eq!(
                state
                    .ctx
                    .cached_cross_file_symbol_type(second_alias, requester_idx as u32),
                None,
                "second alias should not be cached before it is resolved",
            );

            let (second_ty, second_params) = state
                .try_resolve_cross_arena_named_alias_without_child(second_alias)
                .expect("second import alias should resolve through the shortcut");
            assert_eq!(second_ty, cached_target_ty);
            assert_eq!(second_params.len(), cached_target_params.len());
            assert_eq!(
                state
                    .ctx
                    .cached_cross_file_symbol_type(second_alias, requester_idx as u32)
                    .map(|(ty, params)| (ty, params.len())),
                Some((cached_target_ty, cached_target_params.len())),
                "second alias should cache its own alias entry after reusing the target",
            );
        },
    );
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
fn direct_source_file_type_alias_lowers_index_signature_type_literal_access() {
    with_two_file_state(
        "type Box<Value> = { value: Value };\nexport type DictionaryValue<Item> = { [key: string]: Box<Item> }[string];",
        "import { DictionaryValue } from './target';",
        |state, target_binder| {
            let dictionary_value_sym = target_binder
                .file_locals
                .get("DictionaryValue")
                .expect("DictionaryValue");
            let (ty, params) = state
                .direct_source_file_type_alias_result(dictionary_value_sym, Some(1), true)
                .expect(
                    "index-signature type literals with lowerable values should lower directly",
                );
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "DictionaryValue should expose Item");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_numeric_index_signature_type_literal_access() {
    with_two_file_state(
        "type Cell<Data> = { item: Data };\nexport type TableEntry<Row> = { [slot: number]: Cell<Row> }[number];",
        "import { TableEntry } from './target';",
        |state, target_binder| {
            let table_entry_sym = target_binder
                .file_locals
                .get("TableEntry")
                .expect("TableEntry");
            let (ty, params) = state
                .direct_source_file_type_alias_result(table_entry_sym, Some(1), true)
                .expect("renamed numeric index-signature type literals should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "TableEntry should expose Row");
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
fn direct_source_file_type_alias_rejects_index_signature_typeof_alias_application() {
    with_two_file_state(
        "const value = 1;\ntype Box<X> = X | typeof value;\nexport type DictionaryValue<Item> = { [key: string]: Box<Item> }[string];",
        "import { DictionaryValue } from './target';",
        |state, target_binder| {
            let dictionary_value_sym = target_binder
                .file_locals
                .get("DictionaryValue")
                .expect("DictionaryValue");
            assert!(
                state
                    .direct_source_file_type_alias_result(dictionary_value_sym, Some(1), true)
                    .is_none(),
                "flow-sensitive local alias applications in index-signature values must stay on the child-checker path",
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
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Leaf".to_string()));
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
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Wrap".to_string()));
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
        alias_symbol.set_import_module(Some("./target".to_string()));
        alias_symbol.set_import_name(Some("Flow".to_string()));
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
fn direct_source_file_type_alias_lowers_local_interface_application_body() {
    with_two_file_state(
        r#"
            interface Bucket<X> {
                value: X;
            }
            export type Result<T> = T extends Bucket<infer U> ? U : T;
        "#,
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (ty, params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("explicit local interface applications should lower directly");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert_eq!(params.len(), 1, "Result should preserve its type parameter");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_defaulted_local_interface_application() {
    with_two_file_state(
        r#"
            interface Bucket<X = string> {
                value: X;
            }
            export type Result = Bucket;
        "#,
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            assert!(
                state
                    .direct_source_file_type_alias_result(result_sym, Some(1), true)
                    .is_none(),
                "defaulted interface applications need type-param metadata before direct lowering",
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
fn direct_source_file_type_alias_lowers_guarded_recursive_json_aliases() {
    with_two_file_state(
        "type JsonPrimitive = null | number | string | boolean;\ntype JsonObject = { [Key in string]: JsonValue };\ntype JsonArray = JsonValue[] | readonly JsonValue[];\nexport type JsonValue = JsonPrimitive | JsonObject | JsonArray;",
        "import { JsonValue } from './target';",
        |state, target_binder| {
            let value_sym = target_binder
                .file_locals
                .get("JsonValue")
                .expect("JsonValue");
            let (value_ty, value_params) = state
                .direct_source_file_type_alias_result(value_sym, Some(1), true)
                .expect("guarded recursive JSON aliases should lower directly");
            assert_ne!(value_ty, TypeId::UNKNOWN);
            assert_ne!(value_ty, TypeId::ERROR);
            assert!(value_params.is_empty(), "JsonValue should be non-generic");

            let object_sym = target_binder
                .file_locals
                .get("JsonObject")
                .expect("JsonObject");
            let (object_ty, object_params) = state
                .direct_source_file_type_alias_result(object_sym, Some(1), true)
                .expect("mapped object aliases may guard recursive references");
            assert_ne!(object_ty, TypeId::UNKNOWN);
            assert_ne!(object_ty, TypeId::ERROR);
            assert!(object_params.is_empty(), "JsonObject should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_guarded_recursive_object_aliases() {
    with_two_file_state(
        "type Leaf = string;\ntype Node = Leaf | Branch;\ntype Branch = { next: Node };\nexport type Root = Node;",
        "import { Root } from './target';",
        |state, target_binder| {
            let root_sym = target_binder.file_locals.get("Root").expect("Root");
            let (root_ty, root_params) = state
                .direct_source_file_type_alias_result(root_sym, Some(1), true)
                .expect("object members structurally guard recursive aliases");
            assert_ne!(root_ty, TypeId::UNKNOWN);
            assert_ne!(root_ty, TypeId::ERROR);
            assert!(root_params.is_empty(), "Root should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_direct_self_object_alias() {
    with_two_file_state(
        "export type Node = { value: string; next?: Node };",
        "import { Node } from './target';",
        |state, target_binder| {
            let node_sym = target_binder.file_locals.get("Node").expect("Node");
            let (node_ty, node_params) = state
                .direct_source_file_type_alias_result(node_sym, Some(1), true)
                .expect("object members structurally guard direct self aliases");
            assert_ne!(node_ty, TypeId::UNKNOWN);
            assert_ne!(node_ty, TypeId::ERROR);
            assert!(node_params.is_empty(), "Node should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_direct_self_array_alias() {
    with_two_file_state(
        "export type Nested = string | Nested[];",
        "import { Nested } from './target';",
        |state, target_binder| {
            let nested_sym = target_binder.file_locals.get("Nested").expect("Nested");
            let (nested_ty, nested_params) = state
                .direct_source_file_type_alias_result(nested_sym, Some(1), true)
                .expect("array elements structurally guard direct self aliases");
            assert_ne!(nested_ty, TypeId::UNKNOWN);
            assert_ne!(nested_ty, TypeId::ERROR);
            assert!(nested_params.is_empty(), "Nested should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_generic_self_object_alias() {
    with_two_file_state(
        "export type Link<Value> = { value: Value; next?: Link<Value> };",
        "import { Link } from './target';",
        |state, target_binder| {
            let link_sym = target_binder.file_locals.get("Link").expect("Link");
            let (link_ty, link_params) = state
                .direct_source_file_type_alias_result(link_sym, Some(1), true)
                .expect("object members structurally guard generic self aliases");
            assert_ne!(link_ty, TypeId::UNKNOWN);
            assert_ne!(link_ty, TypeId::ERROR);
            assert_eq!(link_params.len(), 1, "Link should expose Value");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_generic_self_array_alias() {
    with_two_file_state(
        "export type Nested<Element> = Element | Nested<Element>[];",
        "import { Nested } from './target';",
        |state, target_binder| {
            let nested_sym = target_binder.file_locals.get("Nested").expect("Nested");
            let (nested_ty, nested_params) = state
                .direct_source_file_type_alias_result(nested_sym, Some(1), true)
                .expect("array elements structurally guard generic self aliases");
            assert_ne!(nested_ty, TypeId::UNKNOWN);
            assert_ne!(nested_ty, TypeId::ERROR);
            assert_eq!(nested_params.len(), 1, "Nested should expose Element");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_generic_self_function_alias() {
    with_two_file_state(
        "export type Step<Input> = (value: Input) => Step<Input>;",
        "import { Step } from './target';",
        |state, target_binder| {
            let step_sym = target_binder.file_locals.get("Step").expect("Step");
            let (step_ty, step_params) = state
                .direct_source_file_type_alias_result(step_sym, Some(1), true)
                .expect("function returns structurally guard generic self aliases");
            assert_ne!(step_ty, TypeId::UNKNOWN);
            assert_ne!(step_ty, TypeId::ERROR);
            assert_eq!(step_params.len(), 1, "Step should expose Input");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_renamed_guarded_function_helper_cycle() {
    with_two_file_state(
        "type Params<Callback extends (...args: any[]) => any> = Callback extends (...args: infer Values) => any ? Values : never;\ntype ResultOf<Callback extends (...args: any[]) => any> = Callback extends (...args: any[]) => infer Output ? Output : never;\ntype Fill<Values extends any[]> = Values;\nexport type Invoke<Callback extends (...args: any[]) => any> = <Provided extends Fill<Params<Callback>>>(...args: Provided) => Invoke<(...args: Provided) => ResultOf<Callback>>;",
        "import { Invoke } from './target';",
        |state, target_binder| {
            let invoke_sym = target_binder.file_locals.get("Invoke").expect("Invoke");
            let (invoke_ty, invoke_params) = state
                .direct_source_file_type_alias_result(invoke_sym, Some(1), true)
                .expect("function-local type params structurally guard helper cycles");
            assert_ne!(invoke_ty, TypeId::UNKNOWN);
            assert_ne!(invoke_ty, TypeId::ERROR);
            assert_eq!(invoke_params.len(), 1, "Invoke should expose Callback");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_guarded_generic_mapped_helper_cycle() {
    with_two_file_state(
        "type DeepObject<Input> = { [Field in keyof Input]: Deep<Input[Field]> };\nexport type Deep<Subject> = Subject extends object ? DeepObject<Subject> : Subject;",
        "import { Deep } from './target';",
        |state, target_binder| {
            let deep_sym = target_binder.file_locals.get("Deep").expect("Deep");
            let (deep_ty, deep_params) = state
                .direct_source_file_type_alias_result(deep_sym, Some(1), true)
                .expect("mapped outputs structurally guard generic helper cycles");
            assert_ne!(deep_ty, TypeId::UNKNOWN);
            assert_ne!(deep_ty, TypeId::ERROR);
            assert_eq!(deep_params.len(), 1, "Deep should expose Subject");
        },
    );
}

#[test]
fn direct_source_file_type_alias_lowers_tuple_tail_conditional_recursion() {
    with_two_file_state(
        "type AllTrue<Items> = Items extends [infer First, ...infer Tail] ? First extends true ? AllTrue<Tail> : false : true;\nexport type Result<Flags extends boolean[]> = AllTrue<Flags>;",
        "import { Result } from './target';",
        |state, target_binder| {
            let result_sym = target_binder.file_locals.get("Result").expect("Result");
            let (result_ty, result_params) = state
                .direct_source_file_type_alias_result(result_sym, Some(1), true)
                .expect("tuple rest inference structurally guards tail-recursive aliases");
            assert_ne!(result_ty, TypeId::UNKNOWN);
            assert_ne!(result_ty, TypeId::ERROR);
            assert_eq!(result_params.len(), 1, "Result should expose Flags");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_tuple_conditional_original_arg_recursion() {
    with_two_file_state(
        "export type Loop<Items> = Items extends [infer First, ...infer Tail] ? Loop<Items> : true;",
        "import { Loop } from './target';",
        |state, target_binder| {
            let loop_sym = target_binder.file_locals.get("Loop").expect("Loop");
            assert!(
                state
                    .direct_source_file_type_alias_result(loop_sym, Some(1), true)
                    .is_none(),
                "tuple-rest conditionals only guard recursive calls that consume the inferred tail",
            );
        },
    );
}

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
