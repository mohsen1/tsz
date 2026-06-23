//! Async method return-type helpers for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::{FunctionData, MethodDeclData};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn inferred_method_return_type_text(
        &self,
        method: &MethodDeclData,
        return_type_id: tsz_solver::types::TypeId,
    ) -> String {
        let return_type_id = self.widen_unique_symbol_value_type_for_dts(return_type_id, 0);
        let text = self.print_type_id_for_inferred_declaration(return_type_id);
        if self.async_method_return_type_is_already_global_promise(method, return_type_id) {
            return text;
        }
        self.wrap_async_method_return_type_text(method, text)
    }

    pub(in crate::declaration_emitter) fn wrap_async_method_return_type_text(
        &self,
        method: &MethodDeclData,
        text: String,
    ) -> String {
        if self.method_is_async(method) && !method.asterisk_token {
            format!("Promise<{text}>")
        } else {
            text
        }
    }

    fn async_method_return_type_is_already_global_promise(
        &self,
        method: &MethodDeclData,
        return_type_id: tsz_solver::types::TypeId,
    ) -> bool {
        self.method_is_async(method)
            && !method.asterisk_token
            && self.type_id_is_global_promise_application(return_type_id)
    }

    pub(in crate::declaration_emitter) fn method_is_async(&self, method: &MethodDeclData) -> bool {
        self.arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword)
    }

    /// Wrap a source-faithful, *unwrapped* function-body return-type text in
    /// `Promise<...>` when the function is an `async` non-generator, mirroring the
    /// method path's [`wrap_async_method_return_type_text`].
    ///
    /// The AST-walking declaration paths (`emit_function_initializer_type_annotation`,
    /// `function_expression_type_text_from_ast_at`) derive the return type from the
    /// function body, which yields the body's own return type. tsc reports an async
    /// function's return type as `Promise<Awaited<T>>`; when the body itself already
    /// produces a global `Promise<...>` value (`Awaited` collapses the nesting) the
    /// text is left as-is to avoid a spurious `Promise<Promise<...>>`. The
    /// `body_return_type_id` is the body's *unwrapped* return type (not the function
    /// signature's already-wrapped return type), so the global-`Promise` check
    /// distinguishes the two cases. Generators (`async function*`) flow through the
    /// generator-yield path and are skipped here.
    pub(in crate::declaration_emitter) fn wrap_async_function_return_type_text(
        &self,
        func: &FunctionData,
        text: String,
        body_return_type_id: Option<tsz_solver::types::TypeId>,
    ) -> String {
        if !func.is_async || func.asterisk_token {
            return text;
        }
        if body_return_type_id
            .is_some_and(|type_id| self.type_id_is_global_promise_application(type_id))
        {
            return text;
        }
        format!("Promise<{text}>")
    }

    /// The body's *unwrapped* return value type for a concise-body arrow or a
    /// single-`return` block body, used to decide whether an async wrapper would
    /// double-wrap an already-`Promise` body (see
    /// [`wrap_async_function_return_type_text`]).
    pub(in crate::declaration_emitter) fn function_body_return_value_type_id(
        &self,
        func: &FunctionData,
    ) -> Option<tsz_solver::types::TypeId> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            let return_expr = self.function_body_single_return_expression(func.body)?;
            self.get_node_type(return_expr)
        } else {
            self.get_node_type(func.body)
        }
    }

    fn type_id_is_global_promise_application(&self, type_id: tsz_solver::types::TypeId) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        let tsz_solver::type_queries::PromiseTypeKind::Application { base, .. } =
            tsz_solver::type_queries::classify_promise_type(interner, type_id)
        else {
            return false;
        };
        self.type_id_is_global_promise_base(base)
    }

    fn type_id_is_global_promise_base(&self, type_id: tsz_solver::types::TypeId) -> bool {
        if type_id == tsz_solver::types::TypeId::PROMISE_BASE {
            return true;
        }
        let Some(interner) = self.type_interner else {
            return false;
        };
        match tsz_solver::type_queries::classify_promise_type(interner, type_id) {
            tsz_solver::type_queries::PromiseTypeKind::Lazy(def_id) => {
                self.def_id_is_global_promise(def_id)
            }
            tsz_solver::type_queries::PromiseTypeKind::TypeQuery(sym_ref) => {
                self.symbol_id_is_global_promise(tsz_binder::SymbolId(sym_ref.0))
            }
            tsz_solver::type_queries::PromiseTypeKind::Application { base, .. } => {
                self.type_id_is_global_promise_base(base)
            }
            _ => false,
        }
    }

    fn def_id_is_global_promise(&self, def_id: tsz_solver::DefId) -> bool {
        self.type_cache
            .as_ref()
            .and_then(|cache| cache.def_to_symbol.get(&def_id).copied())
            .is_some_and(|sym_id| self.symbol_id_is_global_promise(sym_id))
    }

    fn symbol_id_is_global_promise(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        binder.get_global_type("Promise") == Some(sym_id) && binder.lib_symbol_ids.contains(&sym_id)
    }
}
