//! Class interface and implements checking (TS2420, TS2515, TS2654, TS2720).
//! - Interface-extends-class accessibility checks

use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::class::should_report_member_type_mismatch;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn report_type_not_assignable_detail(
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
                format!("Type '{source_type}' is not assignable to type '{target_type}'."),
                code,
            );
        }
    }

    pub(crate) fn report_property_type_incompatible_detail(
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
                format!("Types of property '{member_name}' are incompatible."),
                code,
            );
            self.error(
                pos,
                end - pos,
                format!("Type '{source_type}' is not assignable to type '{target_type}'."),
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
        let is_ambient = self.has_declare_modifier(&class_data.modifiers);
        if !is_ambient && !missing_members.is_empty() {
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

            let is_class_expression = self
                .ctx
                .arena
                .get(class_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

            // TypeScript uses different error codes based on the number of missing members and whether it's an expression:
            // - TS2515: Single missing member: "Non-abstract class 'C' does not implement inherited abstract member 'bar' from class 'B'."
            // - TS2653: Single missing member (class expression): "Non-abstract class expression does not implement inherited abstract member 'bar' from class 'B'."
            // - TS2654: Multiple missing members: "Non-abstract class 'C' is missing implementations for the following members of 'B': 'foo', 'bar'."
            // - TS2656: Multiple missing members (class expression): "Non-abstract class expression is missing implementations for the following members of 'B': 'foo', 'bar'."
            if missing_members.len() == 1 {
                if is_class_expression {
                    self.error_at_node(
                        class_idx,
                        &format!(
                            "Non-abstract class expression does not implement inherited abstract member '{}' from class '{}'.",
                            missing_members[0], base_class_name
                        ),
                        2653,
                    );
                } else {
                    self.error_at_node(
                        class_idx,
                        &format!(
                            "Non-abstract class '{}' does not implement inherited abstract member '{}' from class '{}'.",
                            derived_class_name, missing_members[0], base_class_name
                        ),
                        diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS, // TS2515
                    );
                }
            } else {
                let missing_list = missing_members
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(", ");

                if is_class_expression {
                    self.error_at_node(
                        class_idx,
                        &format!(
                            "Non-abstract class expression is missing implementations for the following members of '{base_class_name}': {missing_list}."
                        ),
                        2656,
                    );
                } else {
                    self.error_at_node(
                        class_idx,
                        &format!(
                            "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{base_class_name}': {missing_list}."
                        ),
                        diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
                    );
                }
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

        let _class_namespace = self.enclosing_namespace_node(class_idx);

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

                    let is_class = (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0;
                    let _is_interface = (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0;

                    let mut interface_type_params = None;
                    let mut has_private_members = false;

                    for &decl_idx in &symbol.declarations {
                        if let Some(node) = self.ctx.arena.get(decl_idx) {
                            if node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION {
                                if let Some(base_class_data) = self.ctx.arena.get_class(node) {
                                    if self.class_has_private_or_protected_members(base_class_data)
                                    {
                                        has_private_members = true;
                                    }
                                    if interface_type_params.is_none() {
                                        interface_type_params =
                                            base_class_data.type_parameters.clone();
                                    }
                                }
                            } else if node.kind
                                == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                                && let Some(interface_decl) = self.ctx.arena.get_interface(node)
                            {
                                if self.interface_extends_class_with_inaccessible_members(
                                    decl_idx,
                                    interface_decl,
                                    class_idx,
                                    class_data,
                                ) {
                                    self.error_at_node(
                                            type_idx,
                                            &format!("Class '{class_name}' incorrectly implements interface '{interface_name}'."),
                                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                        );
                                    // continue manually handled below if we break
                                }
                                if interface_type_params.is_none() {
                                    interface_type_params = interface_decl.type_parameters.clone();
                                }
                            }
                        }
                    }

                    if has_private_members {
                        let message = format!(
                            "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?"
                        );
                        self.error_at_node(type_idx, &message, diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER);
                        continue;
                    }

                    // Check that all interface members are implemented with compatible types
                    let mut missing_members: Vec<String> = Vec::new();
                    let mut incompatible_members: Vec<(NodeIndex, String, String, String)> =
                        Vec::new(); // (node_idx, name, expected_type, actual_type)
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
                        self.push_type_parameters(&interface_type_params);

                    // Fill in missing type arguments with defaults/constraints/unknown
                    if type_args.len() < interface_type_params.len() {
                        for param in interface_type_params.iter().skip(type_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(tsz_solver::TypeId::UNKNOWN);
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

                    let raw_interface_type = self.get_type_of_symbol(sym_id);
                    let interface_type = tsz_solver::instantiate_type(
                        self.ctx.types,
                        raw_interface_type,
                        &substitution,
                    );
                    let interface_type = self.evaluate_type_for_assignability(interface_type);

                    if let Some(shape) =
                        tsz_solver::type_queries::get_object_shape(self.ctx.types, interface_type)
                    {
                        if shape.string_index.is_some() || shape.number_index.is_some() {
                            interface_has_index_signature = true;
                        }

                        for prop in &shape.properties {
                            let member_name = self.ctx.types.resolve_atom(prop.name);
                            let interface_member_type = prop.type_id;

                            // Skip optional properties
                            if prop.optional {
                                continue;
                            }

                            // Check if class has this member
                            if let Some(&class_member_idx) = class_members.get(&member_name) {
                                let class_member_type = if let Some(&cached) =
                                    class_member_types.get(&class_member_idx)
                                {
                                    cached
                                } else {
                                    let computed = self.get_type_of_class_member(class_member_idx);
                                    class_member_types.insert(class_member_idx, computed);
                                    computed
                                };

                                // Check visibility (TS2420)
                                let sym_flags = self
                                    .ctx
                                    .binder
                                    .get_node_symbol(class_member_idx)
                                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                    .map(|s| s.flags)
                                    .unwrap_or(0);
                                let is_class_member_private =
                                    (sym_flags & tsz_binder::symbol_flags::PRIVATE) != 0;
                                let is_class_member_protected =
                                    (sym_flags & tsz_binder::symbol_flags::PROTECTED) != 0;
                                if is_class_member_private {
                                    self.error_at_node(
                                        class_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_name}'.\n  Property '{member_name}' is private in type '{class_name}' but not in type '{interface_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                    continue;
                                }
                                if is_class_member_protected {
                                    self.error_at_node(
                                        class_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_name}'.\n  Property '{member_name}' is protected in type '{class_name}' but not in type '{interface_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                    continue;
                                }

                                // Check type compatibility
                                if interface_member_type != tsz_solver::TypeId::ANY
                                    && class_member_type != tsz_solver::TypeId::ANY
                                    && interface_member_type != tsz_solver::TypeId::ERROR
                                    && class_member_type != tsz_solver::TypeId::ERROR
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
                                        class_member_idx,
                                        member_name.clone(),
                                        expected_str,
                                        actual_str,
                                    ));
                                }
                            } else {
                                missing_members.push(member_name);
                            }
                        }
                    }

                    // Check if interface has index signature but class doesn't
                    if interface_has_index_signature {
                        let class_has_index_signature =
                            class_data.members.nodes.iter().any(|&member_idx| {
                                if let Some(member_node) = self.ctx.arena.get(member_idx) {
                                    member_node.kind
                                        == tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE
                                } else {
                                    false
                                }
                            });

                        if !class_has_index_signature {
                            self.error_at_node(
                                clause_idx,
                                &format!(
                                    "Class '{class_name}' incorrectly implements interface '{interface_name}'. Index signature for type 'number' is missing in type '{class_name}'."
                                ),
                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                            );
                        }
                    }

                    // Report error for missing members
                    let diagnostic_code = if is_class {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
                    } else {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
                    };

                    let is_ambient = self.has_declare_modifier(&class_data.modifiers);
                    if !is_ambient && !missing_members.is_empty() {
                        let missing_message = if missing_members.len() == 1 {
                            format!(
                                "Property '{}' is missing in type '{}' but required in type '{}'.",
                                missing_members[0], class_name, interface_name
                            )
                        } else {
                            let missing_list = missing_members.clone();
                            let formatted_list = if missing_list.len() > 4 {
                                let first_four = missing_list
                                    .iter()
                                    .take(4)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("{}, and {} more", first_four, missing_list.len() - 4)
                            } else {
                                missing_list.join(", ")
                            };
                            format!(
                                "Type '{class_name}' is missing the following properties from type '{interface_name}': {formatted_list}"
                            )
                        };

                        let full_message = if is_class {
                            format!(
                                "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?\n  {missing_message}"
                            )
                        } else {
                            format!(
                                "Class '{class_name}' incorrectly implements interface '{interface_name}'.\n  {missing_message}"
                            )
                        };

                        self.error_at_node(type_idx, &full_message, diagnostic_code);
                    }

                    // Report error for incompatible member types
                    for (class_member_idx, member_name, expected, actual) in incompatible_members {
                        let error_node_idx =
                            if let Some(member_node) = self.ctx.arena.get(class_member_idx) {
                                self.get_member_name_node(member_node)
                                    .unwrap_or(class_member_idx)
                            } else {
                                class_member_idx
                            };
                        self.error_at_node(
                            error_node_idx,
                            &format!(
                                "Property '{member_name}' in type '{class_name}' is not assignable to the same property in base type '{interface_name}'."
                            ),
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                        );
                        self.report_type_not_assignable_detail(
                            error_node_idx,
                            &actual,
                            &expected,
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
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
    /// * `interface_idx` - The `NodeIndex` of the interface declaration
    /// * `interface_decl` - The interface data
    /// * `class_idx` - The `NodeIndex` of the implementing class
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
                                        && (self.has_private_modifier(&prop.modifiers)
                                            || self.has_protected_modifier(&prop.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                    if let Some(method) =
                                        self.ctx.arena.get_method_decl(member_node)
                                        && (self.has_private_modifier(&method.modifiers)
                                            || self.has_protected_modifier(&method.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::GET_ACCESSOR => {
                                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                                        && (self.has_private_modifier(&accessor.modifiers)
                                            || self.has_protected_modifier(&accessor.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::SET_ACCESSOR => {
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
                                            && (self.has_private_modifier(&prop.modifiers)
                                                || self.has_protected_modifier(&prop.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                        if let Some(method) =
                                            self.ctx.arena.get_method_decl(member_node)
                                            && (self.has_private_modifier(&method.modifiers)
                                                || self.has_protected_modifier(&method.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                            && (self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                            && (self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers))
                                        {
                                            return true;
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
}
