use crate::context::{CheckerContext, CheckerOptions};
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use smallvec::smallvec;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::{NodeIndex, ParserState, node::NodeArena};

fn overload_and_impl_decls_for_symbol(
    arena: &NodeArena,
    symbol_id: SymbolId,
    binder: &BinderState,
) -> (NodeIndex, NodeIndex) {
    let symbol = binder
        .get_symbol(symbol_id)
        .unwrap_or_else(|| panic!("symbol {symbol_id:?} should exist for overload probe"));

    let mut overload_decl = None;
    let mut impl_decl = None;

    for decl_idx in &symbol.declarations {
        let Some(node) = arena.get(*decl_idx) else {
            continue;
        };
        if let Some(function) = arena.get_function(node) {
            if function.body.is_some() {
                impl_decl = Some(*decl_idx);
            } else {
                overload_decl = Some(*decl_idx);
            }
        }
    }

    (
        overload_decl.expect("overload declaration should be bodyless"),
        impl_decl.expect("implementation declaration should have body"),
    )
}

fn diagnostics_for(
    arena: &Arc<NodeArena>,
    binder: &BinderState,
    root: NodeIndex,
    types: &TypeInterner,
) -> Vec<Diagnostic> {
    let mut checker = CheckerState {
        ctx: CheckerContext::new(
            arena.as_ref(),
            binder,
            types,
            "fixture.ts".to_string(),
            CheckerOptions::default(),
        ),
    };
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn ts2394_cross_file_unresolved_span_is_suppressed_instead_of_impl_anchored() {
    let source = r#"
function parseArg(x: string): string;
function parseArg(x: number): string {
    return "ok";
}
"#;

    let mut parser = ParserState::new("fixture.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let arena = Arc::new(parser.get_arena().clone());
    let types = TypeInterner::new();
    let parse_arg = binder
        .file_locals
        .get("parseArg")
        .unwrap_or_else(|| panic!("fixture symbol parseArg should exist"));

    let baseline = diagnostics_for(&arena, &binder, root, &types);
    let baseline_ts2394 = baseline.iter().filter(|d| d.code == 2394).count();
    assert_eq!(
        baseline_ts2394, 1,
        "intra-file overload mismatch should report exactly one TS2394 before declaration-arena injection, got: {baseline:?}",
    );

    let (overload_decl, _impl_decl) =
        overload_and_impl_decls_for_symbol(arena.as_ref(), parse_arg, &binder);

    let mut synthetic_arena = (*arena).clone();
    synthetic_arena.source_files.clear();
    let declaration_arenas = Arc::make_mut(&mut binder.declaration_arenas);
    declaration_arenas.insert(
        (parse_arg, overload_decl),
        smallvec![Arc::new(synthetic_arena)],
    );

    let injected = diagnostics_for(&arena, &binder, root, &types);
    let injected_ts2394 = injected.iter().filter(|d| d.code == 2394).count();
    assert_eq!(
        injected_ts2394, 0,
        "declaration arena with no source files should suppress TS2394 anchoring via implementation fallback, got: {injected:?}",
    );
}
