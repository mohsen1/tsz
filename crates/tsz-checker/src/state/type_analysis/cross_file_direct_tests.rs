use super::{is_builtin_lib_file_name, is_external_package_declaration_file_name};
use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::perf_counters::{CrossArenaSymbolMissSource, DirectActualLibAliasBodyOutcome};
use tsz_parser::parser::{ParserState, syntax_kind_ext};
use tsz_solver::TypeId;

#[test]
fn direct_actual_lib_alias_admission_list_is_track7_ratchet() {
    const DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSION_CEILING: usize = 28;

    let admitted = super::DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSIONS;
    assert_eq!(
        admitted.len(),
        DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSION_CEILING,
        "Track 7 actual-lib alias admissions are transitional; replace \
         name-only admissions with stable lib identity queries before growing \
         this ceiling.",
    );
    assert!(
        admitted.windows(2).all(|pair| pair[0] < pair[1]),
        "Keep actual-lib alias admissions sorted so additions are reviewable: {admitted:?}",
    );
    for name in admitted {
        assert!(
            super::is_direct_actual_lib_alias_body_admitted(name),
            "{name} must be admitted by the shared classifier",
        );
    }
    for name in ["Array", "Date", "Iterator", "Promise", "ReadonlyArray"] {
        assert!(
            !super::is_direct_actual_lib_alias_body_admitted(name),
            "{name} is an interface/value helper, not a type-alias body admission",
        );
    }
}

