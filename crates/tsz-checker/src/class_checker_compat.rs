//! Class and interface compatibility checking (TS2415, TS2430), member lookup
//! in class chains, and visibility conflict detection.

use crate::class_checker::{ClassMemberInfo, MemberVisibility};
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::class::should_report_member_type_mismatch;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_class_index_signature_compatibility(
        &mut self,
        derived_class: &tsz_parser::parser::node::ClassData,
        base_class: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        base_class_name: &str,
        substitution: &tsz_solver::TypeSubstitution,
        mut class_extends_error_reported: bool,
    ) {
        use tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE;
        use tsz_solver::instantiate_type;

        // Collect derived class index signatures
        let mut derived_string_index: Option<(TypeId, NodeIndex)> = None;
        let mut derived_number_index: Option<(TypeId, NodeIndex)> = None;

        for &member_idx in &derived_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                derived_number_index = Some((value_type, member_idx));
            } else {
                derived_string_index = Some((value_type, member_idx));
            }
        }

        // Collect base class index signatures
        let mut base_string_index: Option<TypeId> = None;
        let mut base_number_index: Option<TypeId> = None;

        for &member_idx in &base_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                base_number_index = Some(value_type);
            } else {
                base_string_index = Some(value_type);
            }
        }

        // Check string index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_string_index, base_string_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self
                .ctx
                .types
                .is_assignable_to(derived_type, base_type_instantiated)
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'string' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                class_extends_error_reported = true;
            }
        }

        // Check number index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_number_index, base_number_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self
                .ctx
                .types
                .is_assignable_to(derived_type, base_type_instantiated)
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'number' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
            }
        }
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
                                            "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
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

                    if let Some(class_idx) = base_class_idx
                        && let Some(class_node) = self.ctx.arena.get(class_idx)
                        && let Some(class_data) = self.ctx.arena.get_class(class_node)
                    {
                        // Check if any interface member redeclares a private/protected class member
                        for (member_name, _, derived_member_idx, _) in &derived_members {
                            for &class_member_idx in &class_data.members.nodes {
                                let Some(class_member_node) = self.ctx.arena.get(class_member_idx)
                                else {
                                    continue;
                                };

                                let (class_member_name, is_private_or_protected) =
                                    match class_member_node.kind {
                                        k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                            if let Some(prop) =
                                                self.ctx.arena.get_property_decl(class_member_node)
                                            {
                                                let name = self.get_property_name(prop.name);
                                                let is_priv_prot = self
                                                    .has_private_modifier(&prop.modifiers)
                                                    || self.has_protected_modifier(&prop.modifiers);
                                                (name, is_priv_prot)
                                            } else {
                                                continue;
                                            }
                                        }
                                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                            if let Some(method) =
                                                self.ctx.arena.get_method_decl(class_member_node)
                                            {
                                                let name = self.get_property_name(method.name);
                                                let is_priv_prot = self
                                                    .has_private_modifier(&method.modifiers)
                                                    || self
                                                        .has_protected_modifier(&method.modifiers);
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
                                                let name = self.get_property_name(accessor.name);
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

                                if let Some(class_member_name) = class_member_name
                                    && &class_member_name == member_name
                                    && is_private_or_protected
                                {
                                    // Interface redeclares a private/protected member as public - TS2430
                                    self.error_at_node(
                                        *derived_member_idx,
                                        &format!(
                                            "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
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
                                                        "Property '{member_name}' is private in type '{base_name}' but not in type '{derived_name}'."
                                                    ),
                                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                                );
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
                                                "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
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
                                        "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
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

    /// Find a member by name in a class, searching up the inheritance chain.
    /// Returns the member info if found, or None.
    /// Uses cycle detection to handle circular inheritance safely.
    pub(crate) fn find_member_in_class_chain(
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

    /// Internal implementation of `find_member_in_class_chain` with recursion guard.
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
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => {
                // Circular inheritance/depth/iteration limits detected - return None gracefully
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
            if let Some(info) = self.extract_class_member_info(member_idx, skip_private)
                && info.name == target_name
                && info.is_static == target_is_static
            {
                // Found it! Leave guard before returning
                guard.leave(class_idx);
                return Some(info);
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

    pub(crate) const fn class_member_visibility_conflicts(
        &self,
        derived_visibility: MemberVisibility,
        base_visibility: MemberVisibility,
    ) -> bool {
        matches!(
            (derived_visibility, base_visibility),
            (
                MemberVisibility::Private,
                MemberVisibility::Private | MemberVisibility::Protected | MemberVisibility::Public
            ) | (
                MemberVisibility::Protected,
                MemberVisibility::Public | MemberVisibility::Private
            ) | (MemberVisibility::Public, MemberVisibility::Private)
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
            if let Some(name_node) = self.ctx.arena.get(param.name)
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                continue;
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
