//! Signature Building Module
//!
//! This module contains methods for building call and construct signatures.
//! It handles:
//! - Extracting parameters from function/method declarations
//! - Building CallSignature from functions, methods, and constructors
//! - Instantiating signatures with type arguments
//! - Processing return types and type predicates
//!
//! This module extends CheckerState with signature-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::{CheckerState, ParamTypeResolutionMode};
use crate::interner::Atom;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

// =============================================================================
// Signature Building Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Call Signature Building
    // =========================================================================

    /// Build a CallSignature from a function declaration/expression.
    /// `func_idx` is the node index of the function declaration, used to resolve
    /// enclosing type parameters from outer generic scopes (e.g., inner function
    /// overloads referencing outer function type parameters).
    pub(crate) fn call_signature_from_function(
        &mut self,
        func: &crate::parser::node::FunctionData,
        func_idx: crate::parser::NodeIndex,
    ) -> crate::solver::CallSignature {
        // Push enclosing type parameters so that overload signatures can reference
        // type parameters from outer function/class/interface scopes.
        let enclosing_updates = self.push_enclosing_type_parameters(func_idx);

        let (type_params, type_param_updates) = self.push_type_parameters(&func.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&func.parameters);
        let (return_type, type_predicate) = self.return_type_and_predicate(func.type_annotation);

        self.pop_type_parameters(type_param_updates);
        self.pop_type_parameters(enclosing_updates);

        crate::solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: false,
        }
    }

    /// Build a CallSignature from a method declaration.
    pub(crate) fn call_signature_from_method(
        &mut self,
        method: &crate::parser::node::MethodDeclData,
    ) -> crate::solver::CallSignature {
        self.call_signature_from_method_with_this(method, None)
    }

    /// Build a CallSignature from a method declaration with an explicit `this` type.
    /// This is used for static methods where `this` refers to the constructor type.
    pub(crate) fn call_signature_from_method_with_this(
        &mut self,
        method: &crate::parser::node::MethodDeclData,
        explicit_this_type: Option<TypeId>,
    ) -> crate::solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&method.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&method.parameters);
        let (return_type, type_predicate) =
            if method.type_annotation.is_none() && !method.body.is_none() {
                // Infer return type from body when there's no annotation
                // Push the this type for proper resolution
                let pushed_this = if let Some(this_ty) = explicit_this_type {
                    self.ctx.this_type_stack.push(this_ty);
                    true
                } else {
                    false
                };
                let inferred = self.infer_return_type_from_body(method.body, None);
                if pushed_this {
                    self.ctx.this_type_stack.pop();
                }
                (inferred, None)
            } else {
                self.return_type_and_predicate(method.type_annotation)
            };

        self.pop_type_parameters(type_param_updates);

        crate::solver::CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: true,
        }
    }

    /// Build a CallSignature from a constructor declaration.
    pub(crate) fn call_signature_from_constructor(
        &mut self,
        ctor: &crate::parser::node::ConstructorData,
        instance_type: TypeId,
        class_type_params: &[crate::solver::TypeParamInfo],
    ) -> crate::solver::CallSignature {
        let (type_params, type_param_updates) = self.push_type_parameters(&ctor.type_parameters);
        let (params, this_type) = self.extract_params_from_parameter_list(&ctor.parameters);

        self.pop_type_parameters(type_param_updates);

        let mut all_type_params = Vec::with_capacity(class_type_params.len() + type_params.len());
        all_type_params.extend_from_slice(class_type_params);
        all_type_params.extend(type_params);

        crate::solver::CallSignature {
            type_params: all_type_params,
            params,
            this_type,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }
    }

    // =========================================================================
    // Signature Instantiation
    // =========================================================================

    /// Instantiate a call signature with type arguments.
    /// Similar to instantiate_constructor_signature but for call signatures.
    pub(crate) fn instantiate_call_signature(
        &self,
        sig: &crate::solver::CallSignature,
        type_args: &[TypeId],
    ) -> crate::solver::CallSignature {
        use crate::solver::{ParamInfo, TypeSubstitution, instantiate_type};

        let substitution = TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
        let params: Vec<ParamInfo> = sig
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(self.ctx.types, param.type_id, &substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();

        let this_type = sig
            .this_type
            .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution));
        let return_type = instantiate_type(self.ctx.types, sig.return_type, &substitution);
        let type_predicate =
            sig.type_predicate
                .as_ref()
                .map(|predicate| crate::solver::TypePredicate {
                    asserts: predicate.asserts,
                    target: predicate.target.clone(),
                    type_id: predicate
                        .type_id
                        .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution)),
                });

        crate::solver::CallSignature {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: sig.is_method,
        }
    }

    /// Instantiate a constructor signature with type arguments.
    pub(crate) fn instantiate_constructor_signature(
        &self,
        sig: &crate::solver::CallSignature,
        type_args: &[TypeId],
    ) -> crate::solver::CallSignature {
        use crate::solver::{
            CallSignature, ParamInfo, TypePredicate, TypeSubstitution, instantiate_type,
        };

        let substitution = TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
        let params: Vec<ParamInfo> = sig
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name,
                type_id: instantiate_type(self.ctx.types, param.type_id, &substitution),
                optional: param.optional,
                rest: param.rest,
            })
            .collect();

        let this_type = sig
            .this_type
            .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution));
        let return_type = instantiate_type(self.ctx.types, sig.return_type, &substitution);
        let type_predicate = sig.type_predicate.as_ref().map(|predicate| TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target.clone(),
            type_id: predicate
                .type_id
                .map(|type_id| instantiate_type(self.ctx.types, type_id, &substitution)),
        });

        CallSignature {
            type_params: Vec::new(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: sig.is_method,
        }
    }

    // =========================================================================
    // Parameter Extraction
    // =========================================================================

    /// Helper to extract parameters from a SignatureData.
    pub(crate) fn extract_params_from_signature(
        &mut self,
        sig: &crate::parser::node::SignatureData,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        self.extract_params_from_parameter_list_impl(params_list, ParamTypeResolutionMode::OfNode)
    }

    /// Helper to extract parameters from a parameter list.
    pub(crate) fn extract_params_from_parameter_list(
        &mut self,
        params_list: &crate::parser::NodeList,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::FromTypeNode,
        )
    }

    /// Unified implementation for extracting parameters from a parameter list.
    /// The `mode` parameter controls which type resolution method is used.
    pub(crate) fn extract_params_from_parameter_list_impl(
        &mut self,
        params_list: &crate::parser::NodeList,
        mode: ParamTypeResolutionMode,
    ) -> (Vec<crate::solver::ParamInfo>, Option<TypeId>) {
        use crate::solver::ParamInfo;

        let mut params = Vec::new();
        let mut this_type = None;
        let this_atom = self.ctx.types.intern_string("this");

        for &param_idx in &params_list.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            // Resolve parameter type based on mode
            let type_id = if !param.type_annotation.is_none() {
                match mode {
                    ParamTypeResolutionMode::InTypeLiteral => {
                        self.get_type_from_type_node_in_type_literal(param.type_annotation)
                    }
                    ParamTypeResolutionMode::FromTypeNode => {
                        self.get_type_from_type_node(param.type_annotation)
                    }
                    ParamTypeResolutionMode::OfNode => self.get_type_of_node(param.type_annotation),
                }
            } else {
                TypeId::ANY
            };

            // Check for ThisKeyword parameter
            let name_node = self.ctx.arena.get(param.name);
            if let Some(name_node) = name_node
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                if this_type.is_none() {
                    this_type = Some(type_id);
                }
                continue;
            }

            // Extract parameter name
            let name: Option<Atom> = if let Some(name_node) = name_node {
                if let Some(name_data) = self.ctx.arena.get_identifier(name_node) {
                    Some(self.ctx.types.intern_string(&name_data.escaped_text))
                } else {
                    None
                }
            } else {
                None
            };

            let optional = param.question_token || !param.initializer.is_none();
            let rest = param.dot_dot_dot_token;

            // Check for "this" parameter by name
            if let Some(name_atom) = name
                && name_atom == this_atom
            {
                if this_type.is_none() {
                    this_type = Some(type_id);
                }
                continue;
            }

            params.push(ParamInfo {
                name,
                type_id,
                optional,
                rest,
            });
        }

        (params, this_type)
    }

    // =========================================================================
    // Return Type and Type Predicate
    // =========================================================================

    /// Extract return type and type predicate from a type annotation.
    pub(crate) fn return_type_and_predicate(
        &mut self,
        type_annotation: NodeIndex,
    ) -> (TypeId, Option<crate::solver::TypePredicate>) {
        use crate::solver::TypePredicate;

        if type_annotation.is_none() {
            // Return UNKNOWN instead of ANY to enforce strict type checking
            return (TypeId::UNKNOWN, None);
        }

        let Some(node) = self.ctx.arena.get(type_annotation) else {
            return (TypeId::UNKNOWN, None);
        };

        if node.kind != syntax_kind_ext::TYPE_PREDICATE {
            return (self.get_type_from_type_node(type_annotation), None);
        }

        let Some(data) = self.ctx.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node.is_none() {
            None
        } else {
            Some(self.get_type_from_type_node(data.type_node))
        };

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
        };

        (return_type, Some(predicate))
    }

    /// Extract return type and type predicate from a type literal annotation.
    pub(crate) fn return_type_and_predicate_in_type_literal(
        &mut self,
        type_annotation: NodeIndex,
    ) -> (TypeId, Option<crate::solver::TypePredicate>) {
        use crate::solver::TypePredicate;

        if type_annotation.is_none() {
            // Return UNKNOWN instead of ANY for missing type annotation
            return (TypeId::UNKNOWN, None);
        }

        let Some(node) = self.ctx.arena.get(type_annotation) else {
            // Return UNKNOWN instead of ANY for missing node
            return (TypeId::UNKNOWN, None);
        };

        if node.kind != syntax_kind_ext::TYPE_PREDICATE {
            return (
                self.get_type_from_type_node_in_type_literal(type_annotation),
                None,
            );
        }

        let Some(data) = self.ctx.arena.get_type_predicate(node) else {
            return (TypeId::BOOLEAN, None);
        };

        let return_type = if data.asserts_modifier {
            TypeId::VOID
        } else {
            TypeId::BOOLEAN
        };

        let target = match self.type_predicate_target(data.parameter_name) {
            Some(target) => target,
            None => return (return_type, None),
        };

        let type_id = if data.type_node.is_none() {
            None
        } else {
            Some(self.get_type_from_type_node_in_type_literal(data.type_node))
        };

        let predicate = TypePredicate {
            asserts: data.asserts_modifier,
            target,
            type_id,
        };

        (return_type, Some(predicate))
    }
}