fn parse_interface_declarations(
    source: &str,
) -> (
    tsz_parser::parser::node::NodeArena,
    Vec<tsz_parser::NodeIndex>,
) {
    let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena().clone();
    let source_file = arena
        .get_source_file_at(root)
        .expect("source file should parse");
    let declarations = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .filter(|idx| {
            arena
                .get(*idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
        })
        .collect();
    (arena, declarations)
}

fn parse_bound_source(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    parse_bound_source_with_name("fixture.ts", source)
}

fn parse_bound_source_with_name(
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

fn interface_declarations_in_arena(
    arena: &tsz_parser::parser::node::NodeArena,
) -> Vec<tsz_parser::NodeIndex> {
    arena
        .source_files
        .first()
        .expect("source file should parse")
        .statements
        .nodes
        .iter()
        .copied()
        .filter(|idx| {
            arena
                .get(*idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::INTERFACE_DECLARATION)
        })
        .collect()
}

fn interface_member_by_name(
    arena: &tsz_parser::parser::node::NodeArena,
    interface_idx: tsz_parser::NodeIndex,
    member_name: &str,
) -> tsz_parser::NodeIndex {
    let interface = arena
        .get(interface_idx)
        .and_then(|node| arena.get_interface(node))
        .expect("interface declaration");
    interface
        .members
        .nodes
        .iter()
        .copied()
        .find(|&member_idx| {
            let Some(member_node) = arena.get(member_idx) else {
                return false;
            };
            let name_idx = arena
                .get_signature(member_node)
                .map(|signature| signature.name);
            name_idx
                .and_then(|idx| {
                    crate::types_domain::queries::core::get_literal_property_name(arena, idx)
                })
                .is_some_and(|name| name == member_name)
        })
        .unwrap_or_else(|| panic!("member {member_name:?} not found"))
}

#[test]
fn detects_npm_and_source_tree_builtin_lib_names() {
    assert!(is_builtin_lib_file_name("lib.es2024.d.ts"));
    assert!(is_builtin_lib_file_name("lib.dom.d.ts"));
    assert!(is_builtin_lib_file_name("es2024.d.ts"));
    assert!(is_builtin_lib_file_name("es2024.full.d.ts"));
    assert!(is_builtin_lib_file_name("dom.generated.d.ts"));
    assert!(is_builtin_lib_file_name("dom.iterable.generated.d.ts"));
    assert!(is_builtin_lib_file_name("webworker.asynciterable.d.ts"));
    assert!(is_builtin_lib_file_name("decorators.legacy.d.ts"));
}

#[test]
fn does_not_treat_arbitrary_declaration_files_as_builtin_libs() {
    assert!(!is_builtin_lib_file_name("react/index.d.ts"));
    assert!(!is_builtin_lib_file_name(
        "node_modules/@types/node/fs.d.ts"
    ));
    assert!(!is_builtin_lib_file_name("packages/foo/src/types.d.ts"));
}

#[test]
fn detects_external_package_declaration_paths() {
    assert!(is_external_package_declaration_file_name(
        "node_modules/react/index.d.ts"
    ));
    assert!(is_external_package_declaration_file_name(
        "/repo/node_modules/@types/node/fs.d.ts"
    ));
    assert!(is_external_package_declaration_file_name(
        r"C:\repo\node_modules\@types\node\fs.d.ts"
    ));
}

#[test]
fn does_not_treat_local_declaration_paths_as_external_packages() {
    assert!(!is_external_package_declaration_file_name(
        "packages/foo/src/types.d.ts"
    ));
    assert!(!is_external_package_declaration_file_name(
        "/repo/fixtures/node-modules-like/types.d.ts"
    ));
}

#[test]
fn source_file_direct_interface_lowering_accepts_scope_independent_members() {
    let (arena, binder, _types) = parse_bound_source(
        r#"
                interface Leaf {
                    value: number;
                    tag: "leaf";
                    flags: true | false;
                }
            "#,
    );
    let declarations = interface_declarations_in_arena(arena.as_ref());
    let declarations = vec![(declarations[0], arena.as_ref())];

    assert!(
        CheckerState::source_file_interface_declarations_are_direct_lowerable(
            &declarations,
            binder.as_ref(),
        )
    );
}

#[test]
fn source_file_direct_interface_lowering_rejects_scope_dependent_members() {
    let (arena, binder, _types) = parse_bound_source(
        r#"
                class Local { value: number; }
                interface UsesLocal { value: Local; }
            "#,
    );
    let declarations = interface_declarations_in_arena(arena.as_ref());
    let declarations = vec![(declarations[0], arena.as_ref())];

    assert!(
        !CheckerState::source_file_interface_declarations_are_direct_lowerable(
            &declarations,
            binder.as_ref(),
        )
    );
}

#[test]
fn source_file_direct_interface_lowering_accepts_sibling_option_bag_refs() {
    let (arena, binder, _types) = parse_bound_source(
        r#"
                type Mode = "lookup" | "best fit";
                interface Nested { enabled: boolean; }
                interface Options {
                    mode?: Mode;
                    nested: Nested;
                    modes: ReadonlyArray<Mode>;
                }
            "#,
    );
    let declarations = interface_declarations_in_arena(arena.as_ref());
    let declarations = vec![(declarations[1], arena.as_ref())];

    assert!(
        CheckerState::source_file_interface_declarations_are_direct_lowerable(
            &declarations,
            binder.as_ref(),
        )
    );
}

#[test]
fn direct_cross_file_interface_lowering_accepts_source_option_bag_sibling_refs() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                type Mode = "lookup" | "best fit";
                interface Nested { enabled: boolean; }
                interface Options {
                    mode?: Mode;
                    nested: Nested;
                    modes: ReadonlyArray<Mode>;
                }
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let options_sym = binder.file_locals.get("Options").expect("Options symbol");

    let (options_type, params) = state
        .direct_cross_file_interface_lowering(
            options_sym,
            binder.as_ref(),
            arena.as_ref(),
            false,
            true,
        )
        .expect("option-bag interface with direct-lowerable siblings should lower directly");

    assert_ne!(options_type, TypeId::UNKNOWN);
    assert_ne!(options_type, TypeId::ERROR);
    assert!(params.is_empty());
    let nested = types.intern_string("nested");
    let nested_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        options_type,
        nested,
    )
    .expect("directly lowered option bag should retain sibling interface properties");
    let resolved_nested = state.resolve_lazy_type(nested_type);
    let enabled = state.ctx.types.intern_string("enabled");
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            resolved_nested,
            enabled,
        )
        .is_some(),
        "sibling interface property should resolve through the lazy DefId"
    );
}

