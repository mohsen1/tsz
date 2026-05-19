//! Lazy identity preservation for simple actual-lib interface references.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn try_lazy_actual_lib_interface_reference(
        &mut self,
        sym_id: SymbolId,
        escaped_name: &str,
        flags: u32,
        declarations: &[NodeIndex],
        is_merged_with_namespace: bool,
        should_force_interface_decl_path: bool,
    ) -> Option<TypeId> {
        if is_merged_with_namespace
            || should_force_interface_decl_path
            || (flags & symbol_flags::TYPE_ALIAS) != 0
            || (flags & symbol_flags::CLASS) != 0
            || !self.ctx.has_lib_loaded()
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            || self.ctx.file_local_type_shadow_for_lib_name(escaped_name)
            || self.lib_name_locally_augmented(escaped_name)
            || self.interface_declarations_have_index_signature(sym_id, declarations)
        {
            return None;
        }

        let def_id = self
            .ctx
            .get_or_create_def_id_for_symbol_name(sym_id, escaped_name);
        if self.ctx.get_def_type_params(def_id).is_none() {
            let params =
                self.extract_declared_type_params_for_reference_symbol(sym_id, escaped_name);
            if !params.is_empty() {
                self.ctx.insert_def_type_params(def_id, params);
            }
        }

        if self
            .ctx
            .get_def_type_params(def_id)
            .is_some_and(|params| !params.is_empty())
        {
            return None;
        }

        Some(self.ctx.types.lazy(def_id))
    }

    fn interface_declarations_have_index_signature(
        &self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
    ) -> bool {
        declarations.iter().copied().any(|decl_idx| {
            let declaration_arenas = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .cloned();

            if let Some(declaration_arenas) = declaration_arenas {
                return declaration_arenas.iter().any(|arena| {
                    Self::interface_declaration_has_index_signature(decl_idx, arena.as_ref())
                });
            }

            let arena = self
                .ctx
                .binder
                .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
            Self::interface_declaration_has_index_signature(decl_idx, arena)
        })
    }

    fn interface_declaration_has_index_signature(decl_idx: NodeIndex, arena: &NodeArena) -> bool {
        arena
            .get(decl_idx)
            .and_then(|node| arena.get_interface(node))
            .is_some_and(|interface| {
                interface.members.nodes.iter().copied().any(|member_idx| {
                    arena
                        .get(member_idx)
                        .is_some_and(|member| member.kind == syntax_kind_ext::INDEX_SIGNATURE)
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerContext, CheckerOptions, LibContext};
    use crate::query_boundaries::common::{TypeInterner, lazy_def_id};
    use crate::test_utils::load_lib_files;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::operations::property::PropertyAccessResult;

    fn with_lib_checker<R>(lib_names: &[&str], test: impl FnOnce(&mut CheckerState<'_>) -> R) -> R {
        let lib_files = load_lib_files(lib_names);
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

    fn try_lazy_interface_for_name(state: &mut CheckerState<'_>, name: &str) -> Option<TypeId> {
        let sym_id = state
            .ctx
            .binder
            .file_locals
            .get(name)
            .unwrap_or_else(|| panic!("{name} should resolve to a lib symbol"));
        let (escaped_name, flags, declarations) = {
            let symbol = state
                .ctx
                .binder
                .get_symbol(sym_id)
                .unwrap_or_else(|| panic!("{name} symbol data should exist"));
            (
                symbol.escaped_name.clone(),
                symbol.flags,
                symbol.declarations.clone(),
            )
        };

        state.try_lazy_actual_lib_interface_reference(
            sym_id,
            &escaped_name,
            flags,
            &declarations,
            false,
            false,
        )
    }

    #[test]
    fn actual_lib_interface_reference_stays_lazy_and_recovers_members() {
        with_lib_checker(&["es5.d.ts", "dom.d.ts"], |state| {
            let sym_id = state
                .ctx
                .binder
                .file_locals
                .get("HTMLDivElement")
                .expect("HTMLDivElement should resolve to a DOM lib symbol");

            let div_ref = state.type_reference_symbol_type(sym_id);
            assert!(
                lazy_def_id(state.ctx.types, div_ref).is_some(),
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
                    matches!(result, PropertyAccessResult::Success { .. }),
                    "lazy HTMLDivElement should recover {property}, got {result:?}",
                );
            }
        });
    }

    #[test]
    fn actual_lib_interface_reference_rejects_generic_interfaces() {
        with_lib_checker(&["es5.d.ts", "es2015.promise.d.ts"], |state| {
            assert!(
                try_lazy_interface_for_name(state, "Promise").is_none(),
                "generic actual-lib interfaces should use the normal structural path",
            );
        });
    }

    #[test]
    fn actual_lib_interface_reference_rejects_index_signature_interfaces() {
        with_lib_checker(&["es5.d.ts", "dom.d.ts"], |state| {
            assert!(
                try_lazy_interface_for_name(state, "CSSStyleDeclarationBase").is_none(),
                "actual-lib interfaces with index signatures should use the normal structural path",
            );
        });
    }
}
