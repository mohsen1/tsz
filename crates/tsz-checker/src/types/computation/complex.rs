//! Complex type computation: new expressions and constructability.
//!
//! Contextual sensitivity analysis is in `contextual.rs`.
//! Union/intersection/keyof/class helpers are in `type_operators.rs`.

include!("complex_large_methods/get_type_of_new_expression_with_request_15_9.rs");

use crate::call_checker::CallableContext;
use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::construct_signatures::has_construct_overloads;
use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tracing::trace;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

// Re-export for backwards compatibility with existing imports
pub(crate) use super::contextual::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};

fn should_preserve_contextual_application_shape(
    db: &dyn tsz_solver::construction::TypeDatabase,
    ty: TypeId,
) -> bool {
    if crate::query_boundaries::common::application_info(db, ty).is_some() {
        return true;
    }

    if let Some(members) = crate::query_boundaries::common::union_members(db, ty) {
        return members
            .iter()
            .copied()
            .any(|member| should_preserve_contextual_application_shape(db, member));
    }

    if let Some(inner) = crate::query_boundaries::common::readonly_inner_type(db, ty)
        .or_else(|| crate::query_boundaries::common::no_infer_inner_type(db, ty))
    {
        return should_preserve_contextual_application_shape(db, inner);
    }

    false
}

impl<'a> CheckerState<'a> {
    pub(crate) const fn should_suppress_weak_key_arg_mismatch(
        &mut self,
        _callee_expr: NodeIndex,
        _args: &[NodeIndex],
        _mismatch_index: usize,
        _actual: TypeId,
    ) -> bool {
        false
    }
    pub(crate) const fn should_suppress_weak_key_no_overload(
        &mut self,
        _callee_expr: NodeIndex,
        _args: &[NodeIndex],
    ) -> bool {
        false
    }