#[test]
fn direct_cross_file_interface_lowering_resolves_siblings_in_delegate_file() {
    let (target_arena, target_binder, types) = parse_bound_source(
        r#"
                type Padding = 0;
                type Mode = "target";
                interface Options { mode: Mode; }
            "#,
    );
    let (requester_arena, requester_binder, _) = parse_bound_source(
        r#"
                type Mode = "requester";
                import { Options } from "./target";
            "#,
    );
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
    state.ctx.set_current_file_idx(0);

    let options_sym = target_binder
        .file_locals
        .get("Options")
        .expect("Options symbol");
    let target_mode_sym = target_binder
        .file_locals
        .get("Mode")
        .expect("target Mode symbol");
    let requester_mode_sym = requester_binder
        .file_locals
        .get("Mode")
        .expect("requester Mode symbol");
    assert_ne!(
        target_mode_sym, requester_mode_sym,
        "fixture should use distinct raw SymbolIds so DefId resolution can distinguish files",
    );

    let (options_type, _params) = state
        .direct_cross_file_interface_lowering(
            options_sym,
            target_binder.as_ref(),
            target_arena.as_ref(),
            false,
            true,
        )
        .expect("target option-bag interface should lower directly");
    let mode = types.intern_string("mode");
    let mode_type = crate::query_boundaries::common::raw_property_type(
        state.ctx.types.as_type_database(),
        options_type,
        mode,
    )
    .expect("mode property should lower");
    let target_mode_def = state.ctx.get_or_create_def_id(target_mode_sym);

    assert_eq!(
        crate::query_boundaries::common::lazy_def_id(&types, mode_type),
        Some(target_mode_def),
        "source-file direct lowering should resolve sibling type references in the delegate file",
    );
}

#[test]
fn source_file_direct_interface_lowering_rejects_generic_sibling_refs() {
    let (arena, binder, _types) = parse_bound_source(
        r#"
                type Box<T> = T;
                interface Options { value: Box<string>; }
            "#,
    );
    let declarations = interface_declarations_in_arena(arena.as_ref());
    let declarations = vec![(declarations[0], arena.as_ref())];

    assert!(
        !CheckerState::source_file_interface_declarations_are_direct_lowerable(
            &declarations,
            binder.as_ref(),
        )
    );
}

#[test]
fn source_file_direct_interface_lowering_rejects_sibling_cycles() {
    let (arena, binder, _types) = parse_bound_source(
        r#"
                interface Parent { child: Child; }
                interface Child { parent: Parent; }
            "#,
    );
    let declarations = interface_declarations_in_arena(arena.as_ref());
    let declarations = vec![(declarations[0], arena.as_ref())];

    assert!(
        !CheckerState::source_file_interface_declarations_are_direct_lowerable(
            &declarations,
            binder.as_ref(),
        )
    );
}

#[test]
fn direct_source_file_variable_annotation_accepts_same_file_simple_interface() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Leaf { value: number; tag: "leaf"; }
                const leaf: Leaf = { value: 1, tag: "leaf" };
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let leaf_sym = binder.file_locals.get("leaf").expect("leaf symbol");

    let result = state
        .direct_source_file_variable_annotation_type(
            leaf_sym,
            binder.as_ref(),
            arena.as_ref(),
            true,
        )
        .expect("simple same-file interface annotation should lower directly");

    assert!(
        crate::query_boundaries::common::is_lazy_type(&types, result),
        "variable annotation should preserve the interface lazy type"
    );
}

#[test]
fn direct_source_file_variable_annotation_rejects_type_alias_reference() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                type Leaf = { value: number };
                const leaf: Leaf = { value: 1 };
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    let leaf_sym = binder.file_locals.get("leaf").expect("leaf symbol");

    assert!(
        state
            .direct_source_file_variable_annotation_type(
                leaf_sym,
                binder.as_ref(),
                arena.as_ref(),
                true,
            )
            .is_none(),
    );
}

#[test]
fn direct_source_file_type_alias_lowers_scope_independent_alias_body() {
    let (target_arena, target_binder, types) = parse_bound_source(
        r#"
                export type Leaf = string | number;
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Leaf } from './target';");
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
    let leaf_sym = target_binder.file_locals.get("Leaf").expect("Leaf symbol");

    let (ty, params) = state
        .direct_source_file_type_alias_result(leaf_sym, Some(1), true)
        .expect("source-file alias should lower without a child checker");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "Leaf should be non-generic");
    let def_id = state
        .ctx
        .get_existing_def_id(leaf_sym)
        .expect("alias DefId should be registered");
    assert_eq!(
        state.ctx.definition_store.get_body(def_id),
        Some(ty),
        "alias body should be registered for lazy resolution",
    );
    assert_eq!(
        state
            .ctx
            .get_def_type_params(def_id)
            .unwrap_or_default()
            .len(),
        params.len(),
        "alias type parameters should be available from the definition store",
    );
}

