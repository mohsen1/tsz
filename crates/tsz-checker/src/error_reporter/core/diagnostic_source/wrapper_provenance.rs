//! Structured wrapper-display provenance for TS2322-family diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn is_nested_same_wrapper_assignment_display_provenance(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        if self.is_nested_same_wrapper_application_assignment(source, target) {
            return true;
        }

        for expr_idx in [
            self.direct_diagnostic_source_expression(anchor_idx),
            self.assignment_source_expression(anchor_idx),
        ]
        .into_iter()
        .flatten()
        {
            let Some(display_source) =
                self.assignment_source_wrapper_display_type_candidate(source, target, expr_idx)
            else {
                continue;
            };
            if self.is_nested_same_wrapper_application_assignment(display_source, target) {
                return true;
            }
            if self.display_provenance_is_nested_same_wrapper(display_source, target) {
                return true;
            }
        }

        false
    }

    fn display_provenance_is_nested_same_wrapper(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((source_symbol, source_args)) = self.display_provenance_application_info(source)
        else {
            return false;
        };
        let Some((target_symbol, target_args)) = self.display_provenance_application_info(target)
        else {
            return false;
        };
        if source_symbol != target_symbol || source_args.len() != 1 || target_args.len() != 1 {
            return false;
        }
        let source_arg_is_same_wrapper = self
            .display_provenance_application_info(source_args[0])
            .is_some_and(|(symbol, _)| symbol == source_symbol);
        let target_arg_is_same_wrapper = self
            .display_provenance_application_info(target_args[0])
            .is_some_and(|(symbol, _)| symbol == target_symbol);
        source_arg_is_same_wrapper && !target_arg_is_same_wrapper
    }

    fn display_provenance_application_info(
        &mut self,
        type_id: TypeId,
    ) -> Option<(tsz_binder::SymbolId, Vec<TypeId>)> {
        if let Some((base, args)) = self.application_info_or_display_alias(type_id) {
            let symbol = self.display_provenance_type_symbol(base)?;
            return Some((symbol, args));
        }

        let shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)?;
        let symbol = shape.symbol?;
        let def_id = self.ctx.get_or_create_def_id(symbol);
        let type_param_count = self
            .ctx
            .get_def_type_params(def_id)
            .map_or(0, |params| params.len());
        if type_param_count == 0 || shape.properties.len() < type_param_count {
            return None;
        }

        let mut candidates = self
            .display_provenance_property_candidates(&shape.properties, |prop| {
                !prop.is_method && !prop.is_class_prototype
            });
        if candidates.len() < type_param_count {
            candidates = self
                .display_provenance_property_candidates(&shape.properties, |prop| !prop.is_method);
        }
        if candidates.len() < type_param_count {
            candidates = self.display_provenance_property_candidates(&shape.properties, |prop| {
                !prop.is_class_prototype
            });
        }
        if candidates.len() < type_param_count {
            candidates = self.display_provenance_property_candidates(&shape.properties, |_| true);
        }
        if candidates.len() < type_param_count {
            return None;
        }

        Some((
            symbol,
            candidates
                .into_iter()
                .take(type_param_count)
                .map(|(_, type_id)| type_id)
                .collect(),
        ))
    }

    fn display_provenance_type_symbol(&self, type_id: TypeId) -> Option<tsz_binder::SymbolId> {
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
        {
            return self.ctx.def_to_symbol_id(def_id);
        }
        self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .and_then(|shape| shape.symbol)
        })
    }

    fn display_provenance_property_candidates(
        &self,
        properties: &[tsz_solver::PropertyInfo],
        predicate: fn(&tsz_solver::PropertyInfo) -> bool,
    ) -> Vec<(String, TypeId)> {
        let mut candidates: Vec<(String, TypeId)> = properties
            .iter()
            .filter(|prop| predicate(prop))
            .filter_map(|prop| {
                let name = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                if tsz_solver::utils::is_synthetic_private_brand_name(&name) {
                    None
                } else {
                    Some((name, self.display_provenance_property_type(prop)))
                }
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        candidates
    }

    fn display_provenance_property_type(&self, prop: &tsz_solver::PropertyInfo) -> TypeId {
        if !prop.is_method {
            return prop.type_id;
        }

        let extract_from_shape = |params: &[tsz_solver::ParamInfo], return_type: TypeId| {
            if !matches!(
                return_type,
                TypeId::VOID
                    | TypeId::NEVER
                    | TypeId::ANY
                    | TypeId::UNKNOWN
                    | TypeId::UNDEFINED
                    | TypeId::NULL
            ) {
                return return_type;
            }
            params
                .iter()
                .find(|param| {
                    !matches!(
                        param.type_id,
                        TypeId::VOID
                            | TypeId::NEVER
                            | TypeId::ANY
                            | TypeId::UNKNOWN
                            | TypeId::UNDEFINED
                            | TypeId::NULL
                    )
                })
                .map_or(return_type, |param| param.type_id)
        };

        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, prop.type_id)
        {
            return extract_from_shape(&fn_shape.params, fn_shape.return_type);
        }
        if let Some(callable) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, prop.type_id)
            && callable.call_signatures.len() == 1
        {
            let sig = &callable.call_signatures[0];
            return extract_from_shape(&sig.params, sig.return_type);
        }
        prop.type_id
    }

    fn assignment_source_wrapper_display_type_candidate(
        &mut self,
        source: TypeId,
        target: TypeId,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let expr_type = self.get_type_of_node(expr_idx);
        let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
            source
        } else {
            expr_type
        };

        if expr_display_type == TypeId::ERROR {
            return Some(self.widen_type_for_display(source));
        }
        if expr_display_type == target && expr_display_type != source {
            return None;
        }

        let preserve_literal_surface = self.target_preserves_literal_surface(target);
        let widened = if preserve_literal_surface {
            expr_display_type
        } else {
            self.widen_type_for_display(expr_display_type)
        };
        let display_type = if self.should_widen_enum_member_assignment_source(widened, target) {
            self.widen_enum_member_type(widened)
        } else {
            widened
        };

        Some(self.widen_function_like_display_type(display_type))
    }
}
