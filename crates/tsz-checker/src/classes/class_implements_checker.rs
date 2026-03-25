//! Class interface and implements checking (TS2420, TS2515, TS2654, TS2720).
//! - Interface-extends-class accessibility checks

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};
use crate::query_boundaries::common::PropertyAccessResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    fn class_declaration_display_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> String {
        let base_name = if class_data.name.is_some() {
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

        let Some(type_params) = class_data.type_parameters.as_ref() else {
            return base_name;
        };

        let param_names: Vec<&str> = type_params
            .nodes
            .iter()
            .filter_map(|&idx| {
                let tp = self.ctx.arena.get_type_parameter_at(idx)?;
                let ident = self.ctx.arena.get_identifier_at(tp.name)?;
                Some(ident.escaped_text.as_str())
            })
            .collect();

        if param_names.is_empty() {
            base_name
        } else {
            format!("{base_name}<{}>", param_names.join(", "))
        }
    }

    fn implemented_interface_members(
        &mut self,
        interface_name: &str,
        interface_type: TypeId,
        type_args: &[TypeId],
        interface_declarations: &[NodeIndex],
        substitution: &crate::query_boundaries::common::TypeSubstitution,
    ) -> (Vec<PropertyInfo>, bool, String) {
        let array_display_name = |state: &Self| format!("{}[]", state.format_type(type_args[0]));

        if interface_name == "Array" && type_args.len() == 1 {
            let display_name = array_display_name(self);

            if let Some(array_base) = self.ctx.types.get_array_base_type()
                && let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, array_base)
            {
                let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    self.ctx.types.get_array_base_type_params(),
                    type_args,
                );
                let properties = shape
                    .properties
                    .iter()
                    .cloned()
                    .map(|mut prop| {
                        prop.type_id = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            prop.type_id,
                            &substitution,
                        );
                        prop
                    })
                    .collect();
                let has_index_signature =
                    shape.string_index.is_some() || shape.number_index.is_some();
                return (properties, has_index_signature, display_name);
            }
        }

        let display_name = if !type_args.is_empty() {
            self.format_type(interface_type)
        } else {
            interface_name.to_string()
        };

        if let Some(shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, interface_type)
        {
            let has_index_signature = shape.string_index.is_some() || shape.number_index.is_some();
            if !shape.properties.is_empty() {
                return (shape.properties.to_vec(), has_index_signature, display_name);
            }
        }

        let mut properties = Vec::new();
        let mut has_index_signature = false;

        for &decl_idx in interface_declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface_decl) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };

            for &member_idx in &interface_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                    has_index_signature = true;
                    continue;
                }
                if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE
                    && member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE
                {
                    continue;
                }

                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(sig.name) else {
                    continue;
                };

                // For method signatures, always build the full function type
                // (including parameters and method-level type parameters) via
                // get_type_of_interface_member_simple rather than using the
                // object-shape property type which only stores the return type.
                // This ensures proper TS2416 detection when comparing a class
                // method against a generic interface method signature.
                let member_type = if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    let member_type = self.get_type_of_interface_member_simple(member_idx);
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        member_type,
                        substitution,
                    )
                } else {
                    match self.resolve_property_access_with_env(interface_type, &name) {
                        PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            ..
                        } => write_type.unwrap_or(type_id),
                        _ => {
                            let member_type = self.get_type_of_interface_member_simple(member_idx);
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                member_type,
                                substitution,
                            )
                        }
                    }
                };

                properties.push(PropertyInfo {
                    name: self.ctx.types.intern_string(&name),
                    type_id: member_type,
                    write_type: member_type,
                    optional: sig.question_token,
                    readonly: false,
                    is_method: member_node.kind == syntax_kind_ext::METHOD_SIGNATURE,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: properties.len() as u32,
                });
            }
        }

        (properties, has_index_signature, display_name)
    }

    fn implemented_interface_display_name_from_syntax(
        &self,
        type_idx: NodeIndex,
        fallback: &str,
    ) -> String {
        let Some(type_node) = self.ctx.arena.get(type_idx) else {
            return fallback.to_string();
        };

        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(type_name) = self.node_text(type_ref.type_name)
            && type_name == "Array"
            && let Some(type_args) = type_ref.type_arguments.as_ref()
            && type_args.nodes.len() == 1
            && let Some(arg_text) = self.node_text(type_args.nodes[0])
        {
            return format!("{}[]", arg_text.trim().trim_end_matches('>'));
        }

        if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(type_ref) = self.ctx.arena.get_expr_type_args(type_node)
            && let Some(type_name) = self.node_text(type_ref.expression)
            && type_name == "Array"
            && let Some(type_args) = type_ref.type_arguments.as_ref()
            && type_args.nodes.len() == 1
            && let Some(arg_text) = self.node_text(type_args.nodes[0])
        {
            return format!("{}[]", arg_text.trim().trim_end_matches('>'));
        }

        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(type_name) = self.node_text(type_ref.type_name)
        {
            let type_name = type_name
                .split('<')
                .next()
                .unwrap_or(type_name.as_str())
                .trim();
            let type_name = type_name.rsplit('.').next().unwrap_or(type_name).trim();
            if let Some(type_args) = type_ref.type_arguments.as_ref()
                && !type_args.nodes.is_empty()
            {
                let args = type_args
                    .nodes
                    .iter()
                    .filter_map(|&arg_idx| self.node_text(arg_idx))
                    .map(|text| {
                        text.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .trim()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{type_name}<{args}>");
            }
            return type_name.to_string();
        }

        if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(type_ref) = self.ctx.arena.get_expr_type_args(type_node)
            && let Some(type_name) = self.node_text(type_ref.expression)
        {
            let type_name = type_name
                .split('<')
                .next()
                .unwrap_or(type_name.as_str())
                .trim();
            let type_name = type_name.rsplit('.').next().unwrap_or(type_name).trim();
            if let Some(type_args) = type_ref.type_arguments.as_ref()
                && !type_args.nodes.is_empty()
            {
                let args = type_args
                    .nodes
                    .iter()
                    .filter_map(|&arg_idx| self.node_text(arg_idx))
                    .map(|text| {
                        text.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .trim()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{type_name}<{args}>");
            }
            return type_name.to_string();
        }

        if let Some(text) = self.node_text(type_idx) {
            return text.trim().to_string();
        }

        fallback.to_string()
    }

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
        let mut heritage_expr_idx: Option<NodeIndex> = None;
        let mut heritage_type_idx: Option<NodeIndex> = None;

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

                heritage_expr_idx = Some(expr_idx);
                heritage_type_idx = Some(type_idx);

                if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                {
                    base_class_name = ident.escaped_text.clone();

                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&base_class_name)
                        && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    {
                        if symbol.value_declaration.is_some() {
                            base_class_idx = Some(symbol.value_declaration);
                        } else if let Some(&decl_idx) = symbol.declarations.first() {
                            base_class_idx = Some(decl_idx);
                        }
                    }
                }
            }
            break;
        }

        // If the base class was resolved to a non-class declaration (e.g., a const variable
        // holding a mixin result), clear it so we fall through to the type-level fallback.
        if let Some(base_idx) = base_class_idx
            && let Some(base_node) = self.ctx.arena.get(base_idx)
            && self.ctx.arena.get_class(base_node).is_none()
        {
            base_class_idx = None;
        }

        let Some(base_idx) = base_class_idx else {
            // Type-level fallback: resolve via the solver for expression-based heritage
            self.check_abstract_members_from_type(
                class_idx,
                class_data,
                heritage_expr_idx,
                heritage_type_idx,
                &base_class_name,
            );
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

        // TSC also considers members provided through declaration merging
        // (class + interface with same name).  Look up the class symbol and
        // check if any merged interface declarations contribute members that
        // satisfy the abstract requirement.
        if let Some(name_node) = self.ctx.arena.get(class_data.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let class_name = &ident.escaped_text;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(class_name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                for &decl_idx in &symbol.declarations {
                    // Skip the class declaration itself
                    if decl_idx == class_idx {
                        continue;
                    }
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    // Only consider interface declarations (declaration merging)
                    if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                        continue;
                    }
                    let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                        continue;
                    };
                    // Collect own members from the merged interface
                    for &member_idx in &iface.members.nodes {
                        if let Some(name) = self.get_member_name(member_idx) {
                            implemented_members.insert(name);
                        }
                    }
                    // Also collect inherited members from extends clauses
                    // via the solver's resolved type
                    if let Some(ref heritage) = iface.heritage_clauses {
                        for &clause_idx in &heritage.nodes {
                            if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                                && let Some(heritage_clause) =
                                    self.ctx.arena.get_heritage_clause(clause_node)
                            {
                                for &type_idx in &heritage_clause.types.nodes {
                                    let base_type = self.get_type_from_type_node(type_idx);
                                    let base_type = self.evaluate_type_for_assignability(base_type);
                                    if let Some(shape) = tsz_solver::type_queries::get_object_shape(
                                        self.ctx.types,
                                        base_type,
                                    ) {
                                        for prop in &shape.properties {
                                            let member_name =
                                                self.ctx.types.resolve_atom(prop.name);
                                            implemented_members.insert(member_name);
                                        }
                                    }
                                }
                            }
                        }
                    }
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
            let derived_class_name = if class_data.name.is_some() {
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
            // - TS2515: Single missing member: "Non-abstract class 'C' does not implement inherited abstract member bar from class 'B'."
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
                    // tsc points at the class name, not the `class` keyword
                    let error_node = if class_data.name.is_some() {
                        class_data.name
                    } else {
                        class_idx
                    };
                    self.error_at_node(
                        error_node,
                        &format!(
                            "Non-abstract class '{}' does not implement inherited abstract member {} from class '{}'.",
                            derived_class_name, missing_members[0], base_class_name
                        ),
                        diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS, // TS2515
                    );
                }
            } else {
                // tsc points at the class name for declarations, not the `class` keyword
                let error_node = if is_class_expression {
                    class_idx
                } else if class_data.name.is_some() {
                    class_data.name
                } else {
                    class_idx
                };

                // TSC uses different error codes and message format based on count:
                // - 2-4 members: TS2654/TS2656, lists all members
                // - 5+ members: TS2655/TS2650, shows first 4 then "and N more"
                if missing_members.len() > 4 {
                    let truncated_list = missing_members[..4]
                        .iter()
                        .map(|s| format!("'{s}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let remaining = missing_members.len() - 4;

                    if is_class_expression {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class expression is missing implementations for the following members of '{base_class_name}': {truncated_list} and {remaining} more."
                            ),
                            2650,
                        );
                    } else {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{base_class_name}': {truncated_list} and {remaining} more."
                            ),
                            2655,
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
                            error_node,
                            &format!(
                                "Non-abstract class expression is missing implementations for the following members of '{base_class_name}': {missing_list}."
                            ),
                            2656,
                        );
                    } else {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{base_class_name}': {missing_list}."
                            ),
                            diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
                        );
                    }
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
        let mut class_type_param_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        if let Some(params) = class_data.type_parameters.as_ref() {
            for &param_idx in &params.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(param_data.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                class_type_param_names.insert(ident.escaped_text.clone());
            }
        }

        // Collect implemented members from the class (name -> node_idx).
        // Member types are computed lazily only when needed for an interface match.
        let mut class_members: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        // Track method names with multiple declarations (overloads).
        // For overloaded methods, individual declaration types are incomplete —
        // the combined overloaded type must be used instead.
        let mut overloaded_methods: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                if class_members.contains_key(&name) {
                    overloaded_methods.insert(name.clone());
                }
                class_members.insert(name, member_idx);
            }
            if let Some(node) = self.ctx.arena.get(member_idx)
                && node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.ctx.arena.get_constructor(node)
            {
                for &param_idx in &ctor.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && self.has_parameter_property_modifier(&param.modifiers)
                        && let Some(name) = self.get_property_name(param.name)
                    {
                        class_members.insert(name, param_idx);
                    }
                }
            }
        }
        let mut class_member_types: rustc_hash::FxHashMap<NodeIndex, TypeId> =
            rustc_hash::FxHashMap::default();

        // For overloaded methods, get the combined type from the class instance type.
        // The instance type builder already aggregates all overload signatures into a
        // single callable type, which is what tsc checks against the interface.
        let mut overloaded_member_types: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        if !overloaded_methods.is_empty() {
            let class_instance_type = self.get_class_instance_type(class_idx, class_data);
            if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, class_instance_type)
            {
                for prop in &shape.properties {
                    let name = self.ctx.types.resolve_atom(prop.name);
                    if overloaded_methods.contains(&name) {
                        overloaded_member_types.insert(name, prop.type_id);
                    }
                }
            }
        }

        // Build a map of inherited PUBLIC instance members from the base class chain.
        // Only public members can satisfy interface requirements — private/protected inherited
        // members do NOT count, matching tsc's behavior.
        let mut inherited_member_types: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        self.collect_inherited_public_members(
            class_data,
            &class_members,
            &mut inherited_member_types,
        );

        // Also collect names of inherited PRIVATE/PROTECTED members. These don't
        // satisfy interface requirements, but when an interface extends the same base
        // class, these members appear in the interface type shape and must not be
        // reported as "missing" — they're inherited through the shared base class.
        let mut inherited_private_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        self.collect_inherited_private_member_names(
            class_data,
            &mut inherited_private_member_names,
        );

        // Get the class name for error messages
        let class_name = self.class_declaration_display_name(class_data);
        let class_error_idx = if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
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

                // Get the expression and type arguments from either
                // ExpressionWithTypeArguments or TypeReference.
                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };
                // TS2422: a class cannot implement one of its own type parameters.
                // This must be checked even when the type parameter resolves successfully.
                if !class_type_param_names.is_empty()
                    && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                    && class_type_param_names.contains(&ident.escaped_text)
                {
                    self.error_at_node(
                        expr_idx,
                        diagnostic_messages::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                        diagnostic_codes::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                    );
                    continue;
                }

                // Resolve interface/class symbols through canonical heritage resolution so
                // qualified names (e.g. `Promise.Thenable`) are handled correctly.
                if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    let interface_name = self
                        .heritage_name_text(expr_idx)
                        .unwrap_or_else(|| symbol.escaped_name.clone());

                    let is_class = (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0;

                    let mut interface_type_params = None;
                    let mut has_private_members = false;

                    // Track whether any merged interface declaration extends a class
                    // with private members that the implementing class CAN access vs
                    // ones it CANNOT access. When both exist, the conflict is already
                    // reported as TS2320 on the interface itself, so we suppress TS2420.
                    let mut any_inaccessible_privates = false;
                    let mut any_accessible_privates = false;

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
                                    any_inaccessible_privates = true;
                                } else if self
                                    .interface_extends_class_with_accessible_private_members(
                                        interface_decl,
                                        class_data,
                                    )
                                {
                                    any_accessible_privates = true;
                                }
                                if interface_type_params.is_none() {
                                    interface_type_params = interface_decl.type_parameters.clone();
                                }
                            }
                        }
                    }

                    // Only emit TS2420 for inaccessible private base members if
                    // there are no accessible ones from other merged declarations.
                    // When both exist, the interface itself has TS2320 (conflicting
                    // base types) which already covers the error.
                    if any_inaccessible_privates && !any_accessible_privates {
                        self.error_at_node(
                            class_error_idx,
                            &format!("Class '{class_name}' incorrectly implements interface '{interface_name}'."),
                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                        );
                    }

                    if has_private_members {
                        let message = format!(
                            "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?"
                        );
                        self.error_at_node(class_error_idx, &message, diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER);
                        continue;
                    }

                    // Check that all interface members are implemented with compatible types
                    let mut missing_members: Vec<String> = Vec::new();
                    let mut incompatible_members: Vec<(NodeIndex, String, String, String)> =
                        Vec::new(); // (node_idx, name, expected_type, actual_type)
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
                    let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                        self.ctx.types,
                        &interface_type_params,
                        &type_args,
                    );

                    let raw_interface_type = if is_class {
                        let mut instance_type = None;
                        for &decl_idx in &symbol.declarations {
                            if let Some(node) = self.ctx.arena.get(decl_idx)
                                && node.kind == syntax_kind_ext::CLASS_DECLARATION
                                && let Some(target_class_data) = self.ctx.arena.get_class(node)
                            {
                                instance_type =
                                    Some(self.get_class_instance_type(decl_idx, target_class_data));
                                break;
                            }
                        }
                        instance_type.unwrap_or_else(|| self.get_type_of_symbol(sym_id))
                    } else {
                        self.get_type_of_symbol(sym_id)
                    };
                    let interface_type = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        raw_interface_type,
                        &substitution,
                    );
                    let interface_type = self.evaluate_type_for_assignability(interface_type);
                    let (
                        interface_properties,
                        interface_has_index_signature,
                        interface_display_name,
                    ) = self.implemented_interface_members(
                        &interface_name,
                        interface_type,
                        &type_args,
                        &symbol.declarations,
                        &substitution,
                    );
                    let interface_display_name = self
                        .implemented_interface_display_name_from_syntax(
                            type_idx,
                            &interface_display_name,
                        );
                    // Compute the derived class instance type for `this` substitution.
                    // Interface methods may use `this` type (e.g. `view(vnode: Vnode<A, this>)`).
                    // When checking if the class implements the interface, `this` must be
                    // replaced with the class instance type.
                    let class_this_type = self
                        .ctx
                        .binder
                        .get_node_symbol(class_idx)
                        .and_then(|sym_id| self.class_instance_type_from_symbol(sym_id))
                        .or_else(|| self.current_this_type());

                    for prop in &interface_properties {
                        let member_name = self.ctx.types.resolve_atom(prop.name);
                        let mut interface_member_type = prop.type_id;
                        // Substitute `this` type in interface members
                        if let Some(this_type) = class_this_type
                            && tsz_solver::contains_this_type(self.ctx.types, interface_member_type)
                        {
                            interface_member_type = tsz_solver::substitute_this_type(
                                self.ctx.types,
                                interface_member_type,
                                this_type,
                            );
                        }

                        // Skip optional properties
                        if prop.optional {
                            continue;
                        }

                        // Skip private brand properties — these are synthetic markers
                        // for private member compatibility and are handled by the
                        // type-level assignability check, not member-by-member.
                        if member_name.starts_with("__private_brand_") {
                            continue;
                        }

                        // Check if class has this member
                        if let Some(&class_member_idx) = class_members.get(&member_name) {
                            // For overloaded methods, use the combined type from the
                            // class instance type (all overload signatures merged).
                            // For non-overloaded members, use the single declaration type.
                            let mut class_member_type = if let Some(&overloaded_type) =
                                overloaded_member_types.get(&member_name)
                            {
                                overloaded_type
                            } else if let Some(&cached) = class_member_types.get(&class_member_idx)
                            {
                                cached
                            } else {
                                let computed = self.get_type_of_class_member(class_member_idx);
                                class_member_types.insert(class_member_idx, computed);
                                computed
                            };
                            // Substitute `this` type in class members too — the class method
                            // may return `this` (polymorphic), which must be replaced with the
                            // concrete class instance type for a fair comparison against the
                            // interface member (which has already been this-substituted above).
                            if let Some(this_type) = class_this_type
                                && tsz_solver::contains_this_type(self.ctx.types, class_member_type)
                            {
                                class_member_type = tsz_solver::substitute_this_type(
                                    self.ctx.types,
                                    class_member_type,
                                    this_type,
                                );
                            }

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
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is private in type '{class_name}' but not in type '{interface_display_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }
                            if is_class_member_protected {
                                self.error_at_node(
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is protected in type '{class_name}' but not in type '{interface_display_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }

                            // Check type compatibility using regular assignability.
                            // tsc uses the assignable relation (not bivariant) for
                            // implements clause member type checking.
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
                        } else if let Some(&inherited_type) =
                            inherited_member_types.get(&member_name)
                        {
                            // Member inherited from base class — check type compatibility
                            // tsc uses the assignable relation for implements clause checks.
                            if interface_member_type != tsz_solver::TypeId::ANY
                                && inherited_type != tsz_solver::TypeId::ANY
                                && interface_member_type != tsz_solver::TypeId::ERROR
                                && inherited_type != tsz_solver::TypeId::ERROR
                                && should_report_member_type_mismatch(
                                    self,
                                    inherited_type,
                                    interface_member_type,
                                    class_idx,
                                )
                            {
                                let expected_str = self.format_type(interface_member_type);
                                let actual_str = self.format_type(inherited_type);
                                incompatible_members.push((
                                    class_error_idx,
                                    member_name.clone(),
                                    expected_str,
                                    actual_str,
                                ));
                            }
                        } else if !inherited_private_member_names.contains(&member_name) {
                            // Before reporting as missing, check the class instance type.
                            // Members from module augmentations or declaration merging appear
                            // in the computed instance type but not in the AST body or
                            // inheritance chain. E.g., `class X implements X {}` where X is
                            // augmented from another file via `declare module`.
                            let in_instance_type = {
                                let inst = self.get_class_instance_type(class_idx, class_data);
                                if let Some(shape) =
                                    tsz_solver::type_queries::get_object_shape(self.ctx.types, inst)
                                {
                                    let member_atom = self.ctx.types.intern_string(&member_name);
                                    shape.properties.iter().any(|p| p.name == member_atom)
                                } else {
                                    false
                                }
                            };
                            if !in_instance_type {
                                // Only report as missing if it's not a private/protected
                                // member inherited from the same base class chain.
                                missing_members.push(member_name);
                            }
                        }
                    }

                    // TS2559: Weak type detection for implements clauses.
                    // When the interface is a "weak type" (all properties optional,
                    // at least one property, no index signatures) and the class has
                    // no properties in common with the interface, tsc emits TS2559
                    // instead of silently passing. We detect this by checking
                    // assignability through the solver, which includes weak type
                    // detection via the compat layer.
                    if missing_members.is_empty() && incompatible_members.is_empty() {
                        // Check if the interface is a weak type: all properties optional
                        let is_weak = !interface_properties.is_empty()
                            && interface_properties.iter().all(|p| p.optional)
                            && !interface_has_index_signature;

                        if is_weak {
                            let class_instance_type =
                                self.get_class_instance_type(class_idx, class_data);
                            let analysis = self
                                .analyze_assignability_failure(class_instance_type, interface_type);
                            if matches!(
                                analysis.failure_reason,
                                Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                            ) {
                                let class_str = self.format_type(class_instance_type);
                                let iface_str = self.format_type(interface_type);
                                let message = crate::diagnostics::format_message(
                                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                                    &[&class_str, &iface_str],
                                );
                                self.error_at_node(
                                    class_error_idx,
                                    &message,
                                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                                );
                            }
                        }
                    }

                    // Check index signature compatibility (TS2420).
                    // When an interface has an index signature that the class
                    // doesn't satisfy, tsc emits TS2420.  We perform a full
                    // type-level assignability check of the class instance type
                    // against the interface type to catch this.
                    if interface_has_index_signature
                        && missing_members.is_empty()
                        && incompatible_members.is_empty()
                    {
                        let class_instance_type =
                            self.get_class_instance_type(class_idx, class_data);
                        if !self.is_assignable_to(class_instance_type, interface_type) {
                            let message = if is_class {
                                format!(
                                    "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?"
                                )
                            } else {
                                format!(
                                    "Class '{class_name}' incorrectly implements interface '{interface_display_name}'."
                                )
                            };
                            let diagnostic_code = if is_class {
                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
                            } else {
                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
                            };
                            self.error_at_node(class_error_idx, &message, diagnostic_code);
                        }
                    }

                    // Report error for missing members
                    let diagnostic_code = if is_class {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
                    } else {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
                    };

                    // tsc suppresses TS2420 (missing members) when there are
                    // incompatible members (TS2416). Only report missing members
                    // when no type mismatches were found.
                    if !missing_members.is_empty() && incompatible_members.is_empty() {
                        let missing_message = if missing_members.len() == 1 {
                            format!(
                                "Property '{}' is missing in type '{}' but required in type '{}'.",
                                missing_members[0], class_name, interface_display_name
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
                                "Type '{class_name}' is missing the following properties from type '{interface_display_name}': {formatted_list}"
                            )
                        };

                        let full_message = if is_class {
                            format!(
                                "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?\n  {missing_message}"
                            )
                        } else {
                            format!(
                                "Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  {missing_message}"
                            )
                        };

                        self.error_at_node(class_error_idx, &full_message, diagnostic_code);
                    }

                    // tsc suppresses TS2420 when TS2416 is present, but always
                    // emits TS2416 for incompatible member types — even when
                    // implementing a class (not interface). TS2720 is only used
                    // for missing members, not for type-incompatible members.
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
                                "Property '{member_name}' in type '{class_name}' is not assignable to the same property in base type '{interface_display_name}'."
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

    // ============================================================================
    // JSDoc @extends/@augments name mismatch checking (TS8023)
    // ============================================================================

    /// Check that JSDoc `@extends`/`@augments` tag argument matches the actual `extends` clause.
    ///
    /// In JS files, if a class has both `@extends {Foo}` and `extends Bar`,
    /// TSC emits TS8023: "JSDoc '@extends Foo' does not match the 'extends Bar' clause."
    pub(crate) fn check_jsdoc_extends_name_mismatch(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if !self.ctx.is_js_file() {
            return;
        }

        // Get the actual extends clause base class name
        let actual_extends_name = self.get_extends_clause_name(class_data);
        let Some(actual_name) = actual_extends_name else {
            return; // No extends clause, nothing to check
        };

        // Get the JSDoc comment range and search the raw source text
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return;
        };

        // Find the leading JSDoc comment range
        use tsz_common::comments::{get_leading_comments_from_cache, is_jsdoc_comment};
        let leading = get_leading_comments_from_cache(comments, node.pos, source_text);
        let Some(comment) = leading.last() else {
            return;
        };
        if !is_jsdoc_comment(comment, source_text) {
            return;
        }

        let comment_text = comment.get_text(source_text);

        // Search for @extends or @augments in the raw comment text
        for tag in ["augments", "extends"] {
            let needle = format!("@{tag}");
            for (match_pos, _) in comment_text.match_indices(&needle) {
                let after = match_pos + needle.len();
                if after >= comment_text.len() {
                    continue;
                }
                let next_ch = comment_text[after..]
                    .chars()
                    .next()
                    .expect("after < len checked above");
                if next_ch.is_ascii_alphanumeric() {
                    continue;
                }
                let rest = comment_text[after..].trim_start();
                if rest.is_empty() {
                    continue;
                }

                // Extract type name from {TypeName<...>} or TypeName
                let (jsdoc_type_name, type_name_in_rest) = if rest.starts_with('{') {
                    if let Some(close) = rest.find('}') {
                        let name = rest[1..close].trim();
                        (name, &rest[1..close])
                    } else {
                        continue;
                    }
                } else {
                    let end = rest
                        .find(|c: char| c.is_whitespace() || c == '*')
                        .unwrap_or(rest.len());
                    let name = rest[..end].trim();
                    (name, &rest[..end])
                };

                if jsdoc_type_name.is_empty() {
                    continue;
                }

                // Strip type arguments: "Foo<Bar>" → "Foo"
                let jsdoc_base_name = jsdoc_type_name
                    .find('<')
                    .map_or(jsdoc_type_name, |i| &jsdoc_type_name[..i]);

                // Check if the JSDoc @extends type name actually exists. If not,
                // emit TS2304 "Cannot find name" (tsc emits this alongside TS8023,
                // not instead of it).
                if !self.ctx.binder.file_locals.has(jsdoc_base_name) {
                    let type_name_offset =
                        type_name_in_rest.as_ptr() as usize - comment_text.as_ptr() as usize;
                    let error_pos = comment.pos + type_name_offset as u32;
                    let error_len = jsdoc_base_name.len() as u32;
                    let message =
                        format_message(diagnostic_messages::CANNOT_FIND_NAME, &[jsdoc_base_name]);
                    self.ctx.error(
                        error_pos,
                        error_len,
                        message,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                }

                if jsdoc_base_name != actual_name {
                    let message = format_message(
                        diagnostic_messages::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                        &[tag, jsdoc_type_name, &actual_name],
                    );
                    // Anchor at the type name argument in the JSDoc (matches TSC behavior)
                    let type_name_offset =
                        type_name_in_rest.as_ptr() as usize - comment_text.as_ptr() as usize;
                    let error_pos = comment.pos + type_name_offset as u32;
                    let error_len = jsdoc_type_name.len() as u32;
                    self.ctx.error(
                        error_pos,
                        error_len,
                        message,
                        diagnostic_codes::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                    );
                }
                return; // Only check first @extends/@augments tag
            }
        }
    }

    /// Get the base class name from the `extends` clause of a class declaration.
    fn get_extends_clause_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let heritage = class_data.heritage_clauses.as_ref()?;
        for &clause_idx in &heritage.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            if clause_node.kind != syntax_kind_ext::HERITAGE_CLAUSE {
                continue;
            }
            let clause = self.ctx.arena.get_heritage_clause(clause_node)?;
            // Check if this is an extends clause (not implements)
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            // Get the first type in the extends clause
            let first_type_idx = clause.types.nodes.first()?;
            let type_node = self.ctx.arena.get(*first_type_idx)?;
            // ExpressionWithTypeArguments — get the expression part
            if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                && let Some(ewta) = self.ctx.arena.get_expr_type_args(type_node)
            {
                return self.get_leftmost_identifier_name(ewta.expression);
            }
            // Direct identifier
            return self.get_leftmost_identifier_name(*first_type_idx);
        }
        None
    }

    // ============================================================================
    // JSDoc @implements checking
    // ============================================================================

    /// Extract type names from `@implements` JSDoc tags on a class declaration.
    /// Supports both `@implements {TypeName}` and `@implements TypeName` syntax.
    /// Returns a list of type name strings plus positions for empty tags that should emit TS1003.
    fn extract_jsdoc_implements_names(&self, class_idx: NodeIndex) -> (Vec<String>, Vec<u32>) {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return (Vec::new(), Vec::new());
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let Some(node) = self.ctx.arena.get(class_idx) else {
            return (Vec::new(), Vec::new());
        };

        let Some((jsdoc, jsdoc_start)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return (Vec::new(), Vec::new());
        };
        let leading =
            tsz_common::comments::get_leading_comments_from_cache(comments, node.pos, source_text);
        let raw_comment = leading
            .last()
            .and_then(|comment| source_text.get(comment.pos as usize..comment.end as usize))
            .unwrap_or("");

        let mut names = Vec::new();
        let mut missing_positions = Vec::new();
        let needle = "@implements";
        let raw_offsets: Vec<usize> = raw_comment
            .match_indices(needle)
            .filter_map(|(pos, _)| {
                let after = pos + needle.len();
                if after < raw_comment.len()
                    && raw_comment[after..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_alphanumeric())
                {
                    None
                } else {
                    Some(pos)
                }
            })
            .collect();

        let mut tag_index = 0usize;
        for (pos, _) in jsdoc.match_indices(needle) {
            let after = pos + needle.len();
            if after < jsdoc.len()
                && jsdoc[after..]
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
            {
                continue;
            }
            let raw_pos = raw_offsets.get(tag_index).copied();
            tag_index += 1;

            // Skip whitespace after @implements
            let rest = jsdoc.get(after..).unwrap_or("").trim_start();
            if rest.is_empty() {
                if let Some(raw_pos) = raw_pos {
                    missing_positions.push(jsdoc_start + raw_pos as u32 + needle.len() as u32);
                }
                continue;
            }

            // Extract type name — either `{TypeName}` or `TypeName`
            let type_name = if rest.starts_with('{') {
                // Find matching }
                if let Some(close) = rest.find('}') {
                    rest[1..close].trim()
                } else {
                    continue;
                }
            } else {
                // Take until whitespace or end of line
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '*')
                    .unwrap_or(rest.len());
                rest[..end].trim()
            };

            if !type_name.is_empty() {
                names.push(type_name.to_string());
            }
        }
        (names, missing_positions)
    }

    /// Check JSDoc `@implements` tags on a class declaration (JS files only).
    /// This is the JSDoc equivalent of syntactic `implements` clauses.
    /// Reports TS2420 (missing interface members), TS2416 (incompatible member types),
    /// and TS2720 (implementing a class instead of extending).
    pub(crate) fn check_jsdoc_implements_clauses(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        // Only check in JS files
        if !self.ctx.is_js_file() {
            return;
        }

        // Abstract classes don't need to implement interface members
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        let (implements_names, missing_positions) = self.extract_jsdoc_implements_names(class_idx);
        for pos in missing_positions {
            let already_emitted = self
                .ctx
                .diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::IDENTIFIER_EXPECTED && d.start == pos);
            if !already_emitted {
                self.emit_error_at(
                    pos,
                    0,
                    diagnostic_messages::IDENTIFIER_EXPECTED,
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }
        if implements_names.is_empty() {
            return;
        }

        // Get class name for error messages
        let class_name = if class_data.name.is_some() {
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
        let class_error_idx = if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
        };

        // Get the class instance type — this includes JS constructor this-properties
        let class_instance_type = self.get_class_instance_type(class_idx, class_data);

        // Collect class member names from instance type shape for existence checks
        let mut class_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut class_member_type_map: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        if let Some(shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, class_instance_type)
        {
            for prop in &shape.properties {
                let name = self.ctx.types.resolve_atom(prop.name);
                class_member_names.insert(name.clone());
                class_member_type_map.insert(name, prop.type_id);
            }
        }

        for target_name in &implements_names {
            // Resolve the target symbol from file_locals
            let Some(sym_id) = self.ctx.binder.file_locals.get(target_name) else {
                continue;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            let is_class = (symbol.flags & tsz_binder::symbol_flags::CLASS) != 0;

            // Check for private/protected members (TS2720 — should extend, not implement)
            let mut has_private_members = false;
            if is_class {
                for &decl_idx in &symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(base_class_data) = self.ctx.arena.get_class(node)
                        && self.class_has_private_or_protected_members(base_class_data)
                    {
                        has_private_members = true;
                    }
                }
            }

            if has_private_members {
                let message = format!(
                    "Class '{class_name}' incorrectly implements class '{target_name}'. Did you mean to extend '{target_name}' and inherit its members as a subclass?"
                );
                self.error_at_node(
                    class_error_idx,
                    &message,
                    diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER,
                );
                continue;
            }

            // Get the interface/class type and check members.
            // For classes, get_type_of_symbol returns the constructor type, so we need
            // to use get_class_instance_type to get the instance shape with members.
            let interface_type = if is_class {
                // Find the class declaration and get its instance type
                let mut instance_type = None;
                for &decl_idx in &symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(target_class_data) = self.ctx.arena.get_class(node)
                    {
                        instance_type =
                            Some(self.get_class_instance_type(decl_idx, target_class_data));
                        break;
                    }
                }
                instance_type.unwrap_or(TypeId::ERROR)
            } else {
                let raw_type = self.get_type_of_symbol(sym_id);
                self.evaluate_type_for_assignability(raw_type)
            };

            let mut missing_members: Vec<String> = Vec::new();
            let mut incompatible_members: Vec<(String, String, String)> = Vec::new();
            let mut interface_has_index_signature = false;

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
                    if let Some(&class_member_type) = class_member_type_map.get(&member_name) {
                        // Check type compatibility.
                        // Methods use bivariant relation; properties use regular assignability.
                        let mismatch_fn = if prop.is_method {
                            should_report_member_type_mismatch_bivariant
                        } else {
                            should_report_member_type_mismatch
                        };
                        if interface_member_type != TypeId::ANY
                            && class_member_type != TypeId::ANY
                            && interface_member_type != TypeId::ERROR
                            && class_member_type != TypeId::ERROR
                            && mismatch_fn(
                                self,
                                class_member_type,
                                interface_member_type,
                                class_idx,
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
            }

            // Check index signatures
            if interface_has_index_signature {
                let class_has_index_signature =
                    class_data.members.nodes.iter().any(|&member_idx| {
                        if let Some(member_node) = self.ctx.arena.get(member_idx) {
                            member_node.kind == syntax_kind_ext::INDEX_SIGNATURE
                        } else {
                            false
                        }
                    });

                if !class_has_index_signature && missing_members.is_empty() {
                    // tsc emits just the top-level message; index signature detail is a sub-diagnostic
                    self.error_at_node(
                        class_error_idx,
                        &format!(
                            "Class '{class_name}' incorrectly implements interface '{target_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                    );
                }
            }

            // Report missing members
            let diagnostic_code = if is_class {
                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
            } else {
                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
            };

            if !missing_members.is_empty() {
                let missing_message = if missing_members.len() == 1 {
                    format!(
                        "Property '{}' is missing in type '{}' but required in type '{}'.",
                        missing_members[0], class_name, target_name
                    )
                } else {
                    let formatted_list = if missing_members.len() > 4 {
                        let first_four = missing_members
                            .iter()
                            .take(4)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{}, and {} more", first_four, missing_members.len() - 4)
                    } else {
                        missing_members.join(", ")
                    };
                    format!(
                        "Type '{class_name}' is missing the following properties from type '{target_name}': {formatted_list}"
                    )
                };

                let full_message = if is_class {
                    format!(
                        "Class '{class_name}' incorrectly implements class '{target_name}'. Did you mean to extend '{target_name}' and inherit its members as a subclass?\n  {missing_message}"
                    )
                } else {
                    format!(
                        "Class '{class_name}' incorrectly implements interface '{target_name}'.\n  {missing_message}"
                    )
                };

                self.error_at_node(class_error_idx, &full_message, diagnostic_code);
            }

            // Report incompatible member types (TS2416)
            for (member_name, expected, actual) in incompatible_members {
                // For JSDoc @implements, we don't have a specific member node to point to,
                // so use the class name node for the error location.
                // Find the class member node if possible for better error location
                let error_node_idx = class_data
                    .members
                    .nodes
                    .iter()
                    .find_map(|&member_idx| {
                        if let Some(name) = self.get_member_name(member_idx)
                            && name == member_name
                        {
                            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                                self.get_member_name_node(member_node)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(class_error_idx);

                self.error_at_node(
                    error_node_idx,
                    &format!(
                        "Property '{member_name}' in type '{class_name}' is not assignable to the same property in base type '{target_name}'."
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
        }
    }

    // NOTE: check_abstract_members_from_type, find_abstract_members_in_type,
    // collect_class_names_from_instance_type, and is_property_abstract_via_parent
    // are in class_abstract_checker.rs
}