#[test]
fn direct_source_file_type_alias_rejects_complex_generic_typeof_and_self_references() {
    let (generic_arena, generic_binder, types) = parse_bound_source(
        r#"
                type Maybe<X> = X | null;
                export type Box<T> = { value: Maybe<T> };
                export type Wrapped = Maybe<string>;
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Box } from './target';");
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
        Arc::clone(&generic_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&generic_binder),
    ]));
    let box_sym = generic_binder.file_locals.get("Box").expect("Box symbol");
    assert!(
        state
            .direct_source_file_type_alias_result(box_sym, Some(1), true)
            .is_none(),
        "generic/alias-dependent source aliases stay on the child-checker path",
    );
    let wrapped_sym = generic_binder
        .file_locals
        .get("Wrapped")
        .expect("Wrapped symbol");
    assert!(
        state
            .direct_source_file_type_alias_result(wrapped_sym, Some(1), true)
            .is_none(),
        "alias-dependent source aliases stay on the child-checker path",
    );

    let (typeof_arena, typeof_binder, types) = parse_bound_source(
        r#"
                const value = 1;
                export type FromValue = typeof value;
            "#,
    );
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
        Arc::clone(&typeof_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&typeof_binder),
    ]));
    let from_value = typeof_binder
        .file_locals
        .get("FromValue")
        .expect("FromValue symbol");

    assert!(
        state
            .direct_source_file_type_alias_result(from_value, Some(1), true,)
            .is_none(),
        "`typeof` aliases need the child-checker path",
    );

    let (loop_arena, loop_binder, _) = parse_bound_source("export type Loop = Loop;");
    state.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&requester_arena),
        Arc::clone(&typeof_arena),
        Arc::clone(&loop_arena),
    ]));
    state.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&requester_binder),
        Arc::clone(&typeof_binder),
        Arc::clone(&loop_binder),
    ]));
    let loop_sym = loop_binder.file_locals.get("Loop").expect("Loop symbol");
    assert!(
        state
            .direct_source_file_type_alias_result(loop_sym, Some(2), true,)
            .is_none(),
        "self references need the child-checker circularity path",
    );
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
fn direct_source_file_type_alias_lowers_chain_with_union_of_local_refs() {
    with_two_file_state(
        "type Str = string;\ntype Num = number;\nexport type Both = Str | Num;",
        "import { Both } from './target';",
        |state, target_binder| {
            let both_sym = target_binder.file_locals.get("Both").expect("Both");
            let (ty, params) = state
                .direct_source_file_type_alias_result(both_sym, Some(1), true)
                .expect("union of local alias refs must lower without a child checker");
            assert_ne!(ty, TypeId::UNKNOWN);
            assert_ne!(ty, TypeId::ERROR);
            assert!(params.is_empty(), "Both should be non-generic");
        },
    );
}

