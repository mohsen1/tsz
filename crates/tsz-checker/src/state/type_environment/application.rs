use crate::{query_boundaries::state::type_environment as query, state::CheckerState};
use tsz_common::Atom;
use tsz_solver::{TypeId, TypeParamInfo};

pub(crate) struct ApplicationBaseBody {
    pub(crate) body_type: TypeId,
    pub(crate) type_params: Vec<TypeParamInfo>,
}

impl<'a> CheckerState<'a> {
    /// Record semantic (non-display) provenance from an evaluated structural
    /// result back to the nominal `Application` it came from.
    ///
    /// A class/interface instantiation that checker-side evaluation lowered
    /// to a structural shape keeps a reverse link to its application so the
    /// solver relation layer can recover the generic identity for the
    /// accept-only variance fast path. Gated on the result shape carrying a
    /// nominal `symbol` (class/interface instances), so anonymous alias
    /// expansions stay unrecorded.
    pub(crate) fn record_nominal_application_eval_origin(
        &mut self,
        result: TypeId,
        application: TypeId,
    ) {
        if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, result)
            .is_some_and(|shape| shape.symbol.is_some())
        {
            self.ctx
                .types
                .as_type_database()
                .record_application_eval_origin(result, application);
        }
    }

    pub(crate) fn evaluate_application_type_for_property_access(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let Some((base, args)) = query::application_info(self.ctx.types, type_id) else {
            return type_id;
        };
        let Some(base_def_id) = query::lazy_def_id(self.ctx.types, base) else {
            return self.evaluate_application_type(type_id);
        };
        let Some(base_def_info) = self.ctx.definition_store.get(base_def_id) else {
            return self.evaluate_application_type(type_id);
        };
        if base_def_info
            .file_id
            .is_none_or(|file_id| file_id == self.ctx.current_file_idx as u32)
            || base_def_info.is_declare
            || !matches!(
                base_def_info.kind,
                tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
            )
        {
            return self.evaluate_application_type(type_id);
        }
        let Some(ApplicationBaseBody {
            body_type,
            type_params,
        }) = self.resolve_application_base_body(base)
        else {
            return self.evaluate_application_type(type_id);
        };

        if body_type == TypeId::ANY || body_type == TypeId::ERROR {
            return type_id;
        }
        if type_params.is_empty() {
            return body_type;
        }

        let evaluated_args: Vec<TypeId> = args
            .iter()
            .map(|&arg| self.evaluate_type_with_env(arg))
            .collect();
        let substitution =
            query::TypeSubstitution::from_args(self.ctx.types, &type_params, &evaluated_args);
        let mut instantiated = query::instantiate_type(self.ctx.types, body_type, &substitution);
        if query::contains_this_type(self.ctx.types, instantiated) {
            instantiated = query::substitute_this_type(self.ctx.types, instantiated, type_id);
        }
        let evaluated = self.evaluate_type_with_env(instantiated);
        if evaluated == TypeId::ERROR {
            instantiated
        } else {
            evaluated
        }
    }

    pub(crate) fn resolve_application_base_body(
        &mut self,
        base: TypeId,
    ) -> Option<ApplicationBaseBody> {
        let base_def_body = query::lazy_def_id(self.ctx.types, base).and_then(|def_id| {
            let def_info = self.ctx.definition_store.get(def_id)?;
            if def_info.is_declare
                || !matches!(
                    def_info.kind,
                    tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
                )
            {
                return None;
            }
            let body = def_info.body.or_else(|| {
                self.ctx
                    .type_env
                    .try_borrow()
                    .ok()
                    .and_then(|env| env.get_def(def_id))
            })?;
            let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
            Some((def_id, body, params))
        });

        let (mut body_type, mut type_params, base_def_id, mut base_sym_id) =
            if let Some((def_id, body, params)) = base_def_body {
                (body, params, Some(def_id), None)
            } else {
                let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
                let (body, params) = self.type_reference_symbol_type_with_params(sym_id);
                (
                    body,
                    params,
                    self.ctx.get_existing_def_id(sym_id),
                    Some(sym_id),
                )
            };

        if let Some(def_id) = base_def_id
            && query::object_shape(self.ctx.types, body_type).is_some_and(|shape| {
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            })
            && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
        {
            let (candidate_body, candidate_params) =
                self.type_reference_symbol_type_with_params(sym_id);
            let candidate_has_members =
                query::object_shape(self.ctx.types, candidate_body).is_some_and(|shape| {
                    !shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                }) || query::callable_shape(self.ctx.types, candidate_body).is_some_and(|shape| {
                    !shape.properties.is_empty()
                        || !shape.call_signatures.is_empty()
                        || !shape.construct_signatures.is_empty()
                });
            if candidate_has_members {
                body_type = candidate_body;
                if !candidate_params.is_empty() {
                    type_params = candidate_params;
                }
                base_sym_id = Some(sym_id);
            }
        }

        let is_typeof_query = query::type_query_symbol(self.ctx.types, base).is_some();
        if !is_typeof_query
            && (base_sym_id
                .and_then(|sym_id| {
                    self.get_cross_file_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(sym_id))
                })
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                || base_def_id.is_some_and(|def_id| {
                    self.ctx
                        .definition_store
                        .get(def_id)
                        .is_some_and(|info| info.kind == tsz_solver::def::DefKind::Class)
                }))
            && query::callable_shape(self.ctx.types, body_type)
                .is_some_and(|shape| !shape.construct_signatures.is_empty())
            && let Some(def_id) = base_def_id
            && let Ok(env) = self.ctx.type_env.try_borrow()
            && let Some(instance_type) = env.get_class_instance_type(def_id)
        {
            body_type = instance_type;
        }

        Some(ApplicationBaseBody {
            body_type,
            type_params,
        })
    }

    pub(crate) fn instantiate_mapped_property_template_with_env(
        &mut self,
        mapped: &tsz_solver::MappedType,
        key_name: Atom,
    ) -> TypeId {
        let key_literal = self.ctx.types.literal_string_atom(key_name);
        let property_type =
            crate::query_boundaries::state::checking::instantiate_mapped_template_for_property(
                self.ctx.types,
                mapped.template,
                mapped.type_param.name,
                key_literal,
            );

        // When the template produces an IndexAccess (e.g., T[K] → ObjType["key"]),
        // resolve the object part through evaluate_type_with_resolution so that
        // Lazy(DefId) references become concrete types.  Then attempt property
        // access resolution with the resolved object.
        if let Some((obj, _idx)) = query::index_access_types(self.ctx.types, property_type) {
            let obj_type = self.evaluate_type_with_resolution(obj);

            let prop_name_arc = self.ctx.types.resolve_atom_ref(key_name);
            let prop_name: &str = &prop_name_arc;
            match self.resolve_property_access_with_env(obj_type, prop_name) {
                tsz_solver::operations::property::PropertyAccessResult::Success {
                    type_id, ..
                }
                | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => return type_id,
                _ => {}
            }
        }

        self.evaluate_type_with_env(property_type)
    }
}
