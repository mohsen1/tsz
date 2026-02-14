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

use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};
use crate::state::CheckerState;
use crate::types::diagnostics::diagnostic_codes;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Extracted info about a single class member (property, method, or accessor).
struct ClassMemberInfo {
    name: String,
    type_id: TypeId,
    name_idx: NodeIndex,
    visibility: MemberVisibility,
    is_method: bool,
    is_static: bool,
    is_accessor: bool,
    is_abstract: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MemberVisibility {
    Public,
    Protected,
    Private,
}

// =============================================================================
// Class and Interface Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Extract name, type, and flags from a class member node.
    ///
    /// If `skip_private` is true, returns `None` for private members.
    fn extract_class_member_info(
        &mut self,
        member_idx: NodeIndex,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        let member_node = self.ctx.arena.get(member_idx)?;
        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node)?;
                let name = self.get_property_name(prop.name)?;
                if skip_private && self.has_private_modifier(&prop.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&prop.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&prop.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&prop.modifiers);
                let prop_type = if !prop.type_annotation.is_none() {
                    self.get_type_from_type_node(prop.type_annotation)
                } else if !prop.initializer.is_none() {
                    self.get_type_of_node(prop.initializer)
                } else {
                    TypeId::ANY
                };
                let is_abstract = self.has_abstract_modifier(&prop.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: prop_type,
                    name_idx: prop.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor: false,
                    is_abstract,
                })
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node)?;
                let name = self.get_property_name(method.name)?;
                if skip_private && self.has_private_modifier(&method.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&method.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&method.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&method.modifiers);
                let factory = self.ctx.types.factory();
                use tsz_solver::FunctionShape;
                let signature = self.call_signature_from_method(method);
                let method_type = factory.function(FunctionShape {
                    type_params: signature.type_params,
                    params: signature.params,
                    this_type: signature.this_type,
                    return_type: signature.return_type,
                    type_predicate: signature.type_predicate,
                    is_constructor: false,
                    is_method: true,
                });
                let is_abstract = self.has_abstract_modifier(&method.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: method_type,
                    name_idx: method.name,
                    visibility,
                    is_method: true,
                    is_static,
                    is_accessor: false,
                    is_abstract,
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name = self.get_property_name(accessor.name)?;
                if skip_private && self.has_private_modifier(&accessor.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&accessor.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&accessor.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&accessor.modifiers);
                let accessor_type = if !accessor.type_annotation.is_none() {
                    self.get_type_from_type_node(accessor.type_annotation)
                } else {
                    self.infer_getter_return_type(accessor.body)
                };
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: accessor_type,
                    name_idx: accessor.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor: true,
                    is_abstract,
                })
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node)?;
                let name = self.get_property_name(accessor.name)?;
                if skip_private && self.has_private_modifier(&accessor.modifiers) {
                    return None;
                }
                let visibility = if self.has_private_modifier(&accessor.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&accessor.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };
                let is_static = self.has_static_modifier(&accessor.modifiers);
                let accessor_type = accessor
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p| self.ctx.arena.get_parameter_at(p))
                    .map(|param| {
                        if !param.type_annotation.is_none() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else {
                            TypeId::ANY
                        }
                    })
                    .unwrap_or(TypeId::ANY);
                let is_abstract = self.has_abstract_modifier(&accessor.modifiers);
                Some(ClassMemberInfo {
                    name,
                    type_id: accessor_type,
                    name_idx: accessor.name,
                    visibility,
                    is_method: false,
                    is_static,
                    is_accessor: true,
                    is_abstract,
                })
            }
            _ => None,
        }
    }

    // =========================================================================
    // Inheritance Checking
    // =========================================================================

    /// Check that property types in derived class are compatible with base class (error 2416).
    /// For each property/accessor in the derived class, checks if there's a corresponding
    /// member in the base class with incompatible type.
    pub(crate) fn check_property_inheritance_compatibility(
        &mut self,
        _class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use tsz_solver::{TypeSubstitution, instantiate_type};

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

        // Track names that already had TS2610/TS2611 emitted (avoid duplicate for get+set pairs)
        let mut accessor_mismatch_reported: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut class_extends_error_reported = false;

        // Check each member in the derived class
        for &member_idx in &class_data.members.nodes {
            let Some(info) = self.extract_class_member_info(member_idx, false) else {
                continue;
            };
            let (
                member_name,
                member_type,
                member_name_idx,
                member_visibility,
                is_method,
                is_static,
                is_accessor,
            ) = (
                info.name,
                info.type_id,
                info.name_idx,
                info.visibility,
                info.is_method,
                info.is_static,
                info.is_accessor,
            );

            // Skip override checking for private identifiers (#foo)
            // Private fields are scoped to the class that declares them and
            // do NOT participate in the inheritance hierarchy
            if member_name.starts_with('#') {
                continue;
            }

            // Find matching member including private/protected members to detect
            // class-level visibility/branding incompatibilities (TS2415).
            let base_any_info = {
                let mut found = None;
                for &base_member_idx in &base_class.members.nodes {
                    if let Some(info) = self.extract_class_member_info(base_member_idx, false)
                        && info.name == member_name
                        && info.is_static == is_static
                    {
                        found = Some(info);
                        break;
                    }
                }
                if found.is_none() {
                    found = self.find_member_in_class_chain(
                        base_idx,
                        &member_name,
                        is_static,
                        0,
                        false,
                    );
                }
                found
            };

            if let Some(base_any_info) = base_any_info
                && self
                    .class_member_visibility_conflicts(member_visibility, base_any_info.visibility)
            {
                if !class_extends_error_reported {
                    if is_static {
                        self.error_at_node(
                            class_data.name,
                            &format!(
                                "Class static side '{}' incorrectly extends base class static side '{}'.",
                                derived_class_name, base_class_name
                            ),
                            diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                        );
                    } else {
                        self.error_at_node(
                            class_data.name,
                            &format!(
                                "Class '{}' incorrectly extends base class '{}'.",
                                derived_class_name, base_class_name
                            ),
                            diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                        );
                    }
                    class_extends_error_reported = true;
                }
                continue;
            }

            // Look for a matching member in the base class hierarchy (skip private members)
            // First check direct members of the base class, then walk up the chain
            let base_info = {
                let mut found = None;
                for &base_member_idx in &base_class.members.nodes {
                    if let Some(info) = self.extract_class_member_info(base_member_idx, true) {
                        if info.name == member_name && info.is_static == is_static {
                            found = Some(info);
                            break;
                        }
                    }
                }
                // If not found in direct base, walk up the ancestor chain
                if found.is_none() {
                    found =
                        self.find_member_in_class_chain(base_idx, &member_name, is_static, 0, true);
                }
                found
            };

            let Some(base_info) = base_info else {
                continue;
            };

            let base_type = instantiate_type(self.ctx.types, base_info.type_id, &substitution);

            // TS2610/TS2611: Check accessor/property kind mismatch
            // Only applies to non-method members. Fires regardless of types (even ANY).
            if !is_method
                && !base_info.is_method
                && !base_info.is_abstract
                && !accessor_mismatch_reported.contains(&member_name)
            {
                if !is_accessor && base_info.is_accessor {
                    // TS2610: derived property overrides base accessor
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{}' is defined as an accessor in class '{}', but is overridden here in '{}' as an instance property.",
                            member_name, base_class_name, derived_class_name
                        ),
                        diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                    );
                    continue;
                }
                if is_accessor && !base_info.is_accessor {
                    // TS2611: derived accessor overrides base property
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{}' is defined as a property in class '{}', but is overridden here in '{}' as an accessor.",
                            member_name, base_class_name, derived_class_name
                        ),
                        diagnostic_codes::IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR,
                    );
                    continue;
                }
            }

            // TS2425/TS2426: Check for method/property/accessor kind mismatch (INSTANCE members only)
            // Static members use TS2417 instead
            if !is_static {
                // TS2425: Base has property (not method, not accessor), derived has method
                if is_method && !base_info.is_method && !base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{}' defines instance member property '{}', but extended class '{}' defines it as instance member function.",
                            base_class_name, member_name, derived_class_name
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_PROPERTY_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2426: Base has accessor, derived has method
                if is_method && base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{}' defines instance member accessor '{}', but extended class '{}' defines it as instance member function.",
                            base_class_name, member_name, derived_class_name
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_ACCESSOR_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }
            }

            // Skip type compatibility check if either type is ANY
            if member_type == TypeId::ANY || base_type == TypeId::ANY {
                continue;
            }

            // Resolve TypeQuery types (typeof) before comparison
            let resolved_member_type = self.resolve_type_query_type(member_type);
            let resolved_base_type = self.resolve_type_query_type(base_type);

            // Check type compatibility through centralized mismatch policy.
            // Methods use bivariant relation checks; properties use regular assignability.
            let should_report_mismatch = if is_method {
                should_report_member_type_mismatch_bivariant(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            } else {
                should_report_member_type_mismatch(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            };

            if should_report_mismatch {
                let member_type_str = self.format_type(member_type);
                let base_type_str = self.format_type(base_type);

                // TS2417: Static members use different error message and code
                // TS2416: Instance members use standard property incompatibility error
                if is_static {
                    // TS2417: Class static side '{0}' incorrectly extends base class static side '{1}'.
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class static side '{}' incorrectly extends base class static side '{}'.",
                            derived_class_name, base_class_name
                        ),
                        diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                    );
                } else {
                    // TS2416: Instance member incompatibility
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{}' in type '{}' is not assignable to the same property in base type '{}'.",
                            member_name, derived_class_name, base_class_name
                        ),
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                    self.report_type_not_assignable_detail(
                        member_name_idx,
                        &member_type_str,
                        &base_type_str,
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                }
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
        iface_data: &tsz_parser::parser::node::InterfaceData,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{TypeSubstitution, instantiate_type};

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

        let mut derived_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        for (member_name, _, _, _) in &derived_members {
            derived_member_names.insert(member_name.clone());
        }
        for &member_idx in &iface_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && member_node.kind == CALL_SIGNATURE
            {
                derived_member_names.insert(String::from("__call__"));
            }
        }

        let mut inherited_member_sources: rustc_hash::FxHashMap<String, (String, TypeId)> =
            rustc_hash::FxHashMap::default();
        let mut inherited_non_public_class_member_sources: rustc_hash::FxHashMap<String, String> =
            rustc_hash::FxHashMap::default();

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

                for &base_iface_idx in &base_iface_indices {
                    let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                        continue;
                    };
                    let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                        continue;
                    };

                    let (base_type_params, base_type_param_updates) =
                        self.push_type_parameters(&base_iface.type_parameters);

                    let mut base_type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            base_type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    if base_type_args.len() < base_type_params.len() {
                        for param in base_type_params.iter().skip(base_type_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            base_type_args.push(fallback);
                        }
                    }
                    if base_type_args.len() > base_type_params.len() {
                        base_type_args.truncate(base_type_params.len());
                    }

                    let base_substitution = TypeSubstitution::from_args(
                        self.ctx.types,
                        &base_type_params,
                        &base_type_args,
                    );

                    for &base_member_idx in &base_iface.members.nodes {
                        let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                            continue;
                        };

                        let (member_key, member_type) = if base_member_node.kind == CALL_SIGNATURE {
                            (
                                String::from("__call__"),
                                instantiate_type(
                                    self.ctx.types,
                                    self.get_type_of_node(base_member_idx),
                                    &base_substitution,
                                ),
                            )
                        } else if base_member_node.kind == METHOD_SIGNATURE
                            || base_member_node.kind == PROPERTY_SIGNATURE
                        {
                            let Some(sig) = self.ctx.arena.get_signature(base_member_node) else {
                                continue;
                            };
                            let Some(name) = self.get_property_name(sig.name) else {
                                continue;
                            };
                            (
                                name,
                                instantiate_type(
                                    self.ctx.types,
                                    self.get_type_of_interface_member_simple(base_member_idx),
                                    &base_substitution,
                                ),
                            )
                        } else {
                            continue;
                        };

                        if derived_member_names.contains(&member_key) {
                            continue;
                        }

                        if let Some((prev_base_name, prev_member_type)) =
                            inherited_member_sources.get(&member_key)
                        {
                            if prev_base_name != &base_name {
                                let incompatible =
                                    !self.are_mutually_assignable(member_type, *prev_member_type);
                                if incompatible {
                                    self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{}' cannot simultaneously extend types '{}' and '{}'.",
                                            derived_name, prev_base_name, base_name
                                        ),
                                        diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                    );
                                    return;
                                }
                            }
                        } else {
                            inherited_member_sources
                                .insert(member_key, (base_name.clone(), member_type));
                        }
                    }

                    self.pop_type_parameters(base_type_param_updates);
                }

                // If the base is not an interface, check if it's a class with private/protected members (TS2430)
                if base_iface_indices.is_empty() {
                    // Check if the base is a class
                    let mut base_class_idx = None;
                    for &decl_idx in &base_symbol.declarations {
                        if let Some(node) = self.ctx.arena.get(decl_idx)
                            && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        {
                            base_class_idx = Some(decl_idx);
                            break;
                        }
                    }

                    if base_class_idx.is_none() && !base_symbol.value_declaration.is_none() {
                        let decl_idx = base_symbol.value_declaration;
                        if let Some(node) = self.ctx.arena.get(decl_idx)
                            && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        {
                            base_class_idx = Some(decl_idx);
                        }
                    }

                    if let Some(class_idx) = base_class_idx {
                        if let Some(class_node) = self.ctx.arena.get(class_idx)
                            && let Some(class_data) = self.ctx.arena.get_class(class_node)
                        {
                            // Check if any interface member redeclares a private/protected class member
                            for (member_name, _, derived_member_idx, _) in &derived_members {
                                for &class_member_idx in &class_data.members.nodes {
                                    let Some(class_member_node) =
                                        self.ctx.arena.get(class_member_idx)
                                    else {
                                        continue;
                                    };

                                    let (class_member_name, is_private_or_protected) =
                                        match class_member_node.kind {
                                            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                                if let Some(prop) = self
                                                    .ctx
                                                    .arena
                                                    .get_property_decl(class_member_node)
                                                {
                                                    let name = self.get_property_name(prop.name);
                                                    let is_priv_prot = self
                                                        .has_private_modifier(&prop.modifiers)
                                                        || self.has_protected_modifier(
                                                            &prop.modifiers,
                                                        );
                                                    (name, is_priv_prot)
                                                } else {
                                                    continue;
                                                }
                                            }
                                            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                                if let Some(method) = self
                                                    .ctx
                                                    .arena
                                                    .get_method_decl(class_member_node)
                                                {
                                                    let name = self.get_property_name(method.name);
                                                    let is_priv_prot = self
                                                        .has_private_modifier(&method.modifiers)
                                                        || self.has_protected_modifier(
                                                            &method.modifiers,
                                                        );
                                                    (name, is_priv_prot)
                                                } else {
                                                    continue;
                                                }
                                            }
                                            k if k == syntax_kind_ext::GET_ACCESSOR
                                                || k == syntax_kind_ext::SET_ACCESSOR =>
                                            {
                                                if let Some(accessor) =
                                                    self.ctx.arena.get_accessor(class_member_node)
                                                {
                                                    let name =
                                                        self.get_property_name(accessor.name);
                                                    let is_priv_prot = self
                                                        .has_private_modifier(&accessor.modifiers)
                                                        || self.has_protected_modifier(
                                                            &accessor.modifiers,
                                                        );
                                                    (name, is_priv_prot)
                                                } else {
                                                    continue;
                                                }
                                            }
                                            _ => continue,
                                        };

                                    if let Some(class_member_name) = class_member_name {
                                        if &class_member_name == member_name
                                            && is_private_or_protected
                                        {
                                            // Interface redeclares a private/protected member as public - TS2430
                                            self.error_at_node(
                                                *derived_member_idx,
                                                &format!(
                                                    "Interface '{}' incorrectly extends interface '{}'.",
                                                    derived_name, base_name
                                                ),
                                                diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                            );

                                            if let Some((pos, end)) =
                                                self.get_node_span(*derived_member_idx)
                                            {
                                                self.error(
                                                    pos,
                                                    end - pos,
                                                    format!(
                                                        "Property '{}' is private in type '{}' but not in type '{}'.",
                                                        member_name, base_name, derived_name
                                                    ),
                                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                                );
                                            }
                                        }
                                    }
                                }
                            }

                            // TS2320: Interface cannot extend two classes that each contribute a
                            // private/protected member with the same name.
                            for &class_member_idx in &class_data.members.nodes {
                                let Some(member_info) =
                                    self.extract_class_member_info(class_member_idx, false)
                                else {
                                    continue;
                                };

                                if member_info.is_static
                                    || member_info.visibility == MemberVisibility::Public
                                {
                                    continue;
                                }

                                if derived_member_names.contains(&member_info.name) {
                                    continue;
                                }

                                if let Some(prev_base_name) =
                                    inherited_non_public_class_member_sources.get(&member_info.name)
                                {
                                    if prev_base_name != &base_name {
                                        self.error_at_node(
                                            iface_data.name,
                                            &format!(
                                                "Interface '{}' cannot simultaneously extend types '{}' and '{}'.",
                                                derived_name, prev_base_name, base_name
                                            ),
                                            diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                        );
                                        return;
                                    }
                                } else {
                                    inherited_non_public_class_member_sources
                                        .insert(member_info.name, base_name.clone());
                                }
                            }
                        }
                    }

                    continue;
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
                                || should_report_member_type_mismatch(
                                    self,
                                    *member_type,
                                    base_type,
                                    *derived_member_idx,
                                )
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
                                self.report_property_type_incompatible_detail(
                                    iface_data.name,
                                    member_name,
                                    &member_type_str,
                                    &base_type_str,
                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                );

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

    fn report_type_not_assignable_detail(
        &mut self,
        node_idx: NodeIndex,
        source_type: &str,
        target_type: &str,
        code: u32,
    ) {
        if let Some((pos, end)) = self.get_node_span(node_idx) {
            self.error(
                pos,
                end - pos,
                format!(
                    "Type '{}' is not assignable to type '{}'.",
                    source_type, target_type
                ),
                code,
            );
        }
    }

    fn report_property_type_incompatible_detail(
        &mut self,
        node_idx: NodeIndex,
        member_name: &str,
        source_type: &str,
        target_type: &str,
        code: u32,
    ) {
        if let Some((pos, end)) = self.get_node_span(node_idx) {
            self.error(
                pos,
                end - pos,
                format!("Types of property '{}' are incompatible.", member_name),
                code,
            );
            self.error(
                pos,
                end - pos,
                format!(
                    "Type '{}' is not assignable to type '{}'.",
                    source_type, target_type
                ),
                code,
            );
        }
    }

    /// Check that non-abstract class implements all abstract members from base class (error 2654).
    /// Reports "Non-abstract class 'X' is missing implementations for the following members of 'Y': {members}."
    pub(crate) fn check_abstract_member_implementations(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
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
                    diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS, // TS2515
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
                    diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
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
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use tsz_parser::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Abstract classes don't need to implement interface members —
        // their abstract members satisfy the interface contract.
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

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

        let class_namespace = self.enclosing_namespace_node(class_idx);

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

                // Get the expression and type arguments from ExpressionWithTypeArguments
                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                // Resolve interface/class symbols through canonical heritage resolution so
                // qualified names (e.g. `Promise.Thenable`) are handled correctly.
                if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    let interface_name = self
                        .heritage_name_text(expr_idx)
                        .unwrap_or_else(|| symbol.escaped_name.clone());
                    let interface_idx = symbol
                        .declarations
                        .iter()
                        .copied()
                        .find(|&decl_idx| {
                            self.enclosing_namespace_node(decl_idx) == class_namespace
                        })
                        .or_else(|| {
                            if !symbol.value_declaration.is_none() {
                                Some(symbol.value_declaration)
                            } else {
                                symbol.declarations.first().copied()
                            }
                        });
                    let Some(interface_idx) = interface_idx else {
                        continue;
                    };

                    let Some(interface_node) = self.ctx.arena.get(interface_idx) else {
                        continue;
                    };

                    // TS2720: `implements` can reference a class symbol. When that class has
                    // private/protected members, structural implementation is invalid and tsc
                    // reports "Did you mean to extend ...".
                    if interface_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                        if let Some(base_class_data) = self.ctx.arena.get_class(interface_node)
                            && self.class_has_private_or_protected_members(base_class_data)
                        {
                            let message = format!(
                                "Class '{}' incorrectly implements class '{}'. Did you mean to extend '{}' and inherit its members as a subclass?",
                                class_name, interface_name, interface_name
                            );
                            self.error_at_node(
                                type_idx,
                                &message,
                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER,
                            );

                            // For implements-on-class with private/protected members, tsc
                            // reports TS2720 as the primary diagnostic.
                        }
                        continue;
                    }

                    // Check if it's actually an interface declaration
                    if interface_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                        continue;
                    }

                    let Some(interface_decl) = self.ctx.arena.get_interface(interface_node) else {
                        continue;
                    };

                    // Check if interface extends a class with private/protected members (TS2420)
                    // Only error if the implementing class doesn't extend the same base class
                    if self.interface_extends_class_with_inaccessible_members(
                        interface_idx,
                        interface_decl,
                        class_idx,
                        class_data,
                    ) {
                        self.error_at_node(
                            type_idx,
                            &format!(
                                "Class '{}' incorrectly implements interface '{}'.",
                                class_name, interface_name
                            ),
                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                        );
                        continue;
                    }

                    // Check that all interface members are implemented with compatible types
                    let mut missing_members: Vec<String> = Vec::new();
                    let mut incompatible_members: Vec<(String, String, String)> = Vec::new(); // (name, expected_type, actual_type)
                    let mut interface_has_index_signature = false;

                    // Build type arguments vector from implements clause (e.g., A<boolean> -> [boolean])
                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    // Push interface type parameters into scope so they're available when
                    // checking member types (fixes TS2304 false positive for interface type params)
                    let (interface_type_params, interface_type_param_updates) =
                        self.push_type_parameters(&interface_decl.type_parameters);

                    // Fill in missing type arguments with defaults/constraints/unknown
                    if type_args.len() < interface_type_params.len() {
                        for param in interface_type_params.iter().skip(type_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            type_args.push(fallback);
                        }
                    }
                    if type_args.len() > interface_type_params.len() {
                        type_args.truncate(interface_type_params.len());
                    }

                    // Create substitution to instantiate interface type parameters with actual type arguments
                    let substitution = tsz_solver::TypeSubstitution::from_args(
                        self.ctx.types,
                        &interface_type_params,
                        &type_args,
                    );

                    for &member_idx in &interface_decl.members.nodes {
                        let Some(member_node) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };

                        if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                            interface_has_index_signature = true;
                            continue;
                        }

                        // Skip non-property/method signatures
                        if member_node.kind != METHOD_SIGNATURE
                            && member_node.kind != PROPERTY_SIGNATURE
                        {
                            continue;
                        }

                        let Some(member_name) = self.get_member_name(member_idx) else {
                            continue;
                        };

                        // Skip optional interface members — they don't need to be present
                        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                            if sig.question_token {
                                continue;
                            }
                        }

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
                            // Get the expected type from the interface and instantiate with type arguments
                            let interface_member_type =
                                self.get_type_of_interface_member_simple(member_idx);
                            let interface_member_type = tsz_solver::instantiate_type(
                                self.ctx.types,
                                interface_member_type,
                                &substitution,
                            );

                            // Check type compatibility (class member type must be assignable to interface member type)
                            // Skip if either type is any or error (unresolved types shouldn't cause false positives)
                            if interface_member_type != TypeId::ANY
                                && class_member_type != TypeId::ANY
                                && interface_member_type != TypeId::ERROR
                                && class_member_type != TypeId::ERROR
                                && should_report_member_type_mismatch(
                                    self,
                                    class_member_type,
                                    interface_member_type,
                                    class_member_idx,
                                )
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

                    // Check if interface has index signature but class doesn't
                    if interface_has_index_signature {
                        let class_has_index_signature =
                            class_data.members.nodes.iter().any(|&member_idx| {
                                if let Some(member_node) = self.ctx.arena.get(member_idx) {
                                    member_node.kind == syntax_kind_ext::INDEX_SIGNATURE
                                } else {
                                    false
                                }
                            });

                        if !class_has_index_signature {
                            self.error_at_node(
                                clause_idx,
                                &format!(
                                    "Class '{}' incorrectly implements interface '{}'. Index signature for type 'number' is missing in type '{}'.",
                                    class_name, interface_name, class_name
                                ),
                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                            );
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

                    // Pop interface type parameters from scope
                    self.pop_type_parameters(interface_type_param_updates);
                }
            }
        }
    }

    fn enclosing_namespace_node(&self, decl_idx: NodeIndex) -> NodeIndex {
        let mut current = decl_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return NodeIndex::NONE;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return NodeIndex::NONE;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return NodeIndex::NONE;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return parent;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return NodeIndex::NONE;
            }
            current = parent;
        }
    }

    /// Check if an interface extends a class with private/protected members that are
    /// inaccessible to the implementing class.
    ///
    /// When an interface extends a class with private/protected members, those members
    /// become part of the interface's contract. A class implementing such an interface
    /// can only satisfy this contract if it extends the same base class (giving it
    /// access to those private members). Otherwise, TS2420 should be emitted.
    ///
    /// # Arguments
    /// * `interface_idx` - The NodeIndex of the interface declaration
    /// * `interface_decl` - The interface data
    /// * `class_idx` - The NodeIndex of the implementing class
    /// * `class_data` - The class data
    ///
    /// # Returns
    /// true if the interface extends a class with private/protected members that the
    /// implementing class cannot access
    fn interface_extends_class_with_inaccessible_members(
        &mut self,
        _interface_idx: NodeIndex,
        interface_decl: &tsz_parser::parser::node::InterfaceData,
        _class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // First, collect the base classes that the implementing class extends
        let mut class_extends_symbols = std::collections::HashSet::new();
        if let Some(ref class_heritage) = class_data.heritage_clauses {
            for &clause_idx in &class_heritage.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };

                // Only look at extends clauses
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                    if let Some(base_name) = self.heritage_name_text(expr_idx)
                        && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    {
                        class_extends_symbols.insert(sym_id);
                    }
                }
            }
        }

        let Some(ref heritage_clauses) = interface_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (not implements)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression from ExpressionWithTypeArguments or TypeReference
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Resolve the symbol being extended
                if let Some(base_name) = self.heritage_name_text(expr_idx)
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // If the implementing class extends this same base class, then it has
                    // access to the private members - no error needed
                    if class_extends_symbols.contains(&sym_id) {
                        continue;
                    }

                    // Check if any declaration is a class with private/protected members
                    for &decl_idx in &symbol.declarations {
                        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                            continue;
                        };

                        // Check if it's a class declaration
                        if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                            continue;
                        }

                        let Some(class_data) = self.ctx.arena.get_class(decl_node) else {
                            continue;
                        };

                        // Check if class has any private or protected members
                        for &member_idx in &class_data.members.nodes {
                            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                                continue;
                            };

                            match member_node.kind {
                                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                    if let Some(prop) =
                                        self.ctx.arena.get_property_decl(member_node)
                                    {
                                        if self.has_private_modifier(&prop.modifiers)
                                            || self.has_protected_modifier(&prop.modifiers)
                                        {
                                            return true;
                                        }
                                    }
                                }
                                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                    if let Some(method) =
                                        self.ctx.arena.get_method_decl(member_node)
                                    {
                                        if self.has_private_modifier(&method.modifiers)
                                            || self.has_protected_modifier(&method.modifiers)
                                        {
                                            return true;
                                        }
                                    }
                                }
                                k if k == syntax_kind_ext::GET_ACCESSOR => {
                                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                                    {
                                        if self.has_private_modifier(&accessor.modifiers)
                                            || self.has_protected_modifier(&accessor.modifiers)
                                        {
                                            return true;
                                        }
                                    }
                                }
                                k if k == syntax_kind_ext::SET_ACCESSOR => {
                                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                                    {
                                        if self.has_private_modifier(&accessor.modifiers)
                                            || self.has_protected_modifier(&accessor.modifiers)
                                        {
                                            return true;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Also check value_declaration
                    if !symbol.value_declaration.is_none() {
                        let decl_idx = symbol.value_declaration;
                        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                            continue;
                        };

                        if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                            let Some(class_data) = self.ctx.arena.get_class(decl_node) else {
                                continue;
                            };

                            for &member_idx in &class_data.members.nodes {
                                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                                    continue;
                                };

                                match member_node.kind {
                                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                        if let Some(prop) =
                                            self.ctx.arena.get_property_decl(member_node)
                                        {
                                            if self.has_private_modifier(&prop.modifiers)
                                                || self.has_protected_modifier(&prop.modifiers)
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                        if let Some(method) =
                                            self.ctx.arena.get_method_decl(member_node)
                                        {
                                            if self.has_private_modifier(&method.modifiers)
                                                || self.has_protected_modifier(&method.modifiers)
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                        {
                                            if self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers)
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                        {
                                            if self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers)
                                            {
                                                return true;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn class_has_private_or_protected_members(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                        && (self.has_private_modifier(&prop.modifiers)
                            || self.has_protected_modifier(&prop.modifiers))
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(member_node)
                        && (self.has_private_modifier(&method.modifiers)
                            || self.has_protected_modifier(&method.modifiers))
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                        && (self.has_private_modifier(&accessor.modifiers)
                            || self.has_protected_modifier(&accessor.modifiers))
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Find a member by name in a class, searching up the inheritance chain.
    /// Returns the member info if found, or None.
    /// Uses cycle detection to handle circular inheritance safely.
    fn find_member_in_class_chain(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        _depth: usize,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        use tsz_solver::recursion::{RecursionGuard, RecursionProfile};

        // Create a recursion guard for cycle detection
        let mut guard = RecursionGuard::with_profile(RecursionProfile::CheckerRecursion);

        self.find_member_in_class_chain_impl(
            class_idx,
            target_name,
            target_is_static,
            skip_private,
            &mut guard,
        )
    }

    /// Internal implementation of find_member_in_class_chain with recursion guard.
    fn find_member_in_class_chain_impl(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<ClassMemberInfo> {
        use tsz_solver::recursion::RecursionResult;

        // Check for cycles using the recursion guard
        match guard.enter(class_idx) {
            RecursionResult::Cycle => {
                // Circular inheritance detected - return None gracefully
                return None;
            }
            RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                // Exceeded limits - bail out
                return None;
            }
            RecursionResult::Entered => {
                // Proceed with the search
            }
        }

        let class_node = self.ctx.arena.get(class_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;

        // Search direct members
        for &member_idx in &class_data.members.nodes {
            if let Some(info) = self.extract_class_member_info(member_idx, skip_private) {
                if info.name == target_name && info.is_static == target_is_static {
                    // Found it! Leave guard before returning
                    guard.leave(class_idx);
                    return Some(info);
                }
            }
        }

        // Walk up to base class
        let heritage_clauses = match class_data.heritage_clauses.as_ref() {
            Some(clauses) => clauses,
            None => {
                guard.leave(class_idx);
                return None;
            }
        };

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let type_idx = *heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            let expr_node = self.ctx.arena.get(expr_idx)?;
            let ident = self.ctx.arena.get_identifier(expr_node)?;
            let base_name = &ident.escaped_text;
            let sym_id = self.ctx.binder.file_locals.get(base_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            let base_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else {
                *symbol.declarations.first()?
            };

            let result = self.find_member_in_class_chain_impl(
                base_idx,
                target_name,
                target_is_static,
                skip_private,
                guard,
            );

            // Always leave the guard before returning
            guard.leave(class_idx);
            return result;
        }

        guard.leave(class_idx);
        None
    }

    fn class_member_visibility_conflicts(
        &self,
        derived_visibility: MemberVisibility,
        base_visibility: MemberVisibility,
    ) -> bool {
        matches!(
            (derived_visibility, base_visibility),
            (MemberVisibility::Private, MemberVisibility::Private)
                | (MemberVisibility::Private, MemberVisibility::Protected)
                | (MemberVisibility::Private, MemberVisibility::Public)
                | (MemberVisibility::Protected, MemberVisibility::Public)
                | (MemberVisibility::Public, MemberVisibility::Private)
                | (MemberVisibility::Protected, MemberVisibility::Private)
        )
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