#[test]
fn direct_source_file_type_alias_rejects_chain_with_type_args() {
    with_two_file_state(
        "type Wrap<T> = T | null;\nexport type Concrete = Wrap<string>;",
        "import { Concrete } from './target';",
        |state, target_binder| {
            let concrete_sym = target_binder.file_locals.get("Concrete").expect("Concrete");
            assert!(
                state
                    .direct_source_file_type_alias_result(concrete_sym, Some(1), true)
                    .is_none(),
                "chain with type arguments must stay on the child-checker path",
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
fn direct_interface_member_simple_type_substitutes_source_type_params() {
    let (target_arena, target_binder, types) = parse_bound_source(
        r#"
                export interface Box<T> {
                    value: T;
                    optional?: T;
                }
            "#,
    );
    let (requester_arena, requester_binder, _) =
        parse_bound_source("import { Box } from './target';");
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

    let box_sym = target_binder.file_locals.get("Box").expect("Box symbol");
    let box_decl = target_binder
        .get_symbol(box_sym)
        .expect("Box symbol data")
        .declarations[0];
    let value_member = interface_member_by_name(target_arena.as_ref(), box_decl, "value");
    let optional_member = interface_member_by_name(target_arena.as_ref(), box_decl, "optional");

    let results = state
        .direct_cross_file_interface_member_simple_types(
            box_decl,
            &[value_member, optional_member],
            target_arena.as_ref(),
            target_binder.as_ref(),
            Some(&[TypeId::STRING]),
            true,
        )
        .expect("source interface members should lower directly");

    assert_eq!(results.get(&value_member).copied(), Some(TypeId::STRING));
    let optional_type = results
        .get(&optional_member)
        .copied()
        .expect("optional member result");
    assert!(
        crate::query_boundaries::common::is_union_type(&types, optional_type),
        "optional member should include undefined in a union",
    );
}

#[test]
fn direct_interface_member_simple_type_lowers_builtin_property() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let lib = &lib_files[0];
    let array_sym = lib
        .binder
        .file_locals
        .get("Array")
        .expect("Array should resolve in es5 lib");
    let array_decl = lib
        .binder
        .get_symbol(array_sym)
        .expect("Array symbol data")
        .declarations
        .iter()
        .copied()
        .find(|&decl_idx| {
            lib.arena
                .get(decl_idx)
                .and_then(|node| lib.arena.get_interface(node))
                .is_some()
        })
        .expect("Array interface declaration");
    let length_member = interface_member_by_name(lib.arena.as_ref(), array_decl, "length");

    let results = state
        .direct_cross_file_interface_member_simple_types(
            array_decl,
            &[length_member],
            lib.arena.as_ref(),
            lib.binder.as_ref(),
            None,
            false,
        )
        .expect("builtin interface member should lower directly");

    assert_eq!(results.get(&length_member).copied(), Some(TypeId::NUMBER));
}

#[test]
fn resolves_intl_namespace_exported_lib_interface_directly() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());
    let sym_id = state
        .resolve_lib_namespace_export_symbol("Intl", "CollatorOptions")
        .expect("Intl.CollatorOptions export should resolve");

    let ty = state
        .resolve_lib_interface_type_by_symbol("Intl.CollatorOptions", sym_id)
        .expect("Intl.CollatorOptions should lower directly");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
}

#[test]
fn direct_actual_lib_delegation_cache_preserves_type_params() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("ArrayIterator")
        .expect("ArrayIterator should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("ArrayIterator should lower through the direct lib path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "ArrayIterator should expose T");

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(sym_id)
        .expect("direct lib path should populate the delegation cache");
    assert_eq!(cached_ty, ty);
    assert_eq!(
        cached_params.len(),
        params.len(),
        "cache hits must preserve generic application metadata",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_selected_value_interfaces() {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2018.intl.d.ts",
        "es2020.intl.d.ts",
        "es2023.intl.d.ts",
    ]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    for (name, expected_params) in [
        ("Array", 1),
        ("Date", 0),
        ("DateTimeFormatOptions", 0),
        ("Error", 0),
        ("Function", 0),
        ("Locale", 0),
        ("Map", 2),
        ("NumberFormatOptions", 0),
        ("NumberFormatOptionsCurrencyDisplayRegistry", 0),
        ("NumberFormatOptionsSignDisplayRegistry", 0),
        ("NumberFormatOptionsStyleRegistry", 0),
        ("NumberFormatOptionsUseGroupingRegistry", 0),
        ("NumberFormatPartTypeRegistry", 0),
        ("NumberFormatRangePartTypeRegistry", 0),
        ("Object", 0),
        ("Promise", 1),
        ("RegExp", 0),
        ("Set", 1),
        ("Symbol", 0),
        ("WeakMap", 2),
        ("WeakSet", 1),
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .or_else(|| state.resolve_lib_namespace_export_symbol("Intl", name))
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        let Some((ty, params)) = state.direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        ) else {
            panic!("{name} should lower directly");
        };

        assert_ne!(ty, TypeId::UNKNOWN, "{name} must not lower to unknown");
        assert_ne!(ty, TypeId::ERROR, "{name} must not lower to error");
        if name == "ResolvedNumberFormatOptions" {
            let notation = state.ctx.types.intern_string("notation");
            assert!(
                crate::query_boundaries::common::raw_property_type(
                    state.ctx.types.as_type_database(),
                    ty,
                    notation,
                )
                .is_some(),
                "Intl.ResolvedNumberFormatOptions should include es2020 merged members",
            );
        }
        if name == "NumberFormat" {
            let result = state.resolve_property_access_with_env(ty, "resolvedOptions");
            let tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id: method_type,
                ..
            } = result
            else {
                panic!("Intl.NumberFormat.resolvedOptions should resolve, got {result:?}");
            };
            let return_type = crate::query_boundaries::common::return_type_for_type(
                state.ctx.types.as_type_database(),
                method_type,
            )
            .expect("resolvedOptions should have a return type");
            let resolved_return = state.resolve_lazy_type(return_type);
            let notation = state.ctx.types.intern_string("notation");
            assert!(
                crate::query_boundaries::common::raw_property_type(
                    state.ctx.types.as_type_database(),
                    resolved_return,
                    notation,
                )
                .is_some(),
                "Intl.NumberFormat.resolvedOptions should return merged ResolvedNumberFormatOptions",
            );
        }
        if name == "NumberFormatConstructor" {
            let instance_type = crate::query_boundaries::common::construct_return_type_for_type(
                state.ctx.types.as_type_database(),
                ty,
            )
            .expect("Intl.NumberFormatConstructor should construct NumberFormat");
            let resolved_instance = state.resolve_lazy_type(instance_type);
            let result =
                state.resolve_property_access_with_env(resolved_instance, "resolvedOptions");
            let tsz_solver::operations::property::PropertyAccessResult::Success {
                type_id: method_type,
                ..
            } = result
            else {
                panic!(
                    "constructed Intl.NumberFormat.resolvedOptions should resolve, got {result:?}"
                );
            };
            let return_type = crate::query_boundaries::common::return_type_for_type(
                state.ctx.types.as_type_database(),
                method_type,
            )
            .expect("constructed resolvedOptions should have a return type");
            let resolved_return = state.resolve_lazy_type(return_type);
            let notation = state.ctx.types.intern_string("notation");
            assert!(
                crate::query_boundaries::common::raw_property_type(
                    state.ctx.types.as_type_database(),
                    resolved_return,
                    notation,
                )
                .is_some(),
                "constructed Intl.NumberFormat.resolvedOptions should return merged ResolvedNumberFormatOptions",
            );
        }
        if name == "Locale" {
            let calendar = state.ctx.types.intern_string("calendar");
            assert!(
                crate::query_boundaries::common::raw_property_type(
                    state.ctx.types.as_type_database(),
                    ty,
                    calendar,
                )
                .is_some(),
                "Intl.Locale should include inherited LocaleOptions members",
            );
        }
        assert_eq!(
            params.len(),
            expected_params,
            "{name} should preserve type-parameter arity",
        );
        assert!(
            state.ctx.lib_delegation_cache.contains_symbol_type(sym_id),
            "{name} should populate the delegation cache",
        );
    }
}

