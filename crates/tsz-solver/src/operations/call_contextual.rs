use super::{AssignabilityChecker, CallEvaluator};
use crate::types::{TemplateSpan, TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn is_assignable_via_contextual_signatures_strict(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let normalize = |shape: crate::types::FunctionShape| {
            use crate::type_queries::unpack_tuple_rest_parameter;

            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
                .collect();
            normalized
        };
        let source = self.instantiate_generic_function_argument_against_target(source, target);
        let Some(source_fn) = Self::get_contextual_signature_cached(self.interner, source) else {
            return false;
        };
        let Some(target_fn) = Self::get_contextual_signature_cached(self.interner, target) else {
            return false;
        };
        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);

        self.checker.is_assignable_to_strict(
            self.interner.function(source_fn),
            self.interner.function(target_fn),
        )
    }

    /// Check if a callback argument has more required parameters than the target
    /// callback can accept. This is a pre-check that runs before bivariant callback
    /// assignability, because bivariance only relaxes parameter TYPE checking, not
    /// parameter COUNT checking.
    ///
    /// In TypeScript, `(items: X) => void` is NOT assignable to `() => any` because
    /// the source requires 1 argument but the target is called with 0.
    /// This mirrors tsc's behavior where function arity is enforced even in bivariant
    /// callback positions.
    pub(super) fn callback_source_has_excess_required_params(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(source_fn) = Self::get_contextual_signature_cached(self.interner, source) else {
            return false;
        };
        let Some(target_fn) = Self::get_contextual_signature_cached(self.interner, target) else {
            return false;
        };

        // If target has a rest parameter, the arity is effectively unlimited
        // (handled by the existing generic rest check or the full subtype check).
        let target_has_rest = target_fn.params.last().is_some_and(|p| p.rest);
        if target_has_rest {
            return false;
        }

        let source_required = crate::utils::required_param_count(&source_fn.params);
        let target_fixed_count = target_fn.params.len();

        // Extra source params of type `void` are effectively optional in TypeScript
        if source_required > target_fixed_count {
            let extra_are_void = source_fn
                .params
                .iter()
                .skip(target_fixed_count)
                .take(source_required.saturating_sub(target_fixed_count))
                .all(|param| {
                    param.type_id == TypeId::VOID
                        || if let Some(crate::TypeData::Union(list_id)) =
                            self.interner.lookup(param.type_id)
                        {
                            self.interner.type_list(list_id).contains(&TypeId::VOID)
                        } else {
                            false
                        }
                });
            return !extra_are_void;
        }

        false
    }

    pub(super) fn callback_requires_more_fixed_params_than_generic_rest_allows(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let normalize = |shape: crate::types::FunctionShape| {
            use crate::type_queries::unpack_tuple_rest_parameter;

            let mut normalized = shape.clone();
            normalized.params = shape
                .params
                .iter()
                .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
                .collect();
            normalized
        };

        let Some(source_fn) = Self::get_contextual_signature_cached(self.interner, source) else {
            return false;
        };
        let Some(target_fn) = Self::get_contextual_signature_cached(self.interner, target) else {
            return false;
        };

        let source_fn = normalize(source_fn);
        let target_fn = normalize(target_fn);
        let Some(target_rest) = target_fn.params.last().filter(|param| param.rest) else {
            return false;
        };

        let target_rest_is_generic =
            crate::type_queries::is_type_parameter_like(self.interner, target_rest.type_id)
                || crate::type_queries::contains_type_parameters_db(
                    self.interner,
                    target_rest.type_id,
                );

        if !target_rest_is_generic {
            return false;
        }

        let source_required = crate::utils::required_param_count(&source_fn.params);
        let target_fixed_count = target_fn.params.len().saturating_sub(1);
        source_required > target_fixed_count
    }

    pub(super) fn type_uses_inference_placeholders(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::TypeParameter(info)) => {
                let name = self.interner.resolve_atom(info.name);
                name.as_str().starts_with("__infer_")
                    || info
                        .constraint
                        .is_some_and(|constraint| self.type_uses_inference_placeholders(constraint))
            }
            Some(TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| self.type_uses_inference_placeholders(constraint)),
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                self.function_signature_is_contextually_sensitive(&shape.params)
                    || self.type_uses_inference_placeholders(shape.return_type)
            }
            // Callable types represent class constructor values (pre-existing,
            // never contextually sensitive). Merged with default arm below.
            Some(TypeData::Union(members)) | Some(TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_uses_inference_placeholders(member)),
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .properties
                .iter()
                .any(|prop| self.type_uses_inference_placeholders(prop.type_id)),
            Some(TypeData::Array(elem))
            | Some(TypeData::ReadonlyType(elem))
            | Some(TypeData::NoInfer(elem))
            | Some(TypeData::KeyOf(elem))
            | Some(TypeData::Enum(_, elem)) => self.type_uses_inference_placeholders(elem),
            Some(TypeData::Tuple(elements)) => self
                .interner
                .tuple_list(elements)
                .iter()
                .any(|elem| self.type_uses_inference_placeholders(elem.type_id)),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.type_uses_inference_placeholders(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_uses_inference_placeholders(arg))
            }
            Some(TypeData::IndexAccess(obj, key)) => {
                self.type_uses_inference_placeholders(obj)
                    || self.type_uses_inference_placeholders(key)
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_uses_inference_placeholders(cond.check_type)
                    || self.type_uses_inference_placeholders(cond.extends_type)
                    || self.type_uses_inference_placeholders(cond.true_type)
                    || self.type_uses_inference_placeholders(cond.false_type)
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.type_uses_inference_placeholders(mapped.constraint)
                    || self.type_uses_inference_placeholders(mapped.template)
            }
            Some(TypeData::StringIntrinsic { type_arg, .. }) => {
                self.type_uses_inference_placeholders(type_arg)
            }
            Some(TypeData::TemplateLiteral(spans)) => self
                .interner
                .template_list(spans)
                .iter()
                .any(|span| match span {
                    crate::types::TemplateSpan::Text(_) => false,
                    crate::types::TemplateSpan::Type(inner) => {
                        self.type_uses_inference_placeholders(*inner)
                    }
                }),
            _ => false,
        }
    }

    /// Check if a type is contextually sensitive (requires contextual typing for inference).
    ///
    /// Contextually sensitive types include:
    /// - Function types (lambda expressions)
    /// - Callable types (object with call signatures)
    /// - Union/Intersection types containing contextually sensitive members
    /// - Object literals with callable properties (methods)
    ///
    /// These types need deferred inference in Round 2 after non-contextual
    /// arguments have been processed and type variables have been fixed.
    pub(crate) fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        // Check memoization cache to avoid exponential re-traversal on deeply
        // nested type structures (e.g., Application chains where each level
        // references the previous type multiple times via keyof).
        if let Some(&cached) = self.contextual_sensitivity_cache.borrow().get(&type_id) {
            return cached;
        }
        let result = self.is_contextually_sensitive_inner(type_id);
        self.contextual_sensitivity_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    fn is_contextually_sensitive_inner(&self, type_id: TypeId) -> bool {
        let key = match self.interner.lookup(type_id) {
            Some(key) => key,
            None => return false,
        };

        match key {
            // Function types are contextually sensitive only when one of their
            // parameter types still needs contextual typing (has `any` type or
            // inference placeholder). Fully annotated function arguments --
            // including generic function references like `id<T>(x: T) => T` --
            // should participate in Round 1 generic inference.
            //
            // In tsc, contextual sensitivity is an AST-level check
            // (isContextSensitive) that looks at whether the expression is a
            // function expression/arrow with unannotated parameters. A simple
            // identifier referring to a generic function is NOT contextually
            // sensitive. We approximate this by only checking parameter types
            // for placeholder/any markers, not the presence of type_params.
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                self.function_signature_is_contextually_sensitive(&shape.params)
            }
            // Union/Intersection: contextually sensitive if any member is
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.is_contextually_sensitive(member))
            }

            // Object types: only fresh object literals can be contextually sensitive.
            // Non-fresh objects (class instances, evaluated generic types like Set<T>)
            // are never contextually sensitive — their types are already determined.
            // This matches tsc's isContextSensitive which checks the AST expression,
            // not the type: variable references are never contextually sensitive.
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .flags
                    .contains(crate::types::ObjectFlags::FRESH_LITERAL)
                    && (shape.all_properties_context_sensitive()
                        || shape
                            .properties
                            .iter()
                            .any(|prop| self.is_contextually_sensitive(prop.type_id)))
            }

            // Array types: check element type
            TypeData::Array(elem) => self.is_contextually_sensitive(elem),

            // Tuple types: check all elements
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|elem| self.is_contextually_sensitive(elem.type_id))
            }

            // Type applications: check base and arguments
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.is_contextually_sensitive(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.is_contextually_sensitive(arg))
            }

            // Readonly types: look through to inner type
            TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.is_contextually_sensitive(inner)
            }

            // Type parameters with constraints: check constraint
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info
                .constraint
                .is_some_and(|constraint| self.is_contextually_sensitive(constraint)),

            // Index access: check both object and key types
            TypeData::IndexAccess(obj, key) => {
                self.is_contextually_sensitive(obj) || self.is_contextually_sensitive(key)
            }

            // Conditional types: check all branches
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.is_contextually_sensitive(cond.check_type)
                    || self.is_contextually_sensitive(cond.extends_type)
                    || self.is_contextually_sensitive(cond.true_type)
                    || self.is_contextually_sensitive(cond.false_type)
            }

            // Mapped types: check constraint and template
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.is_contextually_sensitive(mapped.constraint)
                    || self.is_contextually_sensitive(mapped.template)
            }

            // KeyOf, StringIntrinsic: check operand
            TypeData::KeyOf(operand)
            | TypeData::StringIntrinsic {
                type_arg: operand, ..
            } => self.is_contextually_sensitive(operand),

            // Enum types: check member type
            TypeData::Enum(_def_id, member_type) => self.is_contextually_sensitive(member_type),

            // Template literals: check type spans
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.is_contextually_sensitive(*inner),
                })
            }

            // Non-contextually sensitive types (Callable = class constructor values)
            TypeData::Callable(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => false,
        }
    }
}
