//! Async method return-type helpers for declaration inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::node::MethodDeclData;
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