#[test]
fn direct_cross_file_interface_lowering_handles_simple_builtin_dom_interfaces() {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let simple_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PaymentCurrencyAmount")
        .expect("PaymentCurrencyAmount should resolve to a dom lib symbol");
    let simple_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&simple_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("PaymentCurrencyAmount should have a delegate arena");
    let (simple_ty, simple_params) = state
        .direct_cross_file_interface_lowering(
            simple_sym_id,
            state.ctx.binder,
            simple_arena,
            false,
            false,
        )
        .expect("simple builtin dom interface should lower directly");
    assert_ne!(simple_ty, TypeId::UNKNOWN);
    assert_ne!(simple_ty, TypeId::ERROR);
    assert!(simple_params.is_empty());

    let heritage_sym_id = state
        .ctx
        .binder
        .file_locals
        .get("AddEventListenerOptions")
        .expect("AddEventListenerOptions should resolve to a dom lib symbol");
    let heritage_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&heritage_sym_id)
        .map(std::convert::AsRef::as_ref)
        .expect("AddEventListenerOptions should have a delegate arena");
    assert!(
        state
            .direct_cross_file_interface_lowering(
                heritage_sym_id,
                state.ctx.binder,
                heritage_arena,
                false,
                false,
            )
            .is_none(),
        "builtin dom interfaces with heritage stay on the fallback path",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_iterator_interfaces_with_params() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts", "esnext.iterator.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    for name in ["Iterator", "IteratorObject"] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);
        let symbol = state
            .ctx
            .binder
            .get_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should exist"))
            .clone();
        let protocol_method_interface = state.symbol_declares_direct_actual_lib_protocol_method(
            sym_id,
            &symbol,
            delegate_arena.expect("generic lib symbol should have a delegate arena"),
        );
        assert!(
            state.symbol_has_direct_actual_lib_interface_type_parameters(sym_id, &symbol),
            "{name} should expose actual-lib interface type parameters",
        );

        let lowered = state.direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        );
        if name == "Iterator" || protocol_method_interface {
            let (ty, params) = lowered
                .unwrap_or_else(|| panic!("{name} should lower through the direct lib path"));
            assert_ne!(ty, TypeId::UNKNOWN, "{name} should not lower to UNKNOWN");
            assert_ne!(ty, TypeId::ERROR, "{name} should not lower to ERROR");
            assert!(
                !params.is_empty(),
                "{name} should preserve generic type parameters",
            );
        } else {
            assert!(
                lowered.is_none(),
                "{name} should fall back instead of directly lowering a generic actual-lib interface",
            );
        }
    }
}