    fn typed_array_length_constructor_return_type(
        &mut self,
        callee_expr: NodeIndex,
        arg_types: &[TypeId],
        return_type: TypeId,
    ) -> Option<TypeId> {
        let callee_name = self.ctx.arena.get_identifier_text(callee_expr)?;
        if !matches!(
            callee_name,
            "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "BigInt64Array"
                | "BigUint64Array"
        ) {
            return None;
        }

        let length_like_constructor = arg_types.is_empty()
            || arg_types.first().is_some_and(|&arg_type| {
                tsz_solver::operations::widening::widen_literal_type(self.ctx.types, arg_type)
                    == TypeId::NUMBER
            });
        if !length_like_constructor {
            return None;
        }

        let (base, args) =
            query::get_application_info(self.ctx.types, return_type).or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(return_type)
                    .and_then(|alias| query::get_application_info(self.ctx.types, alias))
            })?;
        if args.len() != 1 {
            return None;
        }

        let array_buffer = self.resolve_lib_type_by_name("ArrayBuffer")?;
        Some(
            self.ctx
                .types
                .factory()
                .application(base, vec![array_buffer]),
        )
    }

    fn lib_constructor_return_type_for_type_shadow(
        &mut self,
        callee_expr: NodeIndex,
    ) -> Option<TypeId> {
        let callee_name = self.ctx.arena.get_identifier_text(callee_expr)?;
        let value_sym_id = self.find_value_symbol_in_libs(callee_name)?;
        let type_sym_id = self.type_only_non_lib_constructor_shadow(callee_expr, callee_name)?;
        let resolved = self
            .resolve_lib_type_by_name(callee_name)
            .filter(|&ty| !matches!(ty, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN));
        trace!(
            callee_name,
            type_sym_id = type_sym_id.0,
            value_sym_id = value_sym_id.0,
            resolved = ?resolved,
            "lib_constructor_return_type_for_type_shadow"
        );
        resolved
    }

    fn lib_constructor_type_for_type_shadow(&mut self, callee_expr: NodeIndex) -> Option<TypeId> {
        let callee_name = self.ctx.arena.get_identifier_text(callee_expr)?;
        let value_sym_id = self.find_value_symbol_in_libs(callee_name)?;
        let type_sym_id = self.type_only_non_lib_constructor_shadow(callee_expr, callee_name)?;
        let constructor_name = format!("{callee_name}Constructor");
        let constructor_type = self
            .resolve_lib_type_by_name(&constructor_name)
            .or_else(|| Some(self.get_type_of_symbol(value_sym_id)))?;
        trace!(
            callee_name,
            type_sym_id = type_sym_id.0,
            constructor_type = constructor_type.0,
            constructable = crate::query_boundaries::common::has_construct_signatures(
                self.ctx.types,
                constructor_type
            ),
            "lib_constructor_type_for_type_shadow"
        );
        crate::query_boundaries::common::has_construct_signatures(self.ctx.types, constructor_type)
            .then_some(constructor_type)
    }

    fn type_only_non_lib_constructor_shadow(
        &mut self,
        callee_expr: NodeIndex,
        callee_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let crate::symbol_resolver::TypeSymbolResolution::Type(type_sym_id) =
            self.resolve_identifier_symbol_in_type_position(callee_expr)
        else {
            trace!(
                callee_name,
                "lib constructor shadow: no type-position shadow"
            );
            return None;
        };
        if self.ctx.symbol_is_from_actual_or_cloned_lib(type_sym_id) {
            trace!(
                callee_name,
                type_sym_id = type_sym_id.0,
                "lib constructor shadow: type symbol is lib"
            );
            return None;
        }

        let symbol = self.ctx.binder.get_symbol(type_sym_id)?;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        if symbol.has_any_flags(value_flags_except_module) && !symbol.is_type_only {
            trace!(
                callee_name,
                type_sym_id = type_sym_id.0,
                "lib constructor shadow: local type also has a value constructor"
            );
            return None;
        }

        Some(type_sym_id)
    }

    ///
    /// This keeps general alias typing unchanged (important for type-position behavior)
    /// while ensuring constructor resolution sees the direct constructable type.
    fn new_expression_export_equals_constructor_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }

        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }

        let import_decl = self.ctx.arena.get_import_decl(decl_node)?;
        let module_specifier = self.get_require_module_specifier(import_decl.module_specifier)?;
        let exports = self.resolve_effective_module_exports(&module_specifier)?;
        let export_equals_sym = exports.get("export=")?;
        let resolved_export_equals_sym = self
            .ctx
            .binder
            .get_symbol(export_equals_sym)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS))
            .then(|| {
                let mut visited_aliases = AliasCycleTracker::new();
                self.resolve_alias_symbol(export_equals_sym, &mut visited_aliases)
            })
            .flatten()
            .unwrap_or(export_equals_sym);

        let mut constructor_type = self.get_type_of_symbol(resolved_export_equals_sym);
        if constructor_type == TypeId::UNKNOWN || constructor_type == TypeId::ERROR {
            constructor_type = self.get_type_of_symbol(export_equals_sym);
        }

        // If `export =` resolves to an alias chain we couldn't lower to a concrete
        // constructor type, prefer any concrete value export from the module over
        // propagating unknown into TS18046 false positives.
        if constructor_type == TypeId::UNKNOWN || constructor_type == TypeId::ERROR {
            let mut preferred_candidate: Option<TypeId> = None;
            let mut fallback_candidate: Option<TypeId> = None;
            for (export_name, export_sym) in exports.iter() {
                if export_name == "export=" {
                    continue;
                }
                let candidate = self.get_type_of_symbol(*export_sym);
                if candidate == TypeId::UNKNOWN || candidate == TypeId::ERROR {
                    continue;
                }

                let symbol_flags = self
                    .ctx
                    .binder
                    .get_symbol(*export_sym)
                    .map_or(0, |sym| sym.flags);
                let is_likely_constructor_symbol = (symbol_flags
                    & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::FUNCTION))
                    != 0;
                if is_likely_constructor_symbol && preferred_candidate.is_none() {
                    preferred_candidate = Some(candidate);
                }
                if fallback_candidate.is_none() {
                    fallback_candidate = Some(candidate);
                }
            }
            if let Some(candidate) = preferred_candidate.or(fallback_candidate) {
                constructor_type = candidate;
            }
        }

        Some(constructor_type)
    }

    /// Resolve the `"module.exports"` constructor type for a CJS-of-ESM interop
    /// `new` expression. Returns `None` when the interop does not apply, emits
    /// TS2351 and returns `Some(TypeId::ERROR)` when the value is not
    /// constructable, or returns `Some(ty)` when it is.
    fn module_exports_interop_new_type(
        &mut self,
        module_name: &str,
        callee_idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.current_file_uses_module_exports_require_interop(module_name) {
            return None;
        }
        let ty = self
            .resolve_effective_module_exports_from_file(
                module_name,
                Some(self.ctx.current_file_idx),
            )
            .and_then(|exports| exports.get("module.exports"))
            .map(|sym_id| self.get_type_of_symbol(sym_id))?;
        if !crate::query_boundaries::common::has_construct_signatures(self.ctx.types, ty) {
            self.error_not_constructable_at(ty, callee_idx);
            return Some(TypeId::ERROR);
        }
        Some(ty)
    }

    #[allow(dead_code)]
    pub(crate) fn get_type_of_new_expression(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_new_expression_with_request(idx, &TypingRequest::NONE)
    }

    __tsz_split_complex_get_type_of_new_expression_with_request_15_9!();

    /// For intersection constructor types, evaluate any Application members so
    /// the solver can resolve their construct signatures.
    ///
    /// e.g. `Constructor<Tagged> & typeof Base` — `Constructor<Tagged>` is an
    /// Application that must be instantiated to reveal `new(...) => Tagged`.
    fn evaluate_application_members_in_intersection(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) = query::intersection_members(self.ctx.types, type_id) else {
            return type_id;
        };

        let mut changed = false;
        let mut new_members = Vec::with_capacity(members.len());

        for member in &members {
            let evaluated = self.evaluate_application_type(*member);
            if evaluated != *member {
                changed = true;
                new_members.push(evaluated);
            } else {
                new_members.push(*member);
            }
        }

        if changed {
            self.ctx.types.intersection(new_members)
        } else {
            type_id
        }
    }
}
