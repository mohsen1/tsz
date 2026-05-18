use super::*;
use crate::context::{CheckerContext, CheckerOptions, LibContext};
use crate::query_boundaries::common::TypeInterner;
use crate::test_utils::load_lib_files;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::parser::ParserState;

fn with_dom_checker<R>(test: impl FnOnce(&mut CheckerState<'_>) -> R) -> R {
    let lib_files = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    with_lib_checker(lib_files, test)
}

fn with_dom_iterable_checker<R>(test: impl FnOnce(&mut CheckerState<'_>) -> R) -> R {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "dom.d.ts",
    ]);
    with_lib_checker(lib_files, test)
}

fn with_lib_checker<R>(
    lib_files: Vec<Arc<LibFile>>,
    test: impl FnOnce(&mut CheckerState<'_>) -> R,
) -> R {
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
    test(&mut state)
}

#[test]
fn bare_builtin_dom_interface_reference_stays_lazy_and_recovers_members() {
    with_dom_checker(|state| {
        let div_sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLDivElement")
            .expect("HTMLDivElement should resolve to a dom lib symbol");

        let div_ref = state.type_reference_symbol_type(div_sym_id);
        assert!(
            crate::query_boundaries::common::lazy_def_id(state.ctx.types, div_ref).is_some(),
            "bare actual-lib interface references should preserve Lazy(DefId) identity",
        );
        assert!(
            !state
                .ctx
                .lib_type_resolution_cache
                .contains_key("HTMLDivElement"),
            "bare actual-lib interface references should not eagerly materialize the full interface",
        );

        for property in ["innerHTML", "tagName"] {
            let result = state.resolve_property_access_with_env(div_ref, property);
            assert!(
                matches!(
                    result,
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                ),
                "lazy HTMLDivElement should recover {property} without full lowering, got {result:?}",
            );
        }
    });
}

#[test]
fn same_lib_builtin_base_allows_deep_dom_leaf_direct_lowering() {
    with_dom_checker(|state| {
        let html_sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLElement")
            .expect("HTMLElement should resolve to a dom lib symbol");
        assert!(
            state.ctx.binder.get_symbol(html_sym_id).is_some(),
            "HTMLElement should be present in the merged lib binder",
        );

        let div_sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLDivElement")
            .expect("HTMLDivElement should resolve to a dom lib symbol");
        let div_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&div_sym_id)
            .map(std::convert::AsRef::as_ref)
            .expect("HTMLDivElement should have a delegate arena");
        let (div_ty, div_params) = state
            .direct_cross_file_interface_lowering(
                div_sym_id,
                state.ctx.binder,
                div_arena,
                false,
                false,
            )
            .expect("builtin dom interfaces with same-lib bases should lower directly");
        assert!(div_params.is_empty());

        let tag_name = state.ctx.types.intern_string("tagName");
        assert!(
            crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                div_ty,
                tag_name,
            )
            .is_some(),
            "HTMLDivElement should include inherited Element members through same-lib heritage",
        );
    });
}

#[test]
fn builtin_base_in_progress_still_allows_derived_dom_leaf_direct_lowering() {
    with_dom_checker(|state| {
        state
            .ctx
            .lib_heritage_in_progress
            .insert("HTMLElement".into());

        let div_sym_id = state
            .ctx
            .binder
            .file_locals
            .get("HTMLDivElement")
            .expect("HTMLDivElement should resolve to a dom lib symbol");
        let div_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&div_sym_id)
            .map(std::convert::AsRef::as_ref)
            .expect("HTMLDivElement should have a delegate arena");

        let (div_ty, div_params) = state
            .direct_cross_file_interface_lowering(
                div_sym_id,
                state.ctx.binder,
                div_arena,
                false,
                false,
            )
            .expect("derived builtin DOM interfaces should not delegate solely because the base is already merging");
        assert!(div_params.is_empty());

        let align = state.ctx.types.intern_string("align");
        assert!(
            crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                div_ty,
                align,
            )
            .is_some(),
            "HTMLDivElement should keep its own members even when HTMLElement is merging",
        );
    });
}

#[test]
fn builtin_base_cache_sentinel_still_allows_derived_dom_direct_lowering() {
    with_dom_checker(|state| {
        state
            .ctx
            .lib_type_resolution_cache
            .insert("Node".to_string(), None);

        let attr_sym_id = state
            .ctx
            .binder
            .file_locals
            .get("Attr")
            .expect("Attr should resolve to a dom lib symbol");
        let attr_arena = state
            .ctx
            .binder
            .symbol_arenas
            .get(&attr_sym_id)
            .map(std::convert::AsRef::as_ref)
            .expect("Attr should have a delegate arena");

        let (attr_ty, attr_params) = state
            .direct_cross_file_interface_lowering(
                attr_sym_id,
                state.ctx.binder,
                attr_arena,
                false,
                false,
            )
            .expect("derived builtin DOM interfaces should not delegate solely because the base cache has an in-progress sentinel");
        assert!(attr_params.is_empty());

        let name = state.ctx.types.intern_string("name");
        assert!(
            crate::query_boundaries::common::raw_property_type(
                state.ctx.types.as_type_database(),
                attr_ty,
                name,
            )
            .is_some(),
            "Attr should keep its own members even when Node has an in-progress cache sentinel",
        );
    });
}

#[test]
fn builtin_iterator_return_bases_still_allow_direct_dom_heritage_lowering() {
    with_dom_iterable_checker(|state| {
        for (name, own_member, inherited_member) in [
            ("FormDataIterator", "[Symbol.iterator]", "next"),
            ("FontFaceSet", "ready", "add"),
        ] {
            let sym_id = state
                .ctx
                .binder
                .file_locals
                .get(name)
                .unwrap_or_else(|| panic!("{name} should resolve to a dom lib symbol"));
            let arena = state
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or_else(|| panic!("{name} should have a delegate arena"));

            let (ty, _params) = state
                .direct_cross_file_interface_lowering(sym_id, state.ctx.binder, arena, false, false)
                .unwrap_or_else(|| {
                    panic!("{name} should lower directly through iterator-sensitive bases")
                });

            for member in [own_member, inherited_member] {
                let atom = state.ctx.types.intern_string(member);
                assert!(
                    crate::query_boundaries::common::raw_property_type(
                        state.ctx.types.as_type_database(),
                        ty,
                        atom,
                    )
                    .is_some(),
                    "{name} should include {member}",
                );
            }
        }
    });
}
