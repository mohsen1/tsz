//! Class/interface declaration checking (inheritance, implements, abstract members).

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Extracted info about a single class member (property, method, or accessor).
pub(crate) struct ClassMemberInfo {
    pub(crate) name: String,
    pub(crate) type_id: TypeId,
    pub(crate) name_idx: NodeIndex,
    pub(crate) visibility: MemberVisibility,
    pub(crate) is_method: bool,
    pub(crate) is_static: bool,
    pub(crate) is_accessor: bool,
    pub(crate) is_abstract: bool,
    pub(crate) has_override: bool,
    pub(crate) has_dynamic_name: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemberVisibility {
    Public,
    Protected,
    Private,
}

// =============================================================================
// Class and Interface Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Determine if a class member name is dynamic (e.g. `[foo]` or computed expressions),
    /// which cannot be used with an `override` modifier.
    fn is_computed_name_dynamic(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return true;
        };
        if self.ctx.arena.get(computed.expression).is_none() {
            return true;
        }

        if self
            .get_symbol_property_name_from_expr(computed.expression)
            .is_some()
        {
            return false;
        }

        self.is_computed_expression_dynamic(computed.expression)
    }

    fn is_computed_expression_dynamic(&self, expression_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expression_idx) else {
            return true;
        };
        let kind = expr_node.kind;

        if kind == SyntaxKind::Identifier as u16 {
            return false;
        }

        if matches!(
            kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(expr_node)
        {
            return self.is_computed_expression_dynamic(paren.expression);
        }

        true
    }

    /// Collect base member names for override suggestions.
    fn collect_base_member_names_for_override(
        &mut self,
        class_idx: NodeIndex,
        target_is_static: bool,
        members: &mut rustc_hash::FxHashSet<String>,
        visited: &mut rustc_hash::FxHashSet<NodeIndex>,
    ) {
        if !visited.insert(class_idx) {
            return;
        }

        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return;
        };

        for &member_idx in &class_data.members.nodes {
            if let Some(info) = self.extract_class_member_info(member_idx, true)
                && info.is_static == target_is_static
            {
                members.insert(info.name);
            }
        }

        if let Some(base_idx) = self.get_base_class_idx(class_idx) {
            self.collect_base_member_names_for_override(
                base_idx,
                target_is_static,
                members,
                visited,
            );
        }
    }

    /// Find a close member name from base class members for "Did you mean ...?".
    fn find_override_name_suggestion(
        &self,
        base_names: &rustc_hash::FxHashSet<String>,
        target_name: &str,
    ) -> Option<String> {
        let name_len = target_name.len();
        if base_names.is_empty() {
            return None;
        }

        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = name_len * 4 / 10 + 1;
        let mut best_candidate: Option<String> = None;

        for candidate in base_names {
            if candidate == target_name {
                continue;
            }
            if name_len.abs_diff(candidate.len()) > maximum_length_difference {
                continue;
            }
            if candidate.len() < 3 && candidate.to_lowercase() != target_name.to_lowercase() {
                continue;
            }

            if let Some(distance) =
                Self::override_name_levenshtein_with_max(target_name, candidate, best_distance)
                && distance < best_distance
            {
                best_distance = distance;
                best_candidate = Some(candidate.clone());
            }
        }

        best_candidate
    }

    /// Compute edit distance with an upper bound, used for override suggestions.
    fn override_name_levenshtein_with_max(
        s1: &str,
        s2: &str,
        max_distance: usize,
    ) -> Option<usize> {
        if s1.len() > s2.len() {
            return Self::override_name_levenshtein_with_max(s2, s1, max_distance);
        }

        let (short, long) = (s1.as_bytes(), s2.as_bytes());
        let (short_len, long_len) = (short.len(), long.len());
        if long_len - short_len > max_distance {
            return None;
        }

        let mut previous: Vec<usize> = (0..=long_len).collect();
        let mut current: Vec<usize> = vec![0; long_len + 1];

        for (i, &lhs) in short.iter().enumerate() {
            current[0] = i + 1;
            let mut row_min = current[0];
            for (j, &rhs) in long.iter().enumerate() {
                let insert = previous[j + 1] + 1;
                let delete = current[j] + 1;
                let replace = previous[j] + usize::from(lhs != rhs);
                let value = insert.min(delete).min(replace);
                current[j + 1] = value;
                if value < row_min {
                    row_min = value;
                }
            }
            if row_min > max_distance {
                return None;
            }
            previous.copy_from_slice(&current);
        }

        let distance = previous[long_len];
        if distance <= max_distance {
            Some(distance)
        } else {
            None
        }
    }

    /// Report explicit/implicit override errors for constructor parameter properties.
    fn check_constructor_parameter_property_overrides(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_class_idx: Option<NodeIndex>,
        base_class_name: &str,
        base_instance_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
    ) {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }

            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let has_override = self.has_override_modifier(&param.modifiers);
                let base_member = base_class_idx.and_then(|base_idx| {
                    self.find_member_in_class_chain(base_idx, &param_name, false, 0, true)
                });

                if has_override {
                    if base_class_idx.is_none() {
                        self.error_at_node(
                            param.name,
                            &crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                                &[base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                        );
                        continue;
                    }

                    if base_member.is_none() {
                        if let Some(suggestion) = self
                            .find_override_name_suggestion(base_instance_member_names, &param_name)
                        {
                            self.error_at_node(
                                param.name,
                                &crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                    &[base_class_name, &suggestion],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            );
                        } else {
                            self.error_at_node(
                                param.name,
                                &crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                    &[base_class_name],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            );
                        }
                    }
                } else if no_implicit_override && base_member.is_some() {
                    self.error_at_node(
                        param.name,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                            &[base_class_name],
                        ),
                        diagnostic_codes::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                    );
                }
            }
        }
    }

    /// Extract name, type, and flags from a class member node.
    ///
    /// If `skip_private` is true, returns `None` for private members.
    pub(crate) fn extract_class_member_info(
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
                    let init_type = self.get_type_of_node(prop.initializer);
                    if self.has_readonly_modifier(&prop.modifiers) {
                        init_type
                    } else {
                        self.widen_literal_type(init_type)
                    }
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
                    has_override: self.has_override_modifier(&prop.modifiers),
                    has_dynamic_name: self.is_computed_name_dynamic(prop.name),
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
                let signature = self.call_signature_from_method(method, member_idx);
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
                    has_override: self.has_override_modifier(&method.modifiers),
                    has_dynamic_name: self.is_computed_name_dynamic(method.name),
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
                    has_override: self.has_override_modifier(&accessor.modifiers),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
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
                    .map_or(TypeId::ANY, |param| {
                        if !param.type_annotation.is_none() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else {
                            TypeId::ANY
                        }
                    });
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
                    has_override: self.has_override_modifier(&accessor.modifiers),
                    has_dynamic_name: self.is_computed_name_dynamic(accessor.name),
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
        // If there are no heritage clauses, we still need to check for
        // invalid `override` members (TS4112) since override requires extends.
        let heritage_clauses = match class_data.heritage_clauses {
            Some(ref hc) => hc,
            None => {
                // No heritage clauses — still check for override members (TS4112)
                let derived_class_name = if !class_data.name.is_none() {
                    self.ctx
                        .arena
                        .get(class_data.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map_or_else(|| String::from("<anonymous>"), |id| id.escaped_text.clone())
                } else {
                    String::from("<anonymous>")
                };
                for &member_idx in &class_data.members.nodes {
                    let Some(info) = self.extract_class_member_info(member_idx, false) else {
                        continue;
                    };
                    if !info.has_override {
                        continue;
                    }
                    if info.has_dynamic_name {
                        self.error_at_node(
                            info.name_idx,
                            &crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                                &[],
                            ),
                            crate::diagnostics::diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                        );
                        continue;
                    }
                    self.error_at_node(
                        info.name_idx,
                        &crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                            &[&derived_class_name],
                        ),
                        crate::diagnostics::diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                    );
                }
                // Also check constructor parameter properties
                self.check_constructor_parameter_property_overrides(
                    class_data,
                    None,
                    &derived_class_name,
                    &rustc_hash::FxHashSet::default(),
                    self.ctx.no_implicit_override(),
                );
                return;
            }
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
                }

                // Find the base class declaration via heritage symbol resolution
                // This handles namespace scoping correctly
                if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx)
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
            break; // Only one extends clause is valid
        }

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
        let no_implicit_override = self.ctx.no_implicit_override();

        let Some(base_idx) = base_class_idx else {
            // Even without a base class, explicit `override` is invalid.
            for &member_idx in &class_data.members.nodes {
                let Some(info) = self.extract_class_member_info(member_idx, false) else {
                    continue;
                };
                if !info.has_override {
                    continue;
                }

                // Dynamic names are reported with a dedicated diagnostic.
                if info.has_dynamic_name {
                    self.error_at_node(
                        info.name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                            &[],
                        ),
                        diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                    );
                    continue;
                }

                self.error_at_node(
                    info.name_idx,
                    &crate::diagnostics::format_message(
                        diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                        &[&derived_class_name],
                    ),
                    diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                );
            }

            self.check_constructor_parameter_property_overrides(
                class_data,
                None,
                &derived_class_name,
                &rustc_hash::FxHashSet::default(),
                no_implicit_override,
            );

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
        if let Some(nodes) = base_type_argument_nodes.as_ref() {
            for arg_idx in nodes {
                type_args.push(self.get_type_from_type_node(*arg_idx));
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

        let mut base_instance_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut base_static_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        self.collect_base_member_names_for_override(
            base_idx,
            false,
            &mut base_instance_member_names,
            &mut rustc_hash::FxHashSet::default(),
        );
        self.collect_base_member_names_for_override(
            base_idx,
            true,
            &mut base_static_member_names,
            &mut rustc_hash::FxHashSet::default(),
        );

        self.check_constructor_parameter_property_overrides(
            class_data,
            Some(base_idx),
            &base_class_name,
            &base_instance_member_names,
            no_implicit_override,
        );

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
                has_override,
                has_dynamic_name,
            ) = (
                info.name,
                info.type_id,
                info.name_idx,
                info.visibility,
                info.is_method,
                info.is_static,
                info.is_accessor,
                info.has_override,
                info.has_dynamic_name,
            );

            // Skip override checking for private identifiers (#foo)
            // Private fields are scoped to the class that declares them and
            // do NOT participate in the inheritance hierarchy
            if member_name.starts_with('#') {
                continue;
            }

            let base_info =
                self.find_member_in_class_chain(base_idx, &member_name, is_static, 0, true);

            if has_override {
                // Cannot use `override` when name is computed dynamically.
                if has_dynamic_name {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                            &[],
                        ),
                        diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                    );
                    continue;
                }
            }

            if has_dynamic_name {
                // Dynamic names are allowed regardless of `noImplicitOverride`; they cannot
                // satisfy normal override checks because their exact identity cannot be
                // statically proven as an inherited symbol.
            } else if has_override {
                // `override` requires a matching visible base member.
                if base_info.is_none() {
                    let suggestion_names = if is_static {
                        &base_static_member_names
                    } else {
                        &base_instance_member_names
                    };
                    if let Some(suggestion) =
                        self.find_override_name_suggestion(suggestion_names, &member_name)
                    {
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                &[&base_class_name, &suggestion],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                        );
                    } else {
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                &[&base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                        );
                    }
                    continue;
                }
            } else if no_implicit_override && base_info.is_some() {
                if base_info
                    .as_ref()
                    .is_some_and(|base| base.is_abstract && base.is_method)
                {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH,
                            &[&base_class_name],
                        ),
                        diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH,
                    );
                } else {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE,
                            &[&base_class_name],
                        ),
                        diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE,
                    );
                }
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
                                "Class static side '{derived_class_name}' incorrectly extends base class static side '{base_class_name}'."
                            ),
                            diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                        );
                    } else {
                        self.error_at_node(
                            class_data.name,
                            &format!(
                                "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
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
                    if let Some(info) = self.extract_class_member_info(base_member_idx, true)
                        && info.name == member_name
                        && info.is_static == is_static
                    {
                        found = Some(info);
                        break;
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
            // Only applies to non-method, non-static members. Fires regardless of types (even ANY).
            // Static members are allowed to override accessors with properties and vice versa.
            if !is_method
                && !is_static
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
                            "'{member_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
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
                            "'{member_name}' is defined as a property in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an accessor."
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
                            "Class '{base_class_name}' defines instance member property '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
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
                            "Class '{base_class_name}' defines instance member accessor '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
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
                            "Class static side '{derived_class_name}' incorrectly extends base class static side '{base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                    );
                } else {
                    // TS2416: Instance member incompatibility
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{member_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_name}'."
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

        // Check index signature compatibility between derived and base classes (TS2415)
        self.check_class_index_signature_compatibility(
            class_data,
            base_class,
            &derived_class_name,
            &base_class_name,
            &substitution,
            class_extends_error_reported,
        );

        self.pop_type_parameters(base_type_param_updates);
    }

    // Index signature compatibility (TS2415), interface extension compatibility (TS2430),
    // member lookup in class chains, and visibility conflict detection are in
    // `class_checker_compat.rs`.
}
