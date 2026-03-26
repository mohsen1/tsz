//! Class member access-resolution and property inference helpers.

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
        self.summarize_enclosing_class_initialization()
            .is_some_and(|summary| summary.constructor_assigned_fields.contains(&key))
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
                    if matches!(rhs_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
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

    fn collect_declared_names_in_subtree(
        &self,
        node_idx: NodeIndex,
        names: &mut rustc_hash::FxHashSet<String>,
    ) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(decl) = self.ctx.arena.get_variable_declaration(node)
            && let Some(name) = self.get_node_text(decl.name)
        {
            names.insert(name);
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_declared_names_in_subtree(child_idx, names);
        }
    }

    fn enclosing_class_constructor_declared_names(&self) -> rustc_hash::FxHashSet<String> {
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

            if ctor.body.is_some() {
                self.collect_declared_names_in_subtree(ctor.body, &mut names);
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

    fn symbol_is_parameter_property_of_current_class(&self, sym_id: tsz_binder::SymbolId) -> bool {
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
            let Some(param) = self.ctx.arena.get_parameter(decl_node) else {
                return false;
            };
            if !self.has_parameter_property_modifier(&param.modifiers) {
                return false;
            }

            self.enclosing_constructor_of_node(decl_idx)
                .is_some_and(|ctor_idx| class_info.member_nodes.contains(&ctor_idx))
        })
    }

    fn current_class_has_parameter_property_named(&self, name: &str) -> bool {
        let Some(class_info) = self.ctx.enclosing_class.as_ref() else {
            return false;
        };

        class_info.member_nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                return false;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                return false;
            };

            ctor.parameters.nodes.iter().any(|&param_idx| {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    return false;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    return false;
                };
                self.has_parameter_property_modifier(&param.modifiers)
                    && self.get_node_text(param.name).as_deref() == Some(name)
            })
        })
    }

    fn has_cross_file_script_value_named(&self, name: &str) -> bool {
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        {
            for &(file_idx, sym_id) in entries {
                if file_idx == self.ctx.current_file_idx {
                    continue;
                }
                let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                    continue;
                };
                if binder.is_external_module() {
                    continue;
                }
                if let Some(symbol) = binder
                    .get_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol(sym_id))
                    && (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0
                {
                    return true;
                }
            }
            return false;
        }

        self.ctx
            .all_binders
            .as_ref()
            .is_some_and(|all_binders| {
                all_binders.iter().enumerate().any(|(file_idx, binder)| {
                    if file_idx == self.ctx.current_file_idx || binder.is_external_module() {
                        return false;
                    }
                    binder
                        .file_locals
                        .get(name)
                        .and_then(|sym_id| {
                            binder
                                .get_symbol(sym_id)
                                .or_else(|| self.ctx.binder.get_symbol(sym_id))
                        })
                        .is_some_and(|symbol| {
                            (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0
                        })
                })
            })
    }

    fn enclosing_instance_property_initializer_of_node(
        &self,
        node_idx: NodeIndex,
    ) -> Option<(String, NodeIndex)> {
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
            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                let prop = self.ctx.arena.get_property_decl(parent_node)?;
                if self.has_static_modifier(&prop.modifiers) || prop.initializer.is_none() {
                    return None;
                }
                let member_name = self.get_property_name(prop.name)?;
                return Some((member_name, prop.initializer));
            }
            current = parent_idx;
        }
        None
    }

    pub(crate) fn should_suppress_unresolved_name_for_constructor_capture(
        &self,
        name: &str,
        ident_idx: NodeIndex,
    ) -> bool {
        let Some((_member_name, _initializer_idx)) =
            self.enclosing_instance_property_initializer_of_node(ident_idx)
        else {
            return false;
        };

        let ctor_declared_names = self.enclosing_class_constructor_declared_names();
        if !ctor_declared_names.contains(name) {
            return false;
        }

        !(self.current_class_has_parameter_property_named(name)
            && self.ctx.binder.is_external_module())
    }

    /// Check if an identifier is inside `export default expr` within a namespace.
    /// When TS1319 is the correct diagnostic, name resolution (TS2304/TS2552)
    /// produces false positives that should be suppressed.
    pub(crate) fn should_suppress_name_in_export_default_namespace(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut cur = idx;
        for _ in 0..8 {
            let Some(n) = self.ctx.arena.get(cur) else {
                break;
            };
            // Heritage clauses should always emit TS2304 — stop.
            if n.kind == syntax_kind_ext::HERITAGE_CLAUSE {
                break;
            }
            let is_export_default = if n.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                !self
                    .ctx
                    .arena
                    .get_export_assignment(n)
                    .is_some_and(|data| data.is_export_equals)
            } else if n.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.ctx
                    .arena
                    .get_export_decl_at(cur)
                    .is_some_and(|data| data.is_default_export)
            } else {
                false
            };
            if is_export_default {
                let mut ns = cur;
                for _ in 0..8 {
                    if let Some(nn) = self.ctx.arena.get(ns)
                        && nn.kind == syntax_kind_ext::MODULE_DECLARATION
                    {
                        return true;
                    }
                    match self.ctx.arena.get_extended(ns) {
                        Some(e) if e.parent.is_some() => ns = e.parent,
                        _ => break,
                    }
                }
                break;
            }
            match self.ctx.arena.get_extended(cur) {
                Some(e) if e.parent.is_some() => cur = e.parent,
                _ => break,
            }
        }
        false
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
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                for child_idx in self.ctx.arena.get_children(node_idx) {
                    self.collect_unqualified_identifier_references(child_idx, refs);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    self.collect_unqualified_identifier_references(prop.initializer, refs);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_shorthand_property(node) {
                    refs.push((self.get_node_text(prop.name).unwrap_or_default(), prop.name));
                    if prop.object_assignment_initializer.is_some() {
                        self.collect_unqualified_identifier_references(
                            prop.object_assignment_initializer,
                            refs,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                for child_idx in self.ctx.arena.get_children(node_idx) {
                    self.collect_unqualified_identifier_references(child_idx, refs);
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

        let ctor_declared_names = self.enclosing_class_constructor_declared_names();
        if ctor_declared_names.is_empty() {
            return;
        }

        let mut refs = Vec::new();
        self.collect_unqualified_identifier_references(initializer_idx, &mut refs);

        for (name, ident_idx) in refs {
            if !ctor_declared_names.contains(&name) {
                continue;
            }

            if let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, ident_idx)
            {
                if self.symbol_is_constructor_parameter_of_current_class(sym_id) {
                    let should_prefer_instance_member = self
                        .symbol_is_parameter_property_of_current_class(sym_id)
                        && self.ctx.binder.is_external_module()
                        && !self.has_cross_file_script_value_named(&name);
                    let error_code = if should_prefer_instance_member {
                        diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
                    } else {
                        diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN
                    };
                    let args: &[&str] = if error_code
                        == diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN
                    {
                        &[member_name, &name]
                    } else {
                        &[&name]
                    };
                    self.error_at_node_msg(ident_idx, error_code, args);
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
                let should_prefer_instance_member = self
                    .current_class_has_parameter_property_named(&name)
                    && self.ctx.binder.is_external_module()
                    && !self.has_cross_file_script_value_named(&name);
                let error_code = if should_prefer_instance_member {
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS
                } else {
                    diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN
                };
                let args: &[&str] = if error_code
                    == diagnostic_codes::INITIALIZER_OF_INSTANCE_MEMBER_VARIABLE_CANNOT_REFERENCE_IDENTIFIER_DECLARED_IN
                {
                    &[member_name, &name]
                } else {
                    &[&name]
                };
                self.error_at_node_msg(ident_idx, error_code, args);
            }
        }
    }
}
