//! Member access-resolution helpers (class/interface/member utilities).

use crate::query_boundaries::definite_assignment::constructor_assigned_properties;
use crate::state::CheckerState;
use crate::statements::StatementCheckCallbacks;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn infer_property_type_from_class_member_assignments(
        &mut self,
        member_nodes: &[NodeIndex],
        prop_name: NodeIndex,
        is_static: bool,
    ) -> Option<TypeId> {
        let property_name = self.get_property_name(prop_name)?;
        let mut assigned_types = Vec::new();

        for &member_idx in member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if !is_static && member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                    continue;
                };
                if ctor.body.is_none() {
                    continue;
                }
                self.collect_class_member_assignment_types(
                    ctor.body,
                    &property_name,
                    member_nodes,
                    is_static,
                    &mut assigned_types,
                );
            } else if is_static
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                self.collect_class_member_assignment_types(
                    member_idx,
                    &property_name,
                    member_nodes,
                    is_static,
                    &mut assigned_types,
                );
            }
        }

        if assigned_types.is_empty() {
            None
        } else {
            Some(tsz_solver::utils::union_or_single(
                self.ctx.types,
                assigned_types,
            ))
        }
    }

    pub(crate) fn infer_property_type_from_enclosing_class_assignments(
        &mut self,
        prop_name: NodeIndex,
        is_static: bool,
    ) -> Option<TypeId> {
        let member_nodes = self.ctx.enclosing_class.as_ref()?.member_nodes.clone();
        self.infer_property_type_from_class_member_assignments(&member_nodes, prop_name, is_static)
    }

    pub(crate) fn property_assigned_in_enclosing_class_constructor(
        &mut self,
        prop_name: NodeIndex,
    ) -> bool {
        let Some(key) = self.property_key_from_name(prop_name) else {
            return false;
        };
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        let class_idx = class_info.class_idx;
        let member_nodes = class_info.member_nodes.clone();
        let requires_super = self
            .ctx
            .arena
            .get(class_idx)
            .and_then(|n| self.ctx.arena.get_class(n))
            .is_some_and(|class| self.class_has_base(class));

        let mut tracked = rustc_hash::FxHashSet::default();
        tracked.insert(key.clone());

        member_nodes.into_iter().any(|member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                return false;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                return false;
            };
            if ctor.body.is_none() {
                return false;
            }
            constructor_assigned_properties(self, ctor.body, &tracked, requires_super)
                .contains(&key)
        })
    }

    /// Check if a static property is assigned via `this.<prop> = ...` in any
    /// class static block. TSC suppresses TS7008 for static members that are
    /// assigned in static blocks, even when the member has no type annotation.
    pub(crate) fn property_assigned_in_enclosing_class_static_block(
        &self,
        prop_name: NodeIndex,
    ) -> bool {
        let Some(key) = self.property_key_from_name(prop_name) else {
            return false;
        };
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        let member_nodes = class_info.member_nodes.clone();
        let mut tracked = rustc_hash::FxHashSet::default();
        tracked.insert(key.clone());

        member_nodes.into_iter().any(|member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return false;
            }
            // Static blocks are stored as BlockData — analyze their statements
            // for `this.<prop> = ...` patterns using the same flow analysis as
            // constructor assignment checking (no super() requirement).
            self.analyze_constructor_assignments(member_idx, &tracked, false)
                .contains(&key)
        })
    }

    fn collect_class_member_assignment_types(
        &mut self,
        node_idx: NodeIndex,
        property_name: &str,
        member_nodes: &[NodeIndex],
        is_static: bool,
        assigned_types: &mut Vec<TypeId>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::CLASS_EXPRESSION => return,
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.ctx.arena.get_binary_expr(node)
                    && bin.operator_token == tsz_scanner::SyntaxKind::EqualsToken as u16
                    && self
                        .this_access_name(bin.left)
                        .as_deref()
                        .is_some_and(|name| name == property_name)
                {
                    let mut rhs_type = self.get_type_of_node(bin.right);
                    if rhs_type == TypeId::ANY
                        && let Some(name_idx) = self.this_access_name_node(bin.right)
                        && let Some(ref_name) = self.get_property_name(name_idx)
                        && ref_name != property_name
                    {
                        rhs_type = self
                            .class_member_declared_type(member_nodes, name_idx, is_static)
                            .or_else(|| {
                                self.infer_property_type_from_class_member_assignments(
                                    member_nodes,
                                    name_idx,
                                    is_static,
                                )
                            })
                            .unwrap_or(rhs_type);
                    }
                    let rhs_type = self.widen_literal_type(rhs_type);
                    if rhs_type != TypeId::ERROR && rhs_type != TypeId::ANY {
                        assigned_types.push(rhs_type);
                    }
                }
            }
            _ => {}
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_class_member_assignment_types(
                child_idx,
                property_name,
                member_nodes,
                is_static,
                assigned_types,
            );
        }
    }

    fn this_access_name(&self, access_idx: NodeIndex) -> Option<String> {
        let name_idx = self.this_access_name_node(access_idx)?;
        self.get_property_name(name_idx)
    }

    pub(crate) fn this_access_name_node(&self, access_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(access_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let expr_node = self.ctx.arena.get(access.expression)?;
        if expr_node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return None;
        }

        Some(access.name_or_argument)
    }

    fn class_member_declared_type(
        &mut self,
        member_nodes: &[NodeIndex],
        prop_name: NodeIndex,
        is_static: bool,
    ) -> Option<TypeId> {
        let property_name = self.get_property_name(prop_name)?;

        for &member_idx in member_nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.has_static_modifier(&prop.modifiers) != is_static {
                continue;
            }
            if self.get_property_name(prop.name).as_deref() != Some(property_name.as_str()) {
                continue;
            }

            if let Some(&type_id) = self.ctx.node_types.get(&member_idx.0) {
                return Some(type_id);
            }
            if let Some(declared_type) =
                self.effective_class_property_declared_type(member_idx, prop)
            {
                return Some(declared_type);
            }
            if prop.initializer.is_some() {
                return Some(self.get_type_of_node(prop.initializer));
            }
        }

        None
    }

    fn enclosing_class_constructor_param_names(&self) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();

        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return names;
        };

        for &member_idx in &class_info.member_nodes {
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
                if let Some(name) = self.get_node_text(param.name) {
                    names.insert(name);
                }
            }
        }

        names
    }

    fn symbol_declared_within_subtree(
        &self,
        sym_id: tsz_binder::SymbolId,
        root_idx: NodeIndex,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        if symbol.value_declaration.is_some()
            && self.is_node_within(symbol.value_declaration, root_idx)
        {
            return true;
        }

        symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_node_within(decl_idx, root_idx))
    }

    fn enclosing_constructor_of_node(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        let mut steps = 0;
        while steps < 256 {
            steps += 1;
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_idx = ext.parent;
            let parent_node = self.ctx.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    fn symbol_is_constructor_parameter_of_current_class(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let mut decl_nodes = symbol.declarations.clone();
        if symbol.value_declaration.is_some() {
            decl_nodes.push(symbol.value_declaration);
        }

        decl_nodes.into_iter().any(|decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if self.ctx.arena.get_parameter(decl_node).is_none() {
                return false;
            }

            self.enclosing_constructor_of_node(decl_idx)
                .is_some_and(|ctor_idx| class_info.member_nodes.contains(&ctor_idx))
        })
    }

    fn collect_unqualified_identifier_references(
        &self,
        node_idx: NodeIndex,
        refs: &mut Vec<(String, NodeIndex)>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                refs.push((ident.escaped_text.clone(), node_idx));
            }
            return;
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                self.collect_unqualified_identifier_references(access.expression, refs);
            }
            return;
        }

        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            if let Some(access) = self.ctx.arena.get_access_expr(node) {
                self.collect_unqualified_identifier_references(access.expression, refs);
                self.collect_unqualified_identifier_references(access.name_or_argument, refs);
            }
            return;
        }

        if let Some(func) = self.ctx.arena.get_function(node) {
            for &param_idx in &func.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    && param.initializer.is_some()
                {
                    self.collect_unqualified_identifier_references(param.initializer, refs);
                }
            }
            if func.body.is_some() {
                self.collect_unqualified_identifier_references(func.body, refs);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.collect_unqualified_identifier_references(stmt_idx, refs);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.collect_unqualified_identifier_references(expr_stmt.expression, refs);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &list_idx in &var_stmt.declarations.nodes {
                        if let Some(list_node) = self.ctx.arena.get(list_idx)
                            && let Some(decl_list) = self.ctx.arena.get_variable(list_node)
                        {
                            for &decl_idx in &decl_list.declarations.nodes {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && let Some(var_decl) =
                                        self.ctx.arena.get_variable_declaration(decl_node)
                                    && var_decl.initializer.is_some()
                                {
                                    self.collect_unqualified_identifier_references(
                                        var_decl.initializer,
                                        refs,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.collect_unqualified_identifier_references(call.expression, refs);
                    if let Some(args) = &call.arguments {
                        for &arg_idx in &args.nodes {
                            self.collect_unqualified_identifier_references(arg_idx, refs);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.collect_unqualified_identifier_references(paren.expression, refs);
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.collect_unqualified_identifier_references(binary.left, refs);
                    self.collect_unqualified_identifier_references(binary.right, refs);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.ctx.arena.get_conditional_expr(node) {
                    self.collect_unqualified_identifier_references(cond.condition, refs);
                    self.collect_unqualified_identifier_references(cond.when_true, refs);
                    self.collect_unqualified_identifier_references(cond.when_false, refs);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn check_constructor_param_capture_in_instance_initializer(
        &mut self,
        member_name: &str,
        initializer_idx: NodeIndex,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let ctor_param_names = self.enclosing_class_constructor_param_names();
        if ctor_param_names.is_empty() {
            return;
        }

        let mut refs = Vec::new();
        self.collect_unqualified_identifier_references(initializer_idx, &mut refs);

        for (name, ident_idx) in refs {
            if !ctor_param_names.contains(&name) {
                continue;
            }

            if let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, ident_idx)
            {
                if self.symbol_is_constructor_parameter_of_current_class(sym_id) {
                    self.error_at_node_msg(
                        ident_idx,
                        diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                        &[&name],
                    );
                    continue;
                }

                let treat_as_unresolved =
                    self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        let source_is_external_module = self
                            .ctx
                            .get_binder_for_file(symbol.decl_file_idx as usize)
                            .is_some_and(tsz_binder::BinderState::is_external_module);

                        self.ctx.binder.is_external_module()
                            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
                            && (source_is_external_module || symbol.is_exported)
                            && (symbol.flags
                                & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                                    | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE))
                                != 0
                    });

                if treat_as_unresolved {
                    self.error_at_node_msg(
                        ident_idx,
                        diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                        &[&name],
                    );
                    continue;
                }

                if self.symbol_declared_within_subtree(sym_id, initializer_idx) {
                    continue;
                }

                self.error_at_node_msg(
                    ident_idx,
                    diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN,
                    &[member_name, &name],
                );
            } else {
                self.error_at_node_msg(
                    ident_idx,
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS,
                    &[&name],
                );
            }
        }
    }

    /// Check an interface declaration.
    pub(crate) fn check_interface_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(iface) = self.ctx.arena.get_interface(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on interface declarations
        self.check_async_modifier_on_declaration(&iface.modifiers);

        // Check for reserved interface names (error 2427)
        if iface.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(iface.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            // Reserved type names that can't be used as interface names
            match ident.escaped_text.as_str() {
                "string" | "number" | "boolean" | "symbol" | "void" | "object" | "any"
                | "unknown" | "never" | "bigint" | "intrinsic" | "undefined" | "null" => {
                    self.error_at_node(
                        iface.name,
                        &format!("Interface name cannot be '{}'.", ident.escaped_text),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
                _ => {}
            }
        }

        // NOTE: TSC does NOT emit TS1212 for interface declaration names.
        // e.g. `interface interface {}` gets TS1438 only, not TS1212.

        // Check for circular inheritance (TS2310)
        // Must be done before resolving types to avoid infinite recursion
        use crate::class_inheritance::ClassInheritanceChecker;
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if checker.check_interface_inheritance_cycle(stmt_idx, iface) {
            // If cycle detected, we can still proceed with checking members but
            // heritage graph is now aware of the cycle (or it was reported)
        }

        // Push type parameters BEFORE checking heritage clauses
        // This allows heritage clauses to reference the interface's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&iface.type_parameters);

        // Check for duplicate type parameters
        self.check_duplicate_type_parameters(&iface.type_parameters);

        // Collect interface type parameter names for TS2304 checking in heritage clauses
        let interface_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect();

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &iface.heritage_clauses,
            false,
            &interface_type_param_names,
        );

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&iface.type_parameters, stmt_idx);

        // Check each interface member for missing type references and parameter properties
        for &member_idx in &iface.members.nodes {
            self.check_type_member_for_missing_names(member_idx);
            self.check_type_member_for_parameter_properties(member_idx);
            // TS1268: Check index signature parameter types
            self.check_index_signature_parameter_type(member_idx);
            // TS1169: Computed property in interface must have literal/unique symbol type
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(sig) = self.ctx.arena.get_signature(member_node)
            {
                self.check_interface_computed_property_name(sig.name);
            }
        }

        // TS2386: Check optionality agreement for interface method overloads
        {
            use rustc_hash::FxHashMap;

            // Group method signatures by name
            let mut method_groups: FxHashMap<String, Vec<(NodeIndex, bool)>> = FxHashMap::default();
            for &member_idx in &iface.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(sig.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                method_groups
                    .entry(ident.escaped_text.clone())
                    .or_default()
                    .push((member_idx, sig.question_token));
            }
            for members in method_groups.values() {
                if members.len() < 2 {
                    continue;
                }
                let first_optional = members[0].1;
                for &(member_idx, optional) in &members[1..] {
                    if optional != first_optional {
                        let error_node = self
                            .ctx
                            .arena
                            .get(member_idx)
                            .and_then(|n| self.ctx.arena.get_signature(n))
                            .map(|s| s.name)
                            .unwrap_or(member_idx);
                        self.error_at_node(
                            error_node,
                            crate::diagnostics::diagnostic_messages::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                            crate::diagnostics::diagnostic_codes::OVERLOAD_SIGNATURES_MUST_ALL_BE_OPTIONAL_OR_REQUIRED,
                        );
                    }
                }
            }
        }

        // Check for duplicate member names (TS2300)
        self.check_duplicate_interface_members(&iface.members.nodes);

        // Check that properties are assignable to index signatures (TS2411)
        // This includes both directly declared and inherited index signatures.
        // Get the interface type to check for any index signatures (direct or inherited)
        // NOTE: Use get_type_of_symbol to get the cached type, avoiding recursion issues
        let iface_type = if iface.name.is_some() {
            // Get symbol from the interface name and resolve its type
            if let Some(name_node) = self.ctx.arena.get(iface.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) {
                        self.get_type_of_symbol(sym_id)
                    } else {
                        TypeId::ERROR
                    }
                } else {
                    TypeId::ERROR
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Anonymous interface - compute type directly
            self.get_type_of_interface(stmt_idx)
        };

        let index_info = self.ctx.types.get_index_signatures(iface_type);

        // Check if there are own index signatures by scanning members
        let has_own_index_sig = iface.members.nodes.iter().any(|&member_idx| {
            self.ctx.arena.get(member_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE
            })
        });

        // If there are any index signatures (direct, own, or inherited), check compatibility
        if index_info.string_index.is_some()
            || index_info.number_index.is_some()
            || has_own_index_sig
        {
            self.check_index_signature_compatibility(&iface.members.nodes, iface_type, stmt_idx);

            // Also check inherited members from base interfaces against index
            // signatures. The AST-based check above only sees own members; inherited
            // properties live in the solver's resolved type and must be checked too.
            if iface.heritage_clauses.is_some() {
                self.check_inherited_properties_against_index_signatures(
                    iface_type,
                    &iface.members.nodes,
                    stmt_idx,
                );
            }
        }

        // Check that interface correctly extends base interfaces (error 2430)
        self.check_interface_extension_compatibility(stmt_idx, iface);

        self.pop_type_parameters(type_param_updates);
    }

    /// Check for duplicate property names in interface members (TS2300).
    /// TypeScript reports "Duplicate identifier 'X'." for each duplicate occurrence.
    /// NOTE: Method signatures (overloads) are NOT considered duplicates - interfaces allow
    /// multiple method signatures with the same name for function overloading.
    pub(crate) fn check_duplicate_interface_members(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track property names → (member_idx, type_annotation_node) pairs.
        // Methods are allowed to have overloads so they are excluded.
        let mut seen_properties: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>> =
            FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check property signatures for duplicates
            // Method signatures can have multiple overloads (same name, different types)
            let name_and_type = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                    self.ctx.arena.get_signature(member_node).and_then(|sig| {
                        let name = self.get_member_name_text(sig.name)?;
                        Some((name, sig.type_annotation))
                    })
                }
                // Method signatures are allowed to have overloads - don't flag as duplicates
                k if k == syntax_kind_ext::METHOD_SIGNATURE => None,
                // Call, construct, and index signatures don't have names that can conflict
                _ => None,
            };

            if let Some((name, type_ann)) = name_and_type {
                // tsc does not flag duplicate well-known Symbol properties in interfaces
                // (e.g., [Symbol.isConcatSpreadable]) because symbols are structurally unique.
                if name.starts_with("[Symbol.") {
                    continue;
                }
                seen_properties
                    .entry(name)
                    .or_default()
                    .push((member_idx, type_ann));
            }
        }

        // Report errors for duplicates — tsc reports TS2300 on ALL occurrences
        // (both first and subsequent), not just the second+.
        for (name, entries) in &seen_properties {
            if entries.len() > 1 {
                // Resolve the first property's type for TS2717 comparison
                let first_type = if entries[0].1.is_some() {
                    self.get_type_from_type_node(entries[0].1)
                } else {
                    TypeId::ANY
                };

                for (i, &(idx, type_ann)) in entries.iter().enumerate() {
                    // TS2300 on all occurrences
                    let error_node = self.get_interface_member_name_node(idx).unwrap_or(idx);
                    self.error_at_node_msg(
                        error_node,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                        &[name],
                    );

                    // TS2717 on subsequent declarations when types differ
                    if i > 0 {
                        let this_type = if type_ann.is_some() {
                            self.get_type_from_type_node(type_ann)
                        } else {
                            TypeId::ANY
                        };
                        if !self.type_contains_error(first_type)
                            && !self.type_contains_error(this_type)
                        {
                            // TS2717 uses type identity, not assignability.
                            // With interned types, TypeId equality is structural identity.
                            if first_type != this_type {
                                let first_type_str = self.format_type(first_type);
                                let this_type_str = self.format_type(this_type);
                                self.error_at_node_msg(
                                    error_node,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[name, &first_type_str, &this_type_str],
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get property information needed for index signature checking.
    /// Returns (`property_name`, `property_type`, `name_node_index`).
    /// Get the name text from a member name node for duplicate member detection.
    ///
    /// Delegates to `get_literal_property_name` for non-computed names, then handles
    /// computed property names specially: string literals are wrapped as `["text"]`
    /// (matching tsc's diagnostic format), numeric literals are canonicalized, and
    /// well-known symbols like `Symbol.hasInstance` are formatted as `[Symbol.xxx]`.
    pub(crate) fn get_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        // Try non-computed property name first
        if let Some(name) =
            crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
        {
            return Some(name);
        }

        // Handle computed property names with diagnostic-specific formatting
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let expr_node = self.ctx.arena.get(computed.expression)?;
            match expr_node.kind {
                ek if ek == tsz_scanner::SyntaxKind::StringLiteral as u16 => {
                    // tsc formats computed string literals as ["a"] in diagnostics
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(format!("[\"{}\"]", lit.text));
                }
                ek if ek == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    let lit = self.ctx.arena.get_literal(expr_node)?;
                    return Some(
                        tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                            .unwrap_or_else(|| lit.text.clone()),
                    );
                }
                ek if ek == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    // Handle well-known symbols like Symbol.hasInstance
                    let access = self.ctx.arena.get_access_expr(expr_node)?;
                    let obj_node = self.ctx.arena.get(access.expression)?;
                    let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
                    if obj_ident.escaped_text.as_str() == "Symbol" {
                        let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                        let prop_ident = self.ctx.arena.get_identifier(prop_node)?;
                        return Some(format!("[Symbol.{}]", prop_ident.escaped_text));
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Get the name node from an interface member for error reporting.
    fn get_interface_member_name_node(&self, member_idx: NodeIndex) -> Option<NodeIndex> {
        let member_node = self.ctx.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| idx.is_some()),
            k if k == syntax_kind_ext::METHOD_SIGNATURE => self
                .ctx
                .arena
                .get_signature(member_node)
                .map(|sig| sig.name)
                .filter(|idx: &NodeIndex| idx.is_some()),
            _ => None,
        }
    }

    /// Get the display text for a class member name, matching TSC's `declarationNameToString`.
    ///
    /// Unlike `get_member_name_text` which canonicalizes numeric names for dedup keys,
    /// this preserves the original source representation for diagnostic messages.
    /// - Identifiers: `foo` → `"foo"`
    /// - Numeric literals: `0.0` → `"0.0"` (NOT canonicalized to `"0"`)
    /// - String literals: `'0'` → `"'0'"` (wrapped in single quotes)
    pub(crate) fn get_member_name_display_text(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        let name_node = self.ctx.arena.get(name_idx)?;

        // Identifier — same as canonical
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(ident.escaped_text.clone());
        }

        // String literal — wrap in single quotes like TSC's declarationNameToString
        if name_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(format!("'{}'", lit.text));
        }

        // Numeric literal — preserve source text (no canonicalization)
        if name_node.kind == tsz_scanner::SyntaxKind::NumericLiteral as u16
            && let Some(lit) = self.ctx.arena.get_literal(name_node)
        {
            return Some(lit.text.clone());
        }

        // Fall back to get_member_name_text for computed properties, etc.
        self.get_member_name_text(name_idx)
    }

    /// Report TS2300 "Duplicate identifier" error for a class member (property or method).
    /// Helper function to avoid code duplication in `check_duplicate_class_members`.
    fn report_duplicate_class_member_ts2300(&mut self, member_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let member_node = self.ctx.arena.get(member_idx);
        let (name_idx, error_node) = match member_node.map(|n| n.kind) {
            Some(k) if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let prop = self.ctx.arena.get_property_decl(member_node.unwrap());
                let name_idx = prop.map(|p| p.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            Some(k) if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.ctx.arena.get_method_decl(member_node.unwrap());
                let name_idx = method.map(|m| m.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            Some(k) if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.ctx.arena.get_accessor(member_node.unwrap());
                let name_idx = accessor.map(|a| a.name).filter(|idx| idx.is_some());
                let node = name_idx.unwrap_or(member_idx);
                (name_idx, node)
            }
            _ => return,
        };

        // Use display text (preserves source representation) for the diagnostic message,
        // matching TSC's declarationNameToString behavior.
        if let Some(name_idx) = name_idx
            && let Some(display_name) = self.get_member_name_display_text(name_idx)
        {
            self.error_at_node_msg(
                error_node,
                diagnostic_codes::DUPLICATE_IDENTIFIER,
                &[&display_name],
            );
        }
    }

    /// Extract explicit type annotation info for a class property declaration.
    fn get_class_property_declared_type_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, TypeId)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
            return None;
        }

        let prop = self.ctx.arena.get_property_decl(member_node)?;
        let name = self.get_member_name_text(prop.name)?;

        let type_id = if let Some(declared_type) =
            self.effective_class_property_declared_type(member_idx, prop)
        {
            declared_type
        } else if prop.initializer.is_some() {
            // Infer type from initializer when no explicit annotation
            self.get_type_of_node(prop.initializer)
        } else {
            return None;
        };
        Some((name, prop.name, type_id))
    }

    /// Extract type info for a class accessor declaration.
    /// For getters, use explicit return annotation if present, otherwise infer from body.
    /// For setters, use the first parameter type annotation (or `any` if omitted).
    fn get_class_accessor_type_info(
        &mut self,
        member_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, TypeId, bool)> {
        let member_node = self.ctx.arena.get(member_idx)?;
        if member_node.kind != syntax_kind_ext::GET_ACCESSOR
            && member_node.kind != syntax_kind_ext::SET_ACCESSOR
        {
            return None;
        }

        let accessor = self.ctx.arena.get_accessor(member_node)?;
        let name = self.get_member_name_text(accessor.name)?;
        let is_static = self.has_static_modifier(&accessor.modifiers);

        let type_id = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
            if accessor.type_annotation.is_some() {
                self.get_type_from_type_node(accessor.type_annotation)
            } else if accessor.body.is_some() {
                self.infer_getter_return_type(accessor.body)
            } else {
                TypeId::ANY
            }
        } else if let Some(&first_param_idx) = accessor.parameters.nodes.first() {
            if let Some(param) = self.ctx.arena.get_parameter_at(first_param_idx) {
                if param.type_annotation.is_some() {
                    self.get_type_from_type_node(param.type_annotation)
                } else {
                    TypeId::ANY
                }
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::ANY
        };

        Some((name, accessor.name, type_id, is_static))
    }

    /// Check for duplicate property/method names in class members (TS2300, TS2393).
    /// TypeScript reports:
    /// - TS2300 "Duplicate identifier 'X'." for duplicate properties
    /// - TS2393 "Duplicate function implementation." for multiple method implementations
    ///
    /// NOTE: Method overloads (signatures + implementation) are allowed:
    ///   foo(x: number): void;    // overload signature
    ///   foo(x: string): void;    // overload signature  
    ///   foo(x: any) { }          // implementation - this is valid!
    pub(crate) fn check_duplicate_class_members(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use rustc_hash::FxHashMap;

        // Track member names with their info
        struct MemberInfo {
            indices: Vec<NodeIndex>,
            is_property: Vec<bool>, // true for PROPERTY_DECLARATION, false for METHOD_DECLARATION
            method_has_body: Vec<bool>, // only valid when is_property is false
            is_static: Vec<bool>,
        }

        let mut seen_names: FxHashMap<String, MemberInfo> = FxHashMap::default();
        let mut constructor_declarations: Vec<NodeIndex> = Vec::new();
        let mut constructor_implementations: Vec<NodeIndex> = Vec::new();

        // Track accessor occurrences for duplicate detection
        // Key: "get:name" or "set:name" (with "static:" prefix for static members)
        let mut seen_accessors: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        // Track accessor plain names (without get/set prefix) for cross-checking
        // against properties/methods. Key: "name" or "static:name"
        let mut accessor_plain_names: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        for &member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Get the member name and type info
            let (name, is_property, method_has_body, is_static) = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|prop| {
                        let is_static = self.has_static_modifier(&prop.modifiers);
                        self.get_member_name_text(prop.name)
                            .map(|n| (n, true, false, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|method| {
                        let has_body = method.body.is_some();
                        let is_static = self.has_static_modifier(&method.modifiers);
                        self.get_member_name_text(method.name)
                            .map(|n| (n, false, has_body, is_static))
                    })
                    .unwrap_or_default(),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    // Track accessors for duplicate detection (getter/setter pairs are allowed,
                    // but duplicate getters or duplicate setters are not)
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                        && let Some(name) = self.get_member_name_text(accessor.name)
                    {
                        let is_static = self.has_static_modifier(&accessor.modifiers);
                        let kind = if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                            "get"
                        } else {
                            "set"
                        };
                        let key = if is_static {
                            format!("static:{kind}:{name}")
                        } else {
                            format!("{kind}:{name}")
                        };
                        seen_accessors.entry(key).or_default().push(member_idx);

                        // Also track plain name for cross-checking with properties/methods
                        let plain_key = if is_static {
                            format!("static:{name}")
                        } else {
                            name.clone()
                        };
                        accessor_plain_names
                            .entry(plain_key)
                            .or_default()
                            .push(member_idx);
                    }
                    continue;
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    constructor_declarations.push(member_idx);
                    if let Some(constructor) = self.ctx.arena.get_constructor(member_node)
                        && constructor.body.is_some()
                    {
                        constructor_implementations.push(member_idx);
                    }
                    continue;
                }
                _ => continue,
            };

            if name.is_empty() {
                continue;
            }

            // Create a key that considers static vs instance members separately
            let key = if is_static {
                format!("static:{name}")
            } else {
                name.clone()
            };

            let info = seen_names.entry(key).or_insert(MemberInfo {
                indices: Vec::new(),
                is_property: Vec::new(),
                method_has_body: Vec::new(),
                is_static: Vec::new(),
            });
            info.indices.push(member_idx);
            info.is_property.push(is_property);
            info.method_has_body.push(method_has_body);
            info.is_static.push(is_static);
        }

        // Report errors for duplicates
        for info in seen_names.values() {
            if info.indices.len() <= 1 {
                continue;
            }

            // Count types of members
            let property_count = info.is_property.iter().filter(|&&p| p).count();
            let method_count = info.is_property.len() - property_count;
            let method_impl_count = info
                .is_property
                .iter()
                .zip(info.method_has_body.iter())
                .filter(|(is_prop, has_body)| !**is_prop && **has_body)
                .count();

            // Case 1: Multiple properties with same name (no methods) -> TS2300 for subsequent only
            // Case 2: Property mixed with methods:
            //   - If property comes first: TS2300 for ALL (both property and method)
            //   - If method comes first: TS2300 for subsequent (only property)
            // Case 3: Multiple method implementations -> TS2393 for implementations only
            // Case 4: Method overloads (signatures + 1 implementation) -> Valid, no error

            if property_count > 0 && method_count == 0 {
                // TS2717: Duplicate class property declarations with incompatible explicit types.
                // Keep this narrow to explicit type annotations to avoid inference cascades.
                let first_declared = info
                    .indices
                    .first()
                    .and_then(|&idx| self.get_class_property_declared_type_info(idx));

                if let Some((_first_name, _first_name_node, first_type)) = &first_declared
                    && !self.type_contains_error(*first_type)
                {
                    let first_type_str = self.format_type(*first_type);
                    for &idx in info.indices.iter().skip(1) {
                        let Some((_name, name_node, current_type)) =
                            self.get_class_property_declared_type_info(idx)
                        else {
                            continue;
                        };
                        if self.type_contains_error(current_type) {
                            continue;
                        }
                        // TS2717 uses type identity, not assignability.
                        if *first_type != current_type {
                            // Use display text for the message to match TSC's declarationNameToString
                            let display_name = self
                                .get_member_name_display_text(name_node)
                                .unwrap_or_else(|| _name.clone());
                            let current_type_str = self.format_type(current_type);
                            self.error_at_node_msg(
                                    name_node,
                                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                                    &[&display_name, &first_type_str, &current_type_str],
                                );
                        }
                    }
                }

                // All properties: only report subsequent declarations
                for &idx in info.indices.iter().skip(1) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if property_count > 0 && method_count > 0 {
                // Mixed properties and methods: check if first is property
                let first_is_property = info.is_property.first().copied().unwrap_or(false);
                let skip_count = usize::from(!first_is_property);

                for &idx in info.indices.iter().skip(skip_count) {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            } else if method_impl_count > 1 {
                // Multiple method implementations -> TS2393 for implementations only
                for ((&idx, &is_prop), &has_body) in info
                    .indices
                    .iter()
                    .zip(info.is_property.iter())
                    .zip(info.method_has_body.iter())
                {
                    if !is_prop && has_body {
                        let member_node = self.ctx.arena.get(idx);
                        let error_node = member_node
                            .and_then(|n| self.ctx.arena.get_method_decl(n))
                            .map(|m| m.name)
                            .filter(|idx| idx.is_some())
                            .unwrap_or(idx);
                        self.error_at_node(
                            error_node,
                            "Duplicate function implementation.",
                            diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                        );
                    }
                }
            }
            // else: Only method signatures + at most 1 implementation = valid overloads
        }

        // TS2392: multiple constructor implementations are not allowed.
        // Constructor overload signatures are valid; only declarations with bodies count.
        if constructor_implementations.len() > 1 {
            for &idx in &constructor_declarations {
                self.error_at_node(
                    idx,
                    "Multiple constructor implementations are not allowed.",
                    diagnostic_codes::MULTIPLE_CONSTRUCTOR_IMPLEMENTATIONS_ARE_NOT_ALLOWED,
                );
            }
        }

        // Report TS2300 for duplicate accessors (e.g., two getters or two setters with same name)
        // tsc only reports on subsequent (second+) declarations, not the first
        for indices in seen_accessors.values() {
            if indices.len() <= 1 {
                continue;
            }
            for &idx in indices.iter().skip(1) {
                self.report_duplicate_class_member_ts2300(idx);
            }
        }

        // Cross-check accessors against properties/methods for TS2300
        // A field+getter, field+setter, or method+getter/setter conflict is TS2300
        // tsc reports on BOTH the accessor and the conflicting property/method
        for (key, accessor_indices) in &accessor_plain_names {
            if let Some(member_info) = seen_names.get(key) {
                // Report TS2300 on the conflicting property/method declarations
                for &idx in &member_info.indices {
                    self.report_duplicate_class_member_ts2300(idx);
                }
                // Report TS2300 on the accessor declarations
                for &idx in accessor_indices {
                    self.report_duplicate_class_member_ts2300(idx);
                }
            }
        }

        // TS2717: If a property declaration comes after accessors with the same name,
        // report incompatible types (e.g., get/set infer `number`, later field is `any`).
        let mut seen_accessor_type_by_key: FxHashMap<String, TypeId> = FxHashMap::default();
        for &member_idx in members {
            if let Some((name, _name_node, accessor_type, is_static)) =
                self.get_class_accessor_type_info(member_idx)
            {
                if self.type_contains_error(accessor_type) {
                    continue;
                }
                let key = if is_static {
                    format!("static:{name}")
                } else {
                    name
                };
                seen_accessor_type_by_key
                    .entry(key)
                    .or_insert(accessor_type);
                continue;
            }

            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            let Some(name) = self.get_member_name_text(prop.name) else {
                continue;
            };
            let is_static = self.has_static_modifier(&prop.modifiers);
            let key = if is_static {
                format!("static:{}", name.clone())
            } else {
                name.clone()
            };
            let Some(&first_type) = seen_accessor_type_by_key.get(&key) else {
                continue;
            };
            if self.type_contains_error(first_type) {
                continue;
            }
            let current_type = if let Some(declared_type) =
                self.effective_class_property_declared_type(member_idx, prop)
            {
                declared_type
            } else if prop.initializer.is_some() {
                self.get_type_of_node(prop.initializer)
            } else {
                TypeId::ANY
            };
            if self.type_contains_error(current_type) {
                continue;
            }
            let is_incompatible = if first_type == TypeId::ANY || current_type == TypeId::ANY {
                first_type != current_type
            } else {
                !self.are_mutually_assignable(first_type, current_type)
            };
            if is_incompatible {
                let first_type_str = self.format_type(first_type);
                let current_type_str = self.format_type(current_type);
                self.error_at_node_msg(
                    prop.name,
                    diagnostic_codes::SUBSEQUENT_PROPERTY_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_PROPERTY_MUST_BE_OF_TYP,
                    &[&name, &first_type_str, &current_type_str],
                );
            }
        }
    }
}
