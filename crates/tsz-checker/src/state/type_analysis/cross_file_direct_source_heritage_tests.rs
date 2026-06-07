use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

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

#[test]
fn direct_cross_file_interface_lowering_expands_source_option_bag_heritage() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface BaseShape { title: string; logo: string; }
                interface DerivedShape extends BaseShape { count: number; }
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
    let derived_sym = binder
        .file_locals
        .get("DerivedShape")
        .expect("derived symbol");

    let (derived_type, params) = state
        .direct_cross_file_interface_lowering(
            derived_sym,
            binder.as_ref(),
            arena.as_ref(),
            false,
            true,
        )
        .expect("simple same-file option-bag heritage should lower directly");

    assert!(params.is_empty());
    for property in ["title", "logo", "count"] {
        let atom = types.intern_string(property);
        assert!(
            crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                derived_type,
                atom,
            )
            .is_some(),
            "directly lowered derived interface should include {property}"
        );
    }
}

#[test]
fn direct_cross_file_interface_lowering_rejects_generic_source_heritage() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                interface Boxed<T> { value: T; }
                interface Wrapped extends Boxed<string> { label: string; }
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
    let wrapped_sym = binder.file_locals.get("Wrapped").expect("wrapped symbol");

    assert!(
        state
            .direct_cross_file_interface_lowering(
                wrapped_sym,
                binder.as_ref(),
                arena.as_ref(),
                false,
                true,
            )
            .is_none(),
        "generic source heritage stays on the child-checker path"
    );
}
