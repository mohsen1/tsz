use crate::context::{CheckerContext, CheckerOptions};
use crate::query_boundaries::common::{TypeInterner, function_shape_for_type};
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

fn checker_for<'a>(
    arena: &'a tsz_parser::parser::node::NodeArena,
    binder: &'a BinderState,
    types: &'a TypeInterner,
) -> CheckerState<'a> {
    CheckerState {
        ctx: CheckerContext::new(
            arena,
            binder,
            types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        ),
    }
}

#[test]
fn direct_source_file_function_declaration_lowers_annotated_signature() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                export function summarize(value: number, label: string): string {
                    return label + value;
                }
            "#,
    );
    let mut state = checker_for(arena.as_ref(), binder.as_ref(), &types);
    let summarize_sym = binder
        .file_locals
        .get("summarize")
        .expect("function symbol");

    let result = state
        .direct_source_file_function_declaration_type(
            summarize_sym,
            binder.as_ref(),
            arena.as_ref(),
            true,
        )
        .expect("annotated source function should lower directly");
    let shape = function_shape_for_type(&types, result)
        .expect("direct function lowering should produce a function type");

    assert_eq!(shape.params.len(), 2);
    assert_eq!(shape.params[0].type_id, TypeId::NUMBER);
    assert_eq!(shape.params[1].type_id, TypeId::STRING);
    assert_eq!(shape.return_type, TypeId::STRING);
}

#[test]
fn direct_source_file_function_declaration_rejects_inferred_signature() {
    let (arena, binder, types) = parse_bound_source(
        r#"
                export function summarize(value: number) {
                    return value;
                }
                export function format(value): string {
                    return "";
                }
            "#,
    );
    let mut state = checker_for(arena.as_ref(), binder.as_ref(), &types);

    for name in ["summarize", "format"] {
        let sym = binder.file_locals.get(name).expect("function symbol");
        assert!(
            state
                .direct_source_file_function_declaration_type(
                    sym,
                    binder.as_ref(),
                    arena.as_ref(),
                    true,
                )
                .is_none(),
            "{name} should fall back when any signature type is inferred",
        );
    }
}
