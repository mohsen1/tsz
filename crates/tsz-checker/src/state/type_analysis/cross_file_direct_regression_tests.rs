use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{ParserState, syntax_kind_ext};
use tsz_solver::TypeId;

fn parse_bound_source_with_name(
    file_name: &str,
    source: &str,
) -> (Arc<NodeArena>, Arc<BinderState>, TypeInterner) {
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

fn interface_declarations_in_arena(arena: &NodeArena) -> Vec<NodeIndex> {
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

#[test]
fn lib_interface_resolution_ignores_unrelated_local_node_collision() {
    let lib_files = load_lib_files(&["dom.d.ts"]);
    assert_eq!(lib_files.len(), 1, "fixture needs the DOM lib");
    let mut parser = ParserState::new(
        "fixture.ts".to_string(),
        r#"
            interface Noise { sentinel: true; }
            let value: Event;
        "#
        .to_string(),
    );
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    let arena = Arc::new(parser.get_arena().clone());
    let noise_decl = interface_declarations_in_arena(arena.as_ref())
        .into_iter()
        .next()
        .expect("Noise interface should parse");
    let event_sym = binder
        .file_locals
        .get("Event")
        .expect("DOM Event should resolve to a lib symbol");
    let event_arena = binder
        .symbol_arenas
        .get(&event_sym)
        .cloned()
        .unwrap_or_else(|| Arc::clone(&lib_files[0].arena));
    let original_decls = binder
        .get_symbol(event_sym)
        .expect("Event symbol should exist")
        .declarations
        .clone();
    binder
        .symbols
        .get_mut(event_sym)
        .expect("Event symbol should be mutable")
        .declarations = vec![noise_decl];
    Arc::make_mut(&mut binder.symbol_arenas).insert(event_sym, event_arena);
    let declaration_arenas = Arc::make_mut(&mut binder.declaration_arenas);
    for decl_idx in original_decls {
        declaration_arenas.remove(&(event_sym, decl_idx));
    }
    declaration_arenas.remove(&(event_sym, noise_decl));

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

    let event_type = state.get_type_of_symbol(event_sym);
    let ty = state.resolve_lazy_type(event_type);
    let expected_event = state
        .resolve_lib_type_by_name("Event")
        .expect("DOM Event should resolve through lib lookup");
    let sentinel = types.intern_string("sentinel");

    assert_eq!(
        ty, expected_event,
        "same-index collision should still resolve through the canonical DOM Event type",
    );
    assert!(
        crate::query_boundaries::common::raw_property_type(
            state.ctx.types.as_type_database(),
            ty,
            sentinel,
        )
        .is_none(),
        "unrelated current-arena interface members must not augment the lib symbol",
    );
}

#[test]
fn direct_declaration_file_type_alias_lowers_guarded_recursive_bodies() {
    let (arena, binder, types) = parse_bound_source_with_name(
        "node_modules/pkg/index.d.ts",
        r#"
                export type JsonAtom = string | number | JsonAtom[];
                export type CssToken = string | readonly CssToken[];
                export type Loop = Loop;
            "#,
    );
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "requester.ts".to_string(),
        CheckerOptions::default(),
    );
    let mut state = CheckerState { ctx };

    for name in ["JsonAtom", "CssToken"] {
        let sym_id = binder
            .file_locals
            .get(name)
            .expect("recursive alias symbol");
        let (ty, params) = state
            .direct_declaration_file_type_alias_result(sym_id, arena.as_ref())
            .expect("guarded recursive declaration alias should lower directly");
        assert!(params.is_empty(), "{name} should be non-generic");
        assert_ne!(ty, TypeId::UNKNOWN);
        assert_ne!(ty, TypeId::ERROR);
        assert!(
            crate::query_boundaries::common::union_members(&types, ty).is_some(),
            "{name} should preserve its union body",
        );
        let def_id = state
            .ctx
            .get_existing_def_id(sym_id)
            .expect("recursive alias DefId should be registered");
        assert_eq!(
            state.ctx.definition_store.get_body(def_id),
            Some(ty),
            "{name} body should be registered for lazy self references",
        );
    }

    let loop_sym = binder.file_locals.get("Loop").expect("loop alias symbol");
    assert!(
        state
            .direct_declaration_file_type_alias_result(loop_sym, arena.as_ref())
            .is_none(),
        "unguarded alias loops stay on the child-checker circularity path",
    );
}

#[test]
fn direct_external_declaration_interface_lowering_merges_builtin_heritage() {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    let mut parser = ParserState::new(
        "node_modules/pkg/client.d.ts".to_string(),
        r#"
                declare interface LoadFailureEvent extends Event {
                    payload: Error;
                }
                declare interface PackageReadyEvent extends Event {
                    detail: string;
                }
            "#
        .to_string(),
    );
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
        "requester.ts".to_string(),
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

    for (interface_name, own_member) in [
        ("LoadFailureEvent", "payload"),
        ("PackageReadyEvent", "detail"),
    ] {
        let sym_id = binder
            .file_locals
            .get(interface_name)
            .expect("external event interface symbol");
        let (ty, params) = state
            .direct_cross_file_interface_lowering(
                sym_id,
                binder.as_ref(),
                arena.as_ref(),
                false,
                false,
            )
            .expect("simple external declaration heritage should lower directly");
        assert!(params.is_empty(), "{interface_name} should be non-generic");

        for prop_name in [own_member, "target"] {
            let prop = types.intern_string(prop_name);
            assert!(
                crate::query_boundaries::common::raw_property_type(
                    state.ctx.types.as_type_database(),
                    ty,
                    prop,
                )
                .is_some(),
                "{interface_name} should expose {prop_name} after heritage merge",
            );
        }
    }
}