#[test]
fn direct_actual_lib_symbol_type_allows_iterator_without_declaration_arena_proof() {
    let lib_files = load_lib_files(&["es2015.iterable.d.ts", "esnext.iterator.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let iterator_sym_id = binder
        .file_locals
        .get("Iterator")
        .expect("Iterator should resolve to a lib symbol");
    let iterator_decls = binder
        .get_symbol(iterator_sym_id)
        .expect("Iterator symbol should exist")
        .declarations
        .clone();
    let declaration_arenas = std::sync::Arc::make_mut(&mut binder.declaration_arenas);
    for decl_idx in iterator_decls {
        declaration_arenas.remove(&(iterator_sym_id, decl_idx));
    }

    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&iterator_sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            iterator_sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Iterator should still lower through the direct lib path");

    assert_ne!(ty, TypeId::UNKNOWN, "Iterator should not lower to UNKNOWN");
    assert_ne!(ty, TypeId::ERROR, "Iterator should not lower to ERROR");
    assert!(
        !params.is_empty(),
        "Iterator should preserve generic type parameters",
    );
}

#[test]
fn direct_actual_lib_symbol_type_handles_non_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts", "decorators.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("DecoratorMetadataObject")
        .expect("DecoratorMetadataObject should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("non-generic actual-lib alias body should lower directly");

    assert!(
        params.is_empty(),
        "DecoratorMetadataObject should be non-generic",
    );
    assert!(
        crate::query_boundaries::common::lazy_def_id(&types, ty).is_none(),
        "direct alias result should return the registered alias body, not the opaque Lazy alias",
    );

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(cached_ty, ty);
    assert!(cached_params.is_empty());
}

#[test]
fn direct_actual_lib_symbol_type_handles_property_key_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("PropertyKey")
        .expect("PropertyKey should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);
    let symbol = state
        .get_cross_file_symbol(sym_id)
        .expect("PropertyKey symbol should be available")
        .clone();

    let proof = state
        .direct_actual_lib_type_alias_body(
            sym_id,
            &symbol,
            "PropertyKey",
            delegate_arena.expect("PropertyKey should have a delegate arena"),
        )
        .expect("PropertyKey should have a proven actual-lib alias body");
    assert_eq!(proof.outcome, DirectActualLibAliasBodyOutcome::Success);
    assert!(proof.type_params.is_empty(), "PropertyKey is non-generic",);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("PropertyKey should lower through the direct alias body path");
    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty(), "PropertyKey should remain non-generic");
}

#[test]
fn direct_actual_lib_symbol_type_handles_record_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Record")
        .expect("Record should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Record should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 2, "Record should expose K and T");
}

#[test]
fn direct_actual_lib_symbol_type_handles_intl_non_generic_alias_bodies() {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2018.intl.d.ts",
        "es2020.intl.d.ts",
        "es2023.intl.d.ts",
    ]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    for name in [
        "LocalesArgument",
        "NumberFormatOptionsCurrencyDisplay",
        "NumberFormatOptionsSignDisplay",
        "NumberFormatOptionsStyle",
        "NumberFormatOptionsUseGrouping",
        "NumberFormatPartTypes",
        "NumberFormatRangePartTypes",
        "UnicodeBCP47LocaleIdentifier",
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .or_else(|| state.resolve_lib_namespace_export_symbol("Intl", name))
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(
            proof.outcome,
            DirectActualLibAliasBodyOutcome::Success,
            "{name} should be admitted in the direct alias allowlist",
        );
        assert!(
            proof.type_params.is_empty(),
            "{name} should remain non-generic",
        );

        let (direct_ty, direct_params) = state
            .direct_actual_lib_symbol_type(
                sym_id,
                CrossArenaSymbolMissSource::SymbolArena,
                Some(delegate_arena),
                false,
            )
            .unwrap_or_else(|| panic!("{name} should lower through direct alias path"));
        assert_ne!(
            direct_ty,
            TypeId::UNKNOWN,
            "{name} should not lower to UNKNOWN"
        );
        assert_ne!(direct_ty, TypeId::ERROR, "{name} should not lower to ERROR");
        assert!(
            direct_params.is_empty(),
            "{name} should stay non-generic on direct path",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            direct_ty, fallback_body,
            "{name} direct alias body must match child-checker fallback body",
        );
        assert!(
            fallback_params.is_empty(),
            "{name} fallback should remain non-generic",
        );
    }
}

