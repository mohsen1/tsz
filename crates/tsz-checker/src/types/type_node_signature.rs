//! Signature helpers for `TypeNodeChecker`.

use super::type_node::TypeNodeChecker;
use crate::query_boundaries::signature_building as signature_building_boundary;
use crate::query_boundaries::type_construction;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl TypeNodeChecker<'_, '_> {
    /// Extract parameter information from a signature.
    pub(super) fn extract_params_from_signature(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        let mut params: Vec<tsz_solver::ParamInfo> = Vec::new();
        let mut this_type = None;

        if let Some(ref param_list) = sig.parameters {
            for &param_idx in &param_list.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                let name = self.get_param_name(param_data.name);

                if name == "this" {
                    this_type = (param_data.type_annotation.is_some())
                        .then(|| self.check(param_data.type_annotation));
                    continue;
                }

                // Later parameter annotations can reference earlier value
                // parameters via `typeof`.
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.insert(name, param.type_id);
                    }
                }
                let type_id = if param_data.type_annotation.is_some() {
                    self.check(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };
                for param in &params {
                    if let Some(name_atom) = param.name {
                        let name = self.ctx.types.resolve_atom(name_atom);
                        self.ctx.typeof_param_scope.remove(&name);
                    }
                }

                let optional = param_data.question_token || param_data.initializer.is_some();
                let rest = param_data.dot_dot_dot_token;

                let sig_type_id = if param_data.question_token
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && type_id != TypeId::ERROR
                    && !crate::query_boundaries::common::type_contains_undefined(
                        self.ctx.types,
                        type_id,
                    ) {
                    type_construction::type_node_union(
                        self.ctx.types,
                        vec![type_id, TypeId::UNDEFINED],
                    )
                } else {
                    type_id
                };
                params.push(signature_building_boundary::param_info(
                    Some(self.ctx.types.intern_string(&name)),
                    sig_type_id,
                    optional,
                    rest,
                ));
            }
        }

        (params, this_type)
    }

    /// Resolve return type annotation with parameter names in scope for `typeof`.
    ///
    /// Pushes parameter names into `typeof_param_scope` so that `typeof paramName`
    /// in the return type annotation resolves to the parameter's declared type.
    pub(super) fn resolve_return_type_with_params_in_scope(
        &mut self,
        type_annotation: NodeIndex,
        params: &[tsz_solver::ParamInfo],
    ) -> TypeId {
        if type_annotation.is_none() {
            return TypeId::ANY;
        }

        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.insert(name, param.type_id);
            }
        }

        let return_type = self.check(type_annotation);

        for param in params {
            if let Some(name_atom) = param.name {
                let name = self.ctx.types.resolve_atom(name_atom);
                self.ctx.typeof_param_scope.remove(&name);
            }
        }

        return_type
    }

    /// Get parameter name from a binding name node.
    fn get_param_name(&self, name_idx: NodeIndex) -> String {
        if self
            .ctx
            .arena
            .get(name_idx)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
        {
            return "this".to_string();
        }
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) {
            return ident.escaped_text.to_string();
        }
        "_".to_string()
    }
}
