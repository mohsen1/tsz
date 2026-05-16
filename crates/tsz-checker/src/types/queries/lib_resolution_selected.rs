//! Selection helpers for resolving canonical standard-library symbols.

use crate::context::CheckerContext;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId};
use tsz_solver::{DefId, TypeId, TypeParamInfo};

pub(crate) fn selected_lib_symbol_for_name(
    ctx: &CheckerContext<'_>,
    name: &str,
    sym_id: Option<SymbolId>,
    lib_binders: &[Arc<BinderState>],
) -> Option<(SymbolId, Option<Arc<BinderState>>)> {
    sym_id
        .and_then(|sym_id| {
            ctx.binder
                .get_symbol_with_libs(sym_id, lib_binders)
                .is_some_and(|symbol| symbol.escaped_name == name)
                .then_some((sym_id, None))
        })
        .or_else(|| {
            ctx.lib_contexts
                .iter()
                .take(ctx.actual_lib_file_count)
                .find_map(|lib_ctx| {
                    let sym_id = lib_ctx.binder.file_locals.get(name)?;
                    lib_ctx
                        .binder
                        .get_symbol(sym_id)
                        .is_some_and(|symbol| symbol.escaped_name == name)
                        .then_some((sym_id, Some(Arc::clone(&lib_ctx.binder))))
                })
        })
}

pub(crate) fn canonical_interface_symbol_id(
    ctx: &CheckerContext<'_>,
    name: &str,
    sym_id: SymbolId,
    selected_from_lib_context: bool,
) -> SymbolId {
    if !selected_from_lib_context {
        return sym_id;
    }

    let def_id = ctx.get_canonical_lib_def_id(name, sym_id);
    ctx.def_symbol_identity(def_id)
        .map(|(sym_id, _)| sym_id)
        .unwrap_or(sym_id)
}

pub(crate) fn register_selected_lib_def_resolved(
    ctx: &CheckerContext<'_>,
    name: &str,
    sym_id: SymbolId,
    selected_from_lib_context: bool,
    ty: TypeId,
    params: Vec<TypeParamInfo>,
) -> DefId {
    if !selected_from_lib_context {
        return ctx.register_lib_def_resolved(sym_id, ty, params);
    }

    let def_id = ctx.get_canonical_lib_def_id(name, sym_id);
    ctx.insert_def_type_params(def_id, params.clone());
    ctx.register_def_auto_params_in_envs(def_id, ty, params);
    def_id
}
