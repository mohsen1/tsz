//! Class and Interface Checking Module
//!
//! This module contains methods for checking class and interface declarations.
//! It handles:
//! - Property inheritance compatibility (TS2416)
//! - Interface extension compatibility (TS2430)
//! - Abstract member implementations (TS2654)
//! - Implements clause validation (TS2420)
//!
//! This module extends CheckerState with class/interface-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::SyntaxKind;
use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::diagnostic_codes;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::solver::TypeId;

// =============================================================================
// Class and Interface Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Inheritance Checking
    // =========================================================================

    /// Check that property types in derived class are compatible with base class (error 2416).
    /// For each property/accessor in the derived class, checks if there's a corresponding
    /// member in the base class with incompatible type.
    pub(crate) fn check_property_inheritance_compatibility(
        &mut self,
        _class_idx: NodeIndex,
        class_data: &crate::parser::node::ClassData,
    ) {
        use crate::solver::{TypeSubstitution, instantiate_type};

        // Find base class from heritage clauses (extends, not implements)
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();
        let mut base_type_argument_nodes: Option<Vec<NodeIndex>> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            if let Some(&type_idx) = heritage.types.nodes.first()
                && let Some(type_node) = self.ctx.arena.get(type_idx)
            {
                // Handle both cases:
                // 1. ExpressionWithTypeArguments (e.g., Base<T>)
                // 2. Simple Identifier (e.g., Base)
                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        // For simple identifiers without type arguments, the type_node itself is the identifier
                        (type_idx, None)
                    };
                if let Some(args) = type_arguments {
                    base_type_argument_nodes = Some(args.nodes.clone());
                }

                // Get the class name from the expression (identifier)
                if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                {
                    base_class_name = ident.escaped_text.clone();

                    // Find the base class declaration via symbol lookup
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&base_class_name)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        // Try value_declaration first, then declarations
                        if !symbol.value_declaration.is_none() {
                            base_class_idx = Some(symbol.value_declaration);
                        } else if let Some(&decl_idx) = symbol.declarations.first() {
                            base_class_idx = Some(decl_idx);
                        }
                    }
                }
            }
            break; // Only one extends clause is valid
        }

        // If no base class found, nothing to check
        let Some(base_idx) = base_class_idx else {
            return;
        };

        // Get the base class data
        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return;
        };

        let mut type_args = Vec::new();
        if let Some(nodes) = base_type_argument_nodes {
            for arg_idx in nodes {
                type_args.push(self.get_type_from_type_node(arg_idx));
            }
        }

        let (base_type_params, base_type_param_updates) =
            self.push_type_parameters(&base_class.type_parameters);
        if type_args.len() < base_type_params.len() {
            for param in base_type_params.iter().skip(type_args.len()) {
                let fallback = param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN);
                type_args.push(fallback);
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }
        let substitution =
            TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);

        // Get the derived class name for the error message
        let derived_class_name = if !class_data.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        // Check each member in the derived class
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Get the member name, type, and whether it's a method
            // Methods use bivariant checking, properties/accessors use contravariant checking
            // Also track static modifier to ensure static only overrides static
            let (member_name, member_type, member_name_idx, is_method, is_static) =
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(prop.name) else {
                            continue;
                        };

                        // Track whether this is a static property (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&prop.modifiers);

                        // Get the type: either from annotation or inferred from initializer
                        let prop_type = if !prop.type_annotation.is_none() {
                            self.get_type_from_type_node(prop.type_annotation)
                        } else if !prop.initializer.is_none() {
                            self.get_type_of_node(prop.initializer)
                        } else {
                            TypeId::ANY
                        };

                        (name, prop_type, prop.name, false, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(method.name) else {
                            continue;
                        };

                        // Track whether this is a static method (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&method.modifiers);

                        // Build method type with is_method: true for bivariant checking
                        use crate::solver::FunctionShape;
                        let signature = self.call_signature_from_method(method);
                        let method_type = self.ctx.types.function(FunctionShape {
                            type_params: signature.type_params,
                            params: signature.params,
                            this_type: signature.this_type,
                            return_type: signature.return_type,
                            type_predicate: signature.type_predicate,
                            is_constructor: false,
                            is_method: true, // Critical: marks this as a method for bivariant checking
                        });

                        (name, method_type, method.name, true, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(accessor.name) else {
                            continue;
                        };

                        // Track whether this is a static accessor (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&accessor.modifiers);

                        // Get the return type
                        let accessor_type = if !accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        };

                        (name, accessor_type, accessor.name, false, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                        let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(accessor.name) else {
                            continue;
                        };

                        // Track whether this is a static accessor (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&accessor.modifiers);

                        // Get the parameter type (setters have one parameter)
                        let accessor_type =
                            if let Some(&first_param) = accessor.parameters.nodes.first() {
                                if let Some(param_node) = self.ctx.arena.get(first_param) {
                                    if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                        if !param.type_annotation.is_none() {
                                            self.get_type_from_type_node(param.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        }
                                    } else {
                                        TypeId::ANY
                                    }
                                } else {
                                    TypeId::ANY
                                }
                            } else {
                                TypeId::ANY
                            };

                        (name, accessor_type, accessor.name, false, is_static) // (is_method, is_static)
                    }
                    _ => continue,
                };

            // Skip if type is ANY (no meaningful check)
            if member_type == TypeId::ANY {
                continue;
            }

            // Look for a matching member in the base class
            for &base_member_idx in &base_class.members.nodes {
                let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                    continue;
                };

                let (base_name, base_type, _base_is_method, base_is_static) = match base_member_node
                    .kind
                {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(base_prop) = self.ctx.arena.get_property_decl(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_prop.name) else {
                            continue;
                        };

                        // Skip private members (they trigger different errors, not TS2416)
                        if self.has_private_modifier(&base_prop.modifiers) {
                            continue;
                        }

                        // Track whether this is a static property (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&base_prop.modifiers);

                        let prop_type = if !base_prop.type_annotation.is_none() {
                            self.get_type_from_type_node(base_prop.type_annotation)
                        } else if !base_prop.initializer.is_none() {
                            self.get_type_of_node(base_prop.initializer)
                        } else {
                            TypeId::ANY
                        };

                        (name, prop_type, false, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(base_method) = self.ctx.arena.get_method_decl(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_method.name) else {
                            continue;
                        };

                        // Skip private members (they trigger different errors, not TS2416)
                        if self.has_private_modifier(&base_method.modifiers) {
                            continue;
                        }

                        // Track whether this is a static method (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&base_method.modifiers);

                        // Build method type with is_method: true for bivariant checking
                        use crate::solver::FunctionShape;
                        let signature = self.call_signature_from_method(base_method);
                        let method_type = self.ctx.types.function(FunctionShape {
                            type_params: signature.type_params,
                            params: signature.params,
                            this_type: signature.this_type,
                            return_type: signature.return_type,
                            type_predicate: signature.type_predicate,
                            is_constructor: false,
                            is_method: true,
                        });

                        (name, method_type, true, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                        let Some(base_accessor) = self.ctx.arena.get_accessor(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_accessor.name) else {
                            continue;
                        };

                        // Skip private members (they trigger different errors, not TS2416)
                        if self.has_private_modifier(&base_accessor.modifiers) {
                            continue;
                        }

                        // Track whether this is a static accessor (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&base_accessor.modifiers);

                        let accessor_type = if !base_accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(base_accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(base_accessor.body)
                        };

                        (name, accessor_type, false, is_static) // (is_method, is_static)
                    }
                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                        let Some(base_accessor) = self.ctx.arena.get_accessor(base_member_node)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(base_accessor.name) else {
                            continue;
                        };

                        // Skip private members (they trigger different errors, not TS2416)
                        if self.has_private_modifier(&base_accessor.modifiers) {
                            continue;
                        }

                        // Track whether this is a static accessor (don't skip - static members are checked)
                        let is_static = self.has_static_modifier(&base_accessor.modifiers);

                        // Get the parameter type (setters have one parameter)
                        let accessor_type =
                            if let Some(&first_param) = base_accessor.parameters.nodes.first() {
                                if let Some(param_node) = self.ctx.arena.get(first_param) {
                                    if let Some(param) = self.ctx.arena.get_parameter(param_node) {
                                        if !param.type_annotation.is_none() {
                                            self.get_type_from_type_node(param.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        }
                                    } else {
                                        TypeId::ANY
                                    }
                                } else {
                                    TypeId::ANY
                                }
                            } else {
                                TypeId::ANY
                            };

                        (name, accessor_type, false, is_static) // (is_method, is_static)
                    }
                    _ => continue,
                };

                let base_type = instantiate_type(self.ctx.types, base_type, &substitution);

                // Skip if base type is ANY
                if base_type == TypeId::ANY {
                    continue;
                }

                // Check if names match
                if member_name != base_name {
                    continue;
                }

                // Static members can only override static members, instance only instance
                if is_static != base_is_static {
                    continue;
                }

                // Resolve TypeQuery types (typeof) before comparison
                // If member_type is `typeof y` and base_type is `typeof x`,
                // we need to compare the actual types of y and x
                let resolved_member_type = self.resolve_type_query_type(member_type);
                let resolved_base_type = self.resolve_type_query_type(base_type);

                // Check type compatibility - derived type must be assignable to base type
                // Use bivariant checking for methods (is_method = true), contravariant for properties
                let is_compatible = if is_method {
                    // Methods are bivariant in TypeScript for compatibility reasons
                    self.is_assignable_to_bivariant(resolved_member_type, resolved_base_type)
                } else {
                    // Properties and accessors use standard (contravariant with strictFunctionTypes) checking
                    self.is_assignable_to(resolved_member_type, resolved_base_type)
                };

                if !is_compatible {
                    // Format type strings for error message
                    let member_type_str = self.format_type(member_type);
                    let base_type_str = self.format_type(base_type);

                    // Report error 2416 on the member name
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{}' in type '{}' is not assignable to the same property in base type '{}'.",
                            member_name, derived_class_name, base_class_name
                        ),
                        diagnostic_codes::PROPERTY_NOT_ASSIGNABLE_TO_SAME_IN_BASE,
                    );

                    // Add secondary error with type details
                    if let Some((pos, end)) = self.get_node_span(member_name_idx) {
                        self.error(
                            pos,
                            end - pos,
                            format!(
                                "Type '{}' is not assignable to type '{}'.",
                                member_type_str, base_type_str
                            ),
                            diagnostic_codes::PROPERTY_NOT_ASSIGNABLE_TO_SAME_IN_BASE,
                        );
                    }
                }

                break; // Found matching base member, no need to continue
            }
        }

        self.pop_type_parameters(base_type_param_updates);
    }

    /// Check that interface correctly extends its base interfaces (error 2430).
    /// For each member in the derived interface, checks if the same member in a base interface
    /// has an incompatible type.
    pub(crate) fn check_interface_extension_compatibility(
        &mut self,
        _iface_idx: NodeIndex,
        iface_data: &crate::parser::node::InterfaceData,
    ) {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::{TypeSubstitution, instantiate_type};

        // Get heritage clauses (extends)
        let Some(ref heritage_clauses) = iface_data.heritage_clauses else {
            return;
        };

        // Get the derived interface name for the error message
        let derived_name = if !iface_data.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(iface_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        let mut derived_members: Vec<(String, TypeId, NodeIndex, u16)> = Vec::new();
        for &member_idx in &iface_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind != METHOD_SIGNATURE && member_node.kind != PROPERTY_SIGNATURE {
                continue;
            }

            let kind = member_node.kind;
            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            let Some(name) = self.get_property_name(sig.name) else {
                continue;
            };
            let type_id = self.get_type_of_interface_member(member_idx);
            derived_members.push((name, type_id, member_idx, kind));
        }

        // Process each heritage clause (extends)
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Process each extended interface
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };

                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    continue;
                };

                let base_name = self
                    .heritage_name_text(expr_idx)
                    .unwrap_or_else(|| base_symbol.escaped_name.clone());

                let mut base_iface_indices = Vec::new();
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_interface(node).is_some()
                    {
                        base_iface_indices.push(decl_idx);
                    }
                }
                if base_iface_indices.is_empty() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_interface(node).is_some()
                    {
                        base_iface_indices.push(decl_idx);
                    }
                }

                let Some(&base_root_idx) = base_iface_indices.first() else {
                    continue;
                };

                let Some(base_root_node) = self.ctx.arena.get(base_root_idx) else {
                    continue;
                };

                let Some(base_root_iface) = self.ctx.arena.get_interface(base_root_node) else {
                    continue;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_root_iface.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let substitution =
                    TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);

                for (member_name, member_type, derived_member_idx, derived_kind) in &derived_members
                {
                    let mut found = false;

                    for &base_iface_idx in &base_iface_indices {
                        let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                            continue;
                        };
                        let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                            continue;
                        };

                        for &base_member_idx in &base_iface.members.nodes {
                            let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                                continue;
                            };

                            let (base_member_name, base_type) = if base_member_node.kind
                                == METHOD_SIGNATURE
                                || base_member_node.kind == PROPERTY_SIGNATURE
                            {
                                if let Some(sig) = self.ctx.arena.get_signature(base_member_node) {
                                    if let Some(name) = self.get_property_name(sig.name) {
                                        let type_id =
                                            self.get_type_of_interface_member(base_member_idx);
                                        (name, type_id)
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            };

                            if *member_name != base_member_name {
                                continue;
                            }

                            found = true;
                            let base_type =
                                instantiate_type(self.ctx.types, base_type, &substitution);

                            // For method signatures, also check required parameter
                            // count: derived methods must not require more parameters
                            // than the base method provides. This catches the
                            // "target signature provides too few arguments" case.
                            let param_count_incompatible = if *derived_kind == METHOD_SIGNATURE
                                && base_member_node.kind == METHOD_SIGNATURE
                            {
                                let derived_required = self
                                    .count_required_params_from_signature_node(*derived_member_idx);
                                let base_required =
                                    self.count_required_params_from_signature_node(base_member_idx);
                                derived_required > base_required
                            } else {
                                false
                            };

                            if param_count_incompatible
                                || !self.is_assignable_to(*member_type, base_type)
                            {
                                let member_type_str = self.format_type(*member_type);
                                let base_type_str = self.format_type(base_type);

                                self.error_at_node(
                                    iface_data.name,
                                    &format!(
                                        "Interface '{}' incorrectly extends interface '{}'.",
                                        derived_name, base_name
                                    ),
                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                );

                                if let Some((pos, end)) = self.get_node_span(iface_data.name) {
                                    self.error(
                                        pos,
                                        end - pos,
                                        format!(
                                            "Types of property '{}' are incompatible.",
                                            member_name
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                    self.error(
                                        pos,
                                        end - pos,
                                        format!(
                                            "Type '{}' is not assignable to type '{}'.",
                                            member_type_str, base_type_str
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                }

                                self.pop_type_parameters(base_type_param_updates);
                                return;
                            }

                            break;
                        }

                        if found {
                            break;
                        }
                    }
                }

                self.pop_type_parameters(base_type_param_updates);
            }
        }
    }

    /// Check that non-abstract class implements all abstract members from base class (error 2654).
    /// Reports "Non-abstract class 'X' is missing implementations for the following members of 'Y': {members}."
    pub(crate) fn check_abstract_member_implementations(
        &mut self,
        class_idx: NodeIndex,
        class_data: &crate::parser::node::ClassData,
    ) {
        // Only check non-abstract classes
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        // Find base class from heritage clauses
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the base class
            if let Some(&type_idx) = heritage.types.nodes.first()
                && let Some(type_node) = self.ctx.arena.get(type_idx)
            {
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                {
                    base_class_name = ident.escaped_text.clone();

                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&base_class_name)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        if !symbol.value_declaration.is_none() {
                            base_class_idx = Some(symbol.value_declaration);
                        } else if let Some(&decl_idx) = symbol.declarations.first() {
                            base_class_idx = Some(decl_idx);
                        }
                    }
                }
            }
            break;
        }

        let Some(base_idx) = base_class_idx else {
            return;
        };

        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return;
        };

        // Collect implemented members from derived class
        let mut implemented_members = rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                // Check if this member is not abstract (i.e., it's an implementation)
                if !self.member_is_abstract(member_idx) {
                    implemented_members.insert(name);
                }
            }
        }

        // Collect abstract members from base class that are not implemented
        let mut missing_members: Vec<String> = Vec::new();
        for &member_idx in &base_class.members.nodes {
            if self.member_is_abstract(member_idx)
                && let Some(name) = self.get_member_name(member_idx)
                && !implemented_members.contains(&name)
            {
                missing_members.push(name);
            }
        }

        // Report error if there are missing implementations
        if !missing_members.is_empty() {
            let derived_class_name = if !class_data.name.is_none() {
                if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        ident.escaped_text.clone()
                    } else {
                        String::from("<anonymous>")
                    }
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            };

            // TypeScript uses different error codes based on the number of missing members:
            // - TS2515: Single missing member: "Non-abstract class 'C' does not implement inherited abstract member 'bar' from class 'B'."
            // - TS2654: Multiple missing members: "Non-abstract class 'C' is missing implementations for the following members of 'B': 'foo', 'bar'."
            if missing_members.len() == 1 {
                // TS2515: Single missing member
                self.error_at_node(
                    class_idx,
                    &format!(
                        "Non-abstract class '{}' does not implement inherited abstract member '{}' from class '{}'.",
                        derived_class_name, missing_members[0], base_class_name
                    ),
                    diagnostic_codes::ABSTRACT_MEMBER_IN_NON_ABSTRACT_CLASS, // TS2515
                );
            } else {
                // TS2654: Multiple missing members
                let missing_list = missing_members
                    .iter()
                    .map(|s| format!("'{}'", s))
                    .collect::<Vec<_>>()
                    .join(", ");

                self.error_at_node(
                    class_idx,
                    &format!(
                        "Non-abstract class '{}' is missing implementations for the following members of '{}': {}.",
                        derived_class_name, base_class_name, missing_list
                    ),
                    diagnostic_codes::NON_ABSTRACT_CLASS_MISSING_IMPLEMENTATIONS,
                );
            }
        }
    }

    /// Check if a class member has the abstract modifier.
    pub(crate) fn member_is_abstract(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.has_abstract_modifier(&prop.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_abstract_modifier(&method.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.has_abstract_modifier(&accessor.modifiers)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check that a class properly implements all interfaces from its implements clauses.
    /// Emits TS2420 when a class incorrectly implements an interface.
    /// Checks for:
    /// - Missing members (properties and methods)
    /// - Incompatible member types (property type or method signature mismatch)
    pub(crate) fn check_implements_clauses(
        &mut self,
        _class_idx: NodeIndex,
        class_data: &crate::parser::node::ClassData,
    ) {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Collect implemented members from the class (name -> node_idx).
        // Member types are computed lazily only when needed for an interface match.
        let mut class_members: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                class_members.insert(name, member_idx);
            }
        }
        let mut class_member_types: rustc_hash::FxHashMap<NodeIndex, TypeId> =
            rustc_hash::FxHashMap::default();

        // Get the class name for error messages
        let class_name = if !class_data.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.clone()
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check implements clauses
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            };

            // Check each interface in the implements clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression (identifier or property access) from ExpressionWithTypeArguments
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Get the interface symbol
                if let Some(interface_name) = self.heritage_name_text(expr_idx)
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(&interface_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    let interface_idx = if !symbol.value_declaration.is_none() {
                        symbol.value_declaration
                    } else if let Some(&decl_idx) = symbol.declarations.first() {
                        decl_idx
                    } else {
                        continue;
                    };

                    let Some(interface_node) = self.ctx.arena.get(interface_idx) else {
                        continue;
                    };

                    // Check if it's actually an interface declaration
                    if interface_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                        continue;
                    }

                    let Some(interface_decl) = self.ctx.arena.get_interface(interface_node) else {
                        continue;
                    };

                    // Check that all interface members are implemented with compatible types
                    let mut missing_members: Vec<String> = Vec::new();
                    let mut incompatible_members: Vec<(String, String, String)> = Vec::new(); // (name, expected_type, actual_type)

                    for &member_idx in &interface_decl.members.nodes {
                        let Some(member_node) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };

                        // Skip non-property/method signatures
                        if member_node.kind != METHOD_SIGNATURE
                            && member_node.kind != PROPERTY_SIGNATURE
                        {
                            continue;
                        }

                        let Some(member_name) = self.get_member_name(member_idx) else {
                            continue;
                        };

                        // Check if class has this member
                        if let Some(&class_member_idx) = class_members.get(&member_name) {
                            let class_member_type =
                                if let Some(&cached) = class_member_types.get(&class_member_idx) {
                                    cached
                                } else {
                                    let computed = self.get_type_of_class_member(class_member_idx);
                                    class_member_types.insert(class_member_idx, computed);
                                    computed
                                };
                            // Get the expected type from the interface
                            let interface_member_type =
                                self.get_type_of_interface_member_simple(member_idx);

                            // Check type compatibility (class member type must be assignable to interface member type)
                            if interface_member_type != TypeId::ANY
                                && class_member_type != TypeId::ANY
                                && !self.is_assignable_to(class_member_type, interface_member_type)
                            {
                                let expected_str = self.format_type(interface_member_type);
                                let actual_str = self.format_type(class_member_type);
                                incompatible_members.push((
                                    member_name.clone(),
                                    expected_str,
                                    actual_str,
                                ));
                            }
                        } else {
                            missing_members.push(member_name);
                        }
                    }

                    // Report error for missing members
                    if !missing_members.is_empty() {
                        let missing_list = missing_members
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ");

                        self.error_at_node(
                            clause_idx,
                            &format!(
                                "Class '{}' incorrectly implements interface '{}'. Missing members: {}.",
                                class_name, interface_name, missing_list
                            ),
                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                        );
                    }

                    // Report error for incompatible member types
                    for (member_name, expected, actual) in incompatible_members {
                        self.error_at_node(
                            clause_idx,
                            &format!(
                                "Class '{}' incorrectly implements interface '{}'. Property '{}' has type '{}' which is not assignable to type '{}'.",
                                class_name, interface_name, member_name, actual, expected
                            ),
                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                        );
                    }
                }
            }
        }
    }

    /// Count required (non-optional, non-rest, no-initializer) parameters in a
    /// method/function signature node, excluding `this` parameters.
    fn count_required_params_from_signature_node(&self, node_idx: NodeIndex) -> usize {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return 0;
        };
        let Some(sig) = self.ctx.arena.get_signature(node) else {
            return 0;
        };
        let Some(ref params) = sig.parameters else {
            return 0;
        };
        let mut count = 0;
        for &param_idx in &params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Skip `this` pseudo-parameter
            if let Some(name_node) = self.ctx.arena.get(param.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    continue;
                }
            }
            // Rest parameters are not counted as required
            if param.dot_dot_dot_token {
                continue;
            }
            // Optional or has-default parameters are not required
            if param.question_token || !param.initializer.is_none() {
                continue;
            }
            count += 1;
        }
        count
    }
}
