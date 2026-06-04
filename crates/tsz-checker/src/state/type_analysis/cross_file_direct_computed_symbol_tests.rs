use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeId;

fn parse_bound_declaration(
    source: &str,
) -> (
    Arc<tsz_parser::parser::node::NodeArena>,
    Arc<BinderState>,
    TypeInterner,
) {
    let mut parser = ParserState::new(
        "node_modules/typescript/lib/lib.test.iterable.d.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (
        Arc::new(parser.get_arena().clone()),
        Arc::new(binder),
        TypeInterner::new(),
    )
}

fn direct_builtin_interface_type(
    source: &str,
    symbol_name: &str,
) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
    let (arena, binder, types) = parse_bound_declaration(source);
    let sym_id = binder
        .file_locals
        .get(symbol_name)
        .unwrap_or_else(|| panic!("{symbol_name} should resolve in declaration binder"));
    let ctx = CheckerContext::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "fixture.ts".to_string(),
        CheckerOptions::default(),
    );
    let state = CheckerState { ctx };
    state.direct_cross_file_interface_lowering(
        sym_id,
        binder.as_ref(),
        arena.as_ref(),
        false,
        false,
    )
}

#[test]
fn direct_builtin_interface_lowering_accepts_well_known_symbol_computed_members() {
    let (ty, params) = direct_builtin_interface_type(
        r#"
                interface ArrayIterator<T> {
                    next(): T;
                }
                interface IterableBox {
                    [Symbol.iterator](): ArrayIterator<string>;
                    values(): ArrayIterator<string>;
                }
            "#,
        "IterableBox",
    )
    .expect("well-known Symbol.iterator computed member should be admitted");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty());
}

#[test]
fn direct_builtin_interface_lowering_accepts_async_iterator_computed_members() {
    let (ty, params) = direct_builtin_interface_type(
        r#"
                interface AsyncIteratorBox {
                    [Symbol.asyncIterator](): AsyncIteratorBox;
                }
            "#,
        "AsyncIteratorBox",
    )
    .expect("well-known Symbol.asyncIterator computed member should be admitted");

    assert_ne!(ty, TypeId::UNKNOWN);
    assert_ne!(ty, TypeId::ERROR);
    assert!(params.is_empty());
}

#[test]
fn direct_builtin_interface_lowering_rejects_shadowed_symbol_computed_members() {
    assert!(
        direct_builtin_interface_type(
            r#"
                declare const Symbol: { iterator: unique symbol };
                interface ArrayIterator<T> {
                    next(): T;
                }
                interface IterableBox {
                    [Symbol.iterator](): ArrayIterator<string>;
                }
            "#,
            "IterableBox",
        )
        .is_none(),
        "same-arena Symbol value shadows must keep computed members on fallback",
    );
}

#[test]
fn direct_builtin_interface_lowering_rejects_non_admitted_computed_members() {
    assert!(
        direct_builtin_interface_type(
            r#"
                interface MatcherBox {
                    [Symbol.match](): boolean;
                }
            "#,
            "MatcherBox",
        )
        .is_none(),
        "computed names outside the admitted Symbol iterator keys must fall back",
    );
}