#[test]
fn direct_actual_lib_symbol_type_handles_readonly_generic_alias_body_query() {
    let lib_files = load_lib_files(&["es5.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    let sym_id = state
        .ctx
        .binder
        .file_locals
        .get("Readonly")
        .expect("Readonly should resolve to a lib symbol");
    let delegate_arena = state
        .ctx
        .binder
        .symbol_arenas
        .get(&sym_id)
        .map(std::convert::AsRef::as_ref);

    let (ty, params) = state
        .direct_actual_lib_symbol_type(
            sym_id,
            CrossArenaSymbolMissSource::SymbolArena,
            delegate_arena,
            false,
        )
        .expect("Readonly should lower through the direct alias body path");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert_eq!(params.len(), 1, "Readonly should expose T");

    let (cached_ty, cached_params) = state
        .ctx
        .lib_delegation_cache
        .symbol_type(sym_id)
        .expect("direct alias path should populate the delegation cache");
    assert_eq!(cached_ty, ty);
    assert_eq!(
        cached_params.len(),
        params.len(),
        "cache hits must preserve generic alias metadata",
    );

    let (cached_result_ty, cached_result_params) = state
        .delegate_cross_arena_symbol_resolution(sym_id)
        .expect("Readonly cache hit should resolve through lib delegation cache");
    assert_eq!(cached_result_ty, ty);
    assert_eq!(
        cached_result_params.len(),
        params.len(),
        "Readonly cache hits must return the cached alias type params",
    );
}

#[test]
fn direct_actual_lib_alias_proof_matches_mapped_utility_fallback_bodies() {
    let lib_files = load_lib_files(&["es5.d.ts", "es2015.iterable.d.ts", "es2019.array.d.ts"]);
    let mut parser = ParserState::new("fixture.ts".to_string(), "let value;".to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let types = TypeInterner::new();
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    state.ctx.set_lib_contexts(lib_contexts);
    state.ctx.set_actual_lib_file_count(lib_files.len());

    for (name, expected_param_count, expected_outcome) in [
        ("Capitalize", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Exclude", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Extract", 2, DirectActualLibAliasBodyOutcome::Success),
        ("FlatArray", 2, DirectActualLibAliasBodyOutcome::Success),
        (
            "IteratorResult",
            2,
            DirectActualLibAliasBodyOutcome::Success,
        ),
        ("Lowercase", 1, DirectActualLibAliasBodyOutcome::Success),
        ("NonNullable", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Omit", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Partial", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Pick", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Record", 2, DirectActualLibAliasBodyOutcome::Success),
        ("Readonly", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Required", 1, DirectActualLibAliasBodyOutcome::Success),
        ("ReturnType", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Uncapitalize", 1, DirectActualLibAliasBodyOutcome::Success),
        ("Uppercase", 1, DirectActualLibAliasBodyOutcome::Success),
        ("WeakKey", 0, DirectActualLibAliasBodyOutcome::Success),
    ] {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let delegate_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .unwrap_or_else(|| panic!("{name} should have a delegate arena"));
        let symbol = state
            .get_cross_file_symbol(sym_id)
            .unwrap_or_else(|| panic!("{name} symbol should be available"))
            .clone();

        let proof = state
            .direct_actual_lib_type_alias_body(sym_id, &symbol, name, delegate_arena)
            .unwrap_or_else(|| panic!("{name} should have a proven actual-lib alias body"));
        assert_eq!(proof.outcome, expected_outcome);
        assert_eq!(
            proof.type_params.len(),
            expected_param_count,
            "{name} should expose its declared type params",
        );

        let (fallback_body, fallback_params) = state.compute_type_of_symbol(sym_id);
        assert_eq!(
            fallback_body, proof.body,
            "{name} proof must match the existing child-checker fallback body",
        );
        assert_eq!(
            fallback_params.len(),
            proof.type_params.len(),
            "{name} proof must preserve the same type-parameter arity as fallback",
        );
    }
}
