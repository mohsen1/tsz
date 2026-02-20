//! Binder declaration binding, accessors, flow graph, validation, and statistics.

use crate::lib_loader;
use crate::state::FileFeatures;
use crate::{
    ContainerKind, FlowNodeId, Symbol, SymbolArena, SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use std::fmt::Write;
use std::sync::Arc;
use tracing::{debug, warn};
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use super::state::{BinderState, ResolutionStats, ValidationError};

impl BinderState {
    // Declaration binding methods

    pub(crate) fn bind_variable_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(decl) = arena.get_variable_declaration(node) {
            let mut decl_flags = u32::from(node.flags);
            if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0
                && let Some(ext) = arena.get_extended(idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                decl_flags |= u32::from(parent_node.flags);
            }
            let is_block_scoped = (decl_flags & (node_flags::LET | node_flags::CONST)) != 0;
            if let Some(name) = Self::get_identifier_name(arena, decl.name) {
                // Determine if block-scoped (let/const) or function-scoped (var)
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };

                // Check if exported BEFORE allocating symbol
                let is_exported = Self::is_node_exported(arena, idx);

                if self.in_module_augmentation
                    && let Some(ref module_spec) = self.current_augmented_module
                {
                    self.module_augmentations
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                let sym_id = self.declare_symbol(name, flags, idx, is_exported);
                self.node_symbols.insert(decl.name.0, sym_id);
            } else {
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };
                let is_exported = Self::is_node_exported(arena, idx);

                let mut names = Vec::new();
                Self::collect_binding_identifiers(arena, decl.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(name, flags, ident_idx, is_exported);
                    }
                }
            }

            if decl.initializer.is_some() {
                self.bind_node(arena, decl.initializer);
                let flow = self.create_flow_assignment(idx);
                self.current_flow = flow;
            }
        }
    }

    pub(crate) fn bind_function_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(func) = arena.get_function(node) {
            // Track generator/async-generator features for TS2318 diagnostics
            if func.asterisk_token {
                if func.is_async {
                    self.file_features.set(FileFeatures::ASYNC_GENERATORS);
                } else {
                    self.file_features.set(FileFeatures::GENERATORS);
                }
            }
            self.bind_modifiers(arena, func.modifiers.as_ref());
            // Function declaration creates a symbol in the current scope
            if let Some(name) = Self::get_identifier_name(arena, func.name) {
                let is_exported = Self::has_export_modifier(arena, func.modifiers.as_ref());

                if self.in_module_augmentation
                    && let Some(ref module_spec) = self.current_augmented_module
                {
                    self.module_augmentations
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                self.declare_symbol(name, symbol_flags::FUNCTION, idx, is_exported);
            }

            // Enter function scope and bind body
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            // Bind type parameters
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            self.with_fresh_flow(|binder| {
                // Bind parameters
                for &param_idx in &func.parameters.nodes {
                    binder.bind_parameter(arena, param_idx);
                }

                // Hoisting: Collect var and function declarations from the function body
                // This ensures declarations are accessible throughout the function scope
                // before their actual declaration point (JavaScript hoisting behavior)
                //
                // Note: Function declarations in blocks are block-scoped in strict mode
                // and external modules. In non-strict scripts, they hoist (Annex B).
                binder.collect_hoisted_from_node(arena, func.body);
                binder.process_hoisted_functions(arena);
                binder.process_hoisted_vars(arena);

                // Bind body
                binder.bind_node(arena, func.body);
            });

            self.exit_scope(arena);
        }
    }

    #[tracing::instrument(level = "debug", skip(self, arena), fields(param_idx = idx.0))]
    pub(crate) fn bind_parameter(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx)
            && let Some(param) = arena.get_parameter(node)
        {
            self.bind_modifiers(arena, param.modifiers.as_ref());
            if let Some(name) = Self::get_identifier_name(arena, param.name) {
                tracing::debug!(param_name = %name, param_name_idx = param.name.0, "Binding parameter");
                let sym_id =
                    self.declare_symbol(name, symbol_flags::FUNCTION_SCOPED_VARIABLE, idx, false);
                self.node_symbols.insert(param.name.0, sym_id);
                tracing::debug!(param_name = %name, sym_id = sym_id.0, "Parameter bound");
            } else {
                let mut names = Vec::new();
                Self::collect_binding_identifiers(arena, param.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(
                            name,
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            ident_idx,
                            false,
                        );
                    }
                }
            }

            if param.initializer.is_some() {
                self.bind_node(arena, param.initializer);
            }
        }
    }

    /// Bind type parameters for a function/class/interface
    pub(crate) fn bind_type_parameters(
        &mut self,
        arena: &NodeArena,
        type_params: Option<&NodeList>,
    ) {
        if let Some(params) = type_params {
            for &param_idx in &params.nodes {
                if let Some(node) = arena.get(param_idx)
                    && let Some(type_param) = arena.get_type_parameter(node)
                    && let Some(name) = Self::get_identifier_name(arena, type_param.name)
                {
                    tracing::debug!(
                        type_param_name = %name,
                        "Binding type parameter"
                    );
                    self.declare_symbol(name, symbol_flags::TYPE_PARAMETER, param_idx, false);
                }
            }
        }
    }

    /// Bind an arrow function expression - creates a scope and binds the body.
    #[tracing::instrument(level = "debug", skip(self, arena, node), fields(arrow_fn_idx = idx.0))]
    pub(crate) fn bind_arrow_function(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        if let Some(func) = arena.get_function(node) {
            tracing::debug!(
                param_count = func.parameters.nodes.len(),
                "Entering arrow function"
            );
            self.bind_modifiers(arena, func.modifiers.as_ref());
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);

            // Bind type parameters (e.g., <T> in arrow functions)
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    tracing::debug!(
                        param_count = func.parameters.nodes.len(),
                        "Binding arrow function parameters"
                    );
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Hoisting: Collect var and function declarations from the function body
                    // Note: Function declarations in blocks are block-scoped in strict mode
                    // and external modules. In non-strict scripts, they hoist (Annex B).
                    binder.collect_hoisted_from_node(arena, func.body);
                    binder.process_hoisted_functions(arena);
                    binder.process_hoisted_vars(arena);

                    // Bind body (could be a block or an expression)
                    binder.bind_node(arena, func.body);
                },
                true,
            );

            self.exit_scope(arena);
        }
    }

    /// Bind a function expression - creates a scope and binds the body.
    pub(crate) fn bind_function_expression(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(func) = arena.get_function(node) {
            self.bind_modifiers(arena, func.modifiers.as_ref());
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            // Named function expressions bind their name in their own scope
            // (accessible only inside the function body, not in the parent scope)
            if let Some(name) = Self::get_identifier_name(arena, func.name) {
                self.declare_symbol(name, symbol_flags::FUNCTION, idx, false);
            }

            // Bind type parameters
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Hoisting: Collect var and function declarations from the function body
                    // Note: Function declarations in blocks are block-scoped in strict mode
                    // and external modules. In non-strict scripts, they hoist (Annex B).
                    binder.collect_hoisted_from_node(arena, func.body);
                    binder.process_hoisted_functions(arena);
                    binder.process_hoisted_vars(arena);

                    // Bind body
                    binder.bind_node(arena, func.body);
                },
                true,
            );

            self.exit_scope(arena);
        }
    }

    pub(crate) fn bind_callable_body(
        &mut self,
        arena: &NodeArena,
        parameters: &NodeList,
        body: NodeIndex,
        idx: NodeIndex,
    ) {
        self.enter_scope(ContainerKind::Function, idx);
        self.declare_arguments_symbol();

        self.with_fresh_flow(|binder| {
            for &param_idx in &parameters.nodes {
                binder.bind_parameter(arena, param_idx);
            }

            if body.is_some() {
                binder.bind_node(arena, body);
            }
        });

        self.exit_scope(arena);
    }

    pub(crate) fn bind_modifiers(&mut self, arena: &NodeArena, modifiers: Option<&NodeList>) {
        if let Some(list) = modifiers {
            for &modifier_idx in &list.nodes {
                self.bind_node(arena, modifier_idx);
            }
        }
    }

    pub(crate) fn declare_arguments_symbol(&mut self) {
        self.declare_symbol(
            "arguments",
            symbol_flags::FUNCTION_SCOPED_VARIABLE,
            NodeIndex::NONE,
            false,
        );
    }

    pub(crate) fn bind_class_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(class) = arena.get_class(node) {
            self.bind_modifiers(arena, class.modifiers.as_ref());
            if let Some(name) = Self::get_identifier_name(arena, class.name) {
                // Start with CLASS flag
                let mut flags = symbol_flags::CLASS;

                // Add ABSTRACT flag if class has 'abstract' modifier
                if Self::has_abstract_modifier(arena, class.modifiers.as_ref()) {
                    flags |= symbol_flags::ABSTRACT;
                }

                // Check if exported BEFORE allocating symbol
                let is_exported = Self::has_export_modifier(arena, class.modifiers.as_ref());

                if self.in_module_augmentation
                    && let Some(ref module_spec) = self.current_augmented_module
                {
                    self.module_augmentations
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                self.declare_symbol(name, flags, idx, is_exported);
            }

            // Enter class scope for members
            self.enter_scope(ContainerKind::Class, idx);

            for &member_idx in &class.members.nodes {
                self.bind_class_member(arena, member_idx);
            }

            self.exit_scope(arena);
        }
    }

    pub(crate) fn bind_class_expression(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        if let Some(class) = arena.get_class(node) {
            self.bind_modifiers(arena, class.modifiers.as_ref());
            self.enter_scope(ContainerKind::Class, idx);

            if let Some(name) = Self::get_identifier_name(arena, class.name) {
                let mut flags = symbol_flags::CLASS;
                if Self::has_abstract_modifier(arena, class.modifiers.as_ref()) {
                    flags |= symbol_flags::ABSTRACT;
                }
                let sym_id = self.declare_symbol(name, flags, idx, false);
                self.node_symbols.insert(class.name.0, sym_id);
            }

            for &member_idx in &class.members.nodes {
                self.bind_class_member(arena, member_idx);
            }

            self.exit_scope(arena);
        }
    }

    pub(crate) fn bind_class_member(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx) {
            match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = arena.get_method_decl(node) {
                        self.bind_modifiers(arena, method.modifiers.as_ref());
                        if let Some(name_node) = arena.get(method.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, method.name);
                        }
                        if let Some(name) = Self::get_property_name(arena, method.name) {
                            let mut flags = symbol_flags::METHOD;
                            if Self::has_abstract_modifier(arena, method.modifiers.as_ref()) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if Self::has_static_modifier(arena, method.modifiers.as_ref()) {
                                flags |= symbol_flags::STATIC;
                            }
                            if Self::has_private_modifier(arena, method.modifiers.as_ref()) {
                                flags |= symbol_flags::PRIVATE;
                            }
                            let sym_id = self.declare_symbol(&name, flags, idx, false);
                            self.node_symbols.insert(method.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &method.parameters, method.body, idx);
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = arena.get_property_decl(node) {
                        self.bind_modifiers(arena, prop.modifiers.as_ref());
                        if let Some(name_node) = arena.get(prop.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, prop.name);
                        }
                        if let Some(name) = Self::get_property_name(arena, prop.name) {
                            let mut flags = symbol_flags::PROPERTY;
                            if Self::has_abstract_modifier(arena, prop.modifiers.as_ref()) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if Self::has_static_modifier(arena, prop.modifiers.as_ref()) {
                                flags |= symbol_flags::STATIC;
                            }
                            if Self::has_private_modifier(arena, prop.modifiers.as_ref()) {
                                flags |= symbol_flags::PRIVATE;
                            }
                            let sym_id = self.declare_symbol(&name, flags, idx, false);
                            self.node_symbols.insert(prop.name.0, sym_id);
                        }

                        if prop.initializer.is_some() {
                            self.bind_node(arena, prop.initializer);
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = arena.get_accessor(node) {
                        self.bind_modifiers(arena, accessor.modifiers.as_ref());
                        if let Some(name_node) = arena.get(accessor.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, accessor.name);
                        }
                        if let Some(name) = Self::get_property_name(arena, accessor.name) {
                            let mut flags = if node.kind == syntax_kind_ext::GET_ACCESSOR {
                                symbol_flags::GET_ACCESSOR
                            } else {
                                symbol_flags::SET_ACCESSOR
                            };
                            if Self::has_abstract_modifier(arena, accessor.modifiers.as_ref()) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if Self::has_static_modifier(arena, accessor.modifiers.as_ref()) {
                                flags |= symbol_flags::STATIC;
                            }
                            if Self::has_private_modifier(arena, accessor.modifiers.as_ref()) {
                                flags |= symbol_flags::PRIVATE;
                            }
                            let sym_id = self.declare_symbol(&name, flags, idx, false);
                            self.node_symbols.insert(accessor.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &accessor.parameters, accessor.body, idx);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    self.declare_symbol("constructor", symbol_flags::CONSTRUCTOR, idx, false);
                    if let Some(ctor) = arena.get_constructor(node) {
                        self.bind_modifiers(arena, ctor.modifiers.as_ref());
                        self.bind_callable_body(arena, &ctor.parameters, ctor.body, idx);
                    }
                }
                k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                    if let Some(block) = arena.get_block(node) {
                        self.enter_scope(ContainerKind::Block, idx);
                        for &stmt_idx in &block.statements.nodes {
                            self.bind_node(arena, stmt_idx);
                        }
                        self.exit_scope(arena);
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn bind_interface_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(iface) = arena.get_interface(node)
            && let Some(name) = Self::get_identifier_name(arena, iface.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = Self::has_export_modifier(arena, iface.modifiers.as_ref());

            // If we're inside a global augmentation block, track this as an augmentation
            // that should merge with lib.d.ts symbols at type resolution time
            if self.in_global_augmentation {
                self.global_augmentations
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(idx));
            }

            // In script files (non-module files), top-level interface declarations that match
            // built-in global type names should also be treated as augmentations.
            // TypeScript allows `interface Array<T> { ... }` in scripts without `declare global`.
            if !self.in_global_augmentation
                && self.is_global_scope()
                && !self.is_external_module
                && Self::is_built_in_global_type(name)
            {
                self.global_augmentations
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(idx));
            }

            // Rule #44: Track module augmentation interfaces
            // These will be merged with the target module's interface at type resolution time
            if self.in_module_augmentation
                && let Some(ref module_spec) = self.current_augmented_module
            {
                self.module_augmentations
                    .entry(module_spec.clone())
                    .or_default()
                    .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
            }

            self.declare_symbol(name, symbol_flags::INTERFACE, idx, is_exported);
        }
    }

    pub(crate) fn bind_type_alias_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(alias) = arena.get_type_alias(node)
            && let Some(name) = Self::get_identifier_name(arena, alias.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = Self::has_export_modifier(arena, alias.modifiers.as_ref());

            // If we're inside a global augmentation block, track this as an augmentation
            // that should merge with lib.d.ts symbols at type resolution time
            if self.in_global_augmentation {
                self.global_augmentations
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(idx));
            }

            // Rule #44: Track module augmentation type aliases
            if self.in_module_augmentation
                && let Some(ref module_spec) = self.current_augmented_module
            {
                self.module_augmentations
                    .entry(module_spec.clone())
                    .or_default()
                    .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
            }

            self.declare_symbol(name, symbol_flags::TYPE_ALIAS, idx, is_exported);
        }
    }

    pub(crate) fn bind_enum_declaration(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        if let Some(enum_decl) = arena.get_enum(node)
            && let Some(name) = Self::get_identifier_name(arena, enum_decl.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = Self::has_export_modifier(arena, enum_decl.modifiers.as_ref());

            if self.in_module_augmentation
                && let Some(ref module_spec) = self.current_augmented_module
            {
                self.module_augmentations
                    .entry(module_spec.clone())
                    .or_default()
                    .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
            }

            // Check if this is a const enum
            let is_const = Self::has_const_modifier(arena, enum_decl.modifiers.as_ref());
            let enum_flags = if is_const {
                symbol_flags::CONST_ENUM
            } else {
                symbol_flags::REGULAR_ENUM
            };

            let enum_sym_id = self.declare_symbol(name, enum_flags, idx, is_exported);

            // Get existing exports (for namespace merging)
            let mut exports = SymbolTable::new();
            if let Some(enum_symbol) = self.symbols.get(enum_sym_id)
                && let Some(ref existing_exports) = enum_symbol.exports
            {
                exports = (**existing_exports).clone();
            }

            // Bind enum members and add them to exports
            // This allows enum members to be accessed as Enum.MemberName
            // and enables enum + namespace merging
            self.enter_scope(ContainerKind::Block, idx);

            // Seed the new scope with existing exports from prior declarations.
            // This allows merged enum declarations to reference members from
            // earlier declarations (e.g., `enum E { a } enum E { c = a }`).
            for (name, sym_id) in exports.iter() {
                self.current_scope.set(name.to_string(), *sym_id);
            }

            for &member_idx in &enum_decl.members.nodes {
                if let Some(member_node) = arena.get(member_idx)
                    && let Some(member) = arena.get_enum_member(member_node)
                    && let Some(member_name) = Self::get_identifier_name(arena, member.name)
                {
                    let sym_id = self
                        .symbols
                        .alloc(symbol_flags::ENUM_MEMBER, member_name.to_string());
                    // Set value_declaration for enum members so the checker can find the parent enum
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.value_declaration = member_idx;
                        sym.declarations.push(member_idx);
                        sym.parent = enum_sym_id; // Set parent to the enum symbol
                    }
                    self.current_scope.set(member_name.to_string(), sym_id);
                    self.node_symbols.insert(member_idx.0, sym_id);
                    // Add to exports for namespace merging
                    exports.set(member_name.to_string(), sym_id);

                    // Bind the initializer expression so that nested functions,
                    // IIFEs, and closures within enum member initializers get
                    // their scopes and symbols properly bound.
                    if member.initializer.is_some() {
                        self.bind_expression(arena, member.initializer);
                    }
                }
            }
            self.exit_scope(arena);

            // Update the enum's exports with members
            if let Some(enum_symbol) = self.symbols.get_mut(enum_sym_id) {
                enum_symbol.exports = Some(Box::new(exports));
            }
        }
    }

    pub(crate) fn bind_switch_statement(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        self.record_flow(idx);
        if let Some(switch_data) = arena.get_switch(node) {
            self.bind_expression(arena, switch_data.expression);

            let pre_switch_flow = self.current_flow;
            let end_label = self.create_branch_label();
            let mut fallthrough_flow = FlowNodeId::NONE;

            // Push end_label as break target so break statements in cases jump here
            self.break_targets.push(end_label);

            // Case block contains case clauses
            let mut has_default_clause = false;
            if let Some(case_block_node) = arena.get(switch_data.case_block)
                && let Some(case_block) = arena.get_block(case_block_node)
            {
                // Enter a block scope for the case block - all case clauses share this scope
                self.enter_scope(ContainerKind::Block, switch_data.case_block);

                for &clause_idx in &case_block.statements.nodes {
                    if let Some(clause_node) = arena.get(clause_idx)
                        && let Some(clause) = arena.get_case_clause(clause_node)
                    {
                        if clause.expression.is_none() {
                            has_default_clause = true;
                        }

                        self.switch_clause_to_switch.insert(clause_idx.0, idx);

                        self.current_flow = pre_switch_flow;
                        if clause.expression.is_some() {
                            self.bind_expression(arena, clause.expression);
                        }

                        let clause_flow = self.create_switch_clause_flow(
                            pre_switch_flow,
                            fallthrough_flow,
                            clause_idx,
                        );
                        self.current_flow = clause_flow;

                        for &stmt_idx in &clause.statements.nodes {
                            self.bind_node(arena, stmt_idx);
                        }

                        self.add_antecedent(end_label, self.current_flow);

                        if Self::clause_allows_fallthrough(arena, clause) {
                            fallthrough_flow = self.current_flow;
                        } else {
                            fallthrough_flow = FlowNodeId::NONE;
                        }
                    }
                }

                // Exhaustiveness: if no default clause, create an implicit default
                // path representing "no case matched". This SWITCH_CLAUSE uses the
                // case_block node as a marker so the checker can detect it and apply
                // default-clause narrowing (excluding all case values).
                if !has_default_clause {
                    let implicit_default_flow = self.create_switch_clause_flow(
                        pre_switch_flow,
                        FlowNodeId::NONE,
                        switch_data.case_block,
                    );
                    self.add_antecedent(end_label, implicit_default_flow);
                }

                // Exit the case block scope
                self.exit_scope(arena);
            }

            self.break_targets.pop();
            self.current_flow = end_label;
        }
    }

    pub(crate) fn clause_allows_fallthrough(
        arena: &NodeArena,
        clause: &tsz_parser::parser::node::CaseClauseData,
    ) -> bool {
        let Some(&last_stmt_idx) = clause.statements.nodes.last() else {
            return true;
        };

        let Some(stmt_node) = arena.get(last_stmt_idx) else {
            return true;
        };

        !matches!(
            stmt_node.kind,
            k if k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
        )
    }

    pub(crate) fn bind_try_statement(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        self.record_flow(idx);
        if let Some(try_data) = arena.get_try(node) {
            let pre_try_flow = self.current_flow;
            let end_label = self.create_branch_label();

            // Bind try block
            self.bind_node(arena, try_data.try_block);
            let post_try_flow = self.current_flow;

            // Bind catch clause
            if try_data.catch_clause.is_some()
                && let Some(catch_node) = arena.get(try_data.catch_clause)
                && let Some(catch) = arena.get_catch_clause(catch_node)
            {
                self.enter_scope(ContainerKind::Block, idx);

                // Catch can be entered from any point in try.
                self.current_flow = pre_try_flow;

                // Bind catch variable and mark it assigned.
                if catch.variable_declaration.is_some() {
                    self.bind_node(arena, catch.variable_declaration);
                    let flow = self.create_flow_assignment(catch.variable_declaration);
                    self.current_flow = flow;
                }

                // Bind catch block
                self.bind_node(arena, catch.block);
                self.add_antecedent(end_label, self.current_flow);

                self.exit_scope(arena);
            }

            // Add post-try flow to end label
            self.add_antecedent(end_label, post_try_flow);

            // Bind finally block
            if try_data.finally_block.is_none() {
                self.current_flow = end_label;
            } else {
                self.current_flow = end_label;
                self.bind_node(arena, try_data.finally_block);
            }
        }
    }

    // Public accessors

    /// Check if lib symbols have been merged into this binder's local arena.
    pub const fn lib_symbols_are_merged(&self) -> bool {
        self.lib_symbols_merged
    }

    /// Set the `lib_symbols_merged` flag.
    ///
    /// This should be called when a binder is reconstructed from a `MergedProgram`
    /// where all lib symbols have already been remapped to unique global IDs.
    pub const fn set_lib_symbols_merged(&mut self, merged: bool) {
        self.lib_symbols_merged = merged;
    }

    pub fn get_symbol(&self, id: SymbolId) -> Option<&Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
        }

        // Prefer local symbols first so source-file declarations can shadow
        // lib symbols even when SymbolId values collide.
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Legacy path (for backward compatibility when lib_symbols_merged is false):
        // If this is a lib symbol ID, check lib binders first to avoid
        // collision with local symbols at the same index
        if self.lib_symbol_ids.contains(&id) {
            for lib_binder in &self.lib_binders {
                if let Some(sym) = lib_binder.symbols.get(id) {
                    return Some(sym);
                }
            }
        }

        // Finally try lib binders for any remaining cases
        for lib_binder in &self.lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }
        None
    }

    /// Get a symbol, checking lib binders if not found locally.
    /// This is used by the checker to resolve symbols that come from lib.d.ts.
    pub fn get_symbol_with_libs<'a>(
        &'a self,
        id: SymbolId,
        lib_binders: &'a [Arc<Self>],
    ) -> Option<&'a Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
        }

        // Prefer local symbols first so source-file declarations can shadow
        // lib symbols even when SymbolId values collide.
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Legacy path (for backward compatibility when lib_symbols_merged is false):
        // Prefer lib binders when the ID is known to originate from libs
        if self.lib_symbol_ids.contains(&id) {
            for lib_binder in lib_binders {
                if let Some(sym) = lib_binder.symbols.get(id) {
                    return Some(sym);
                }
            }
        }

        // Then try lib binders
        for lib_binder in lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }

        None
    }

    /// Look up a global type by name from `file_locals` and lib binders.
    ///
    /// This method is used by the checker to find built-in types like Array, Object,
    /// Function, Promise, etc. It checks:
    /// 1. Local `file_locals` (for user-defined globals or merged lib symbols)
    /// 2. Lib binders (only when `lib_symbols_merged` is false)
    ///
    /// Returns the `SymbolId` if found, None otherwise.
    pub fn get_global_type(&self, name: &str) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check lib binders directly (for backward compatibility)
        for lib_binder in &self.lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Look up a global type by name, using provided lib binders.
    ///
    /// This variant is used when the checker has its own lib contexts and needs
    /// to search them explicitly.
    pub fn get_global_type_with_libs(
        &self,
        name: &str,
        lib_binders: &[Arc<Self>],
    ) -> Option<SymbolId> {
        // First check file_locals (includes merged lib symbols when lib_symbols_merged is true)
        if let Some(sym_id) = self.file_locals.get(name) {
            return Some(sym_id);
        }

        // Fast path: If lib symbols are merged, they're all in file_locals already
        if self.lib_symbols_merged {
            return None;
        }

        // Legacy path: check provided lib binders (for backward compatibility)
        for lib_binder in lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        // Finally check our own lib binders
        for lib_binder in &self.lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    /// Check if a global type exists (in `file_locals` or lib binders).
    ///
    /// This is a convenience method for checking type availability without
    /// actually retrieving the symbol.
    pub fn has_global_type(&self, name: &str) -> bool {
        self.get_global_type(name).is_some()
    }

    pub fn get_node_symbol(&self, node: NodeIndex) -> Option<SymbolId> {
        self.node_symbols.get(&node.0).copied()
    }

    pub const fn get_symbols(&self) -> &SymbolArena {
        &self.symbols
    }

    /// Check if the current source file is an external module (has top-level import/export).
    /// This is used by the checker to determine if ES module semantics apply.
    pub const fn is_external_module(&self) -> bool {
        self.is_external_module
    }

    /// Check if a module specifier likely refers to an existing module that can be augmented.
    /// Rule #44: Module augmentation vs ambient module declaration detection.
    ///
    /// Returns true if:
    /// - The module specifier refers to an already declared module
    /// - The specifier looks like an external package (not a relative path)
    pub(crate) fn is_potential_module_augmentation(&self, module_specifier: &str) -> bool {
        // In external modules, relative `declare module "./x"` is always an augmentation target.
        if module_specifier.starts_with("./")
            || module_specifier.starts_with("../")
            || module_specifier == "."
            || module_specifier == ".."
        {
            return true;
        }

        // Check if we've already declared this module
        if self.declared_modules.contains(module_specifier) {
            return true;
        }

        // Check if we have exports from this module (meaning it was resolved)
        if self.module_exports.contains_key(module_specifier) {
            return true;
        }

        // External packages (not relative paths) are assumed to exist and can be augmented
        // This handles cases like `declare module 'express' { ... }`
        !module_specifier.starts_with('.') && !module_specifier.starts_with('/')
    }

    /// Get the flow node that was active at a given AST node.
    /// Used by the checker for control flow analysis.
    pub fn get_node_flow(&self, node: NodeIndex) -> Option<FlowNodeId> {
        self.node_flow.get(&node.0).copied()
    }

    /// Get the containing switch statement for a case/default clause.
    pub fn get_switch_for_clause(&self, clause: NodeIndex) -> Option<NodeIndex> {
        self.switch_clause_to_switch.get(&clause.0).copied()
    }

    /// Record the current flow node for an AST node.
    /// Called during binding to track flow position for identifiers and other expressions.
    pub(crate) fn record_flow(&mut self, node: NodeIndex) {
        if self.current_flow.is_some() {
            use tracing::trace;
            if let Some(flow_node) = self.flow_nodes.get(self.current_flow) {
                trace!(
                    node_idx = node.0,
                    flow_id = self.current_flow.0,
                    flow_flags = flow_node.flags,
                    "record_flow: associating node with flow"
                );
            }
            self.node_flow.insert(node.0, self.current_flow);
        }
    }

    pub(crate) fn with_fresh_flow<F>(&mut self, bind_body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.with_fresh_flow_inner(bind_body, false);
    }

    /// Create a fresh flow for a function body, optionally capturing the enclosing flow for closures.
    /// If `capture_enclosing` is true, the START node will point to the enclosing flow, allowing
    /// const/let variables to preserve narrowing from the outer scope.
    pub(crate) fn with_fresh_flow_inner<F>(&mut self, bind_body: F, capture_enclosing: bool)
    where
        F: FnOnce(&mut Self),
    {
        let prev_flow = self.current_flow;
        let start_flow = self.flow_nodes.alloc(flow_flags::START);

        // For closures (arrow functions and function expressions), capture the enclosing flow
        // so that const/let variables can preserve narrowing from the outer scope
        if capture_enclosing
            && prev_flow.is_some()
            && let Some(start_node) = self.flow_nodes.get_mut(start_flow)
        {
            start_node.antecedent.push(prev_flow);
        }

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
    }

    // =========================================================================
    // Expression binding for flow analysis
    // =========================================================================

    pub(crate) const fn is_assignment_operator(operator: u16) -> bool {
        matches!(
            operator,
            k if k == SyntaxKind::EqualsToken as u16
                || k == SyntaxKind::PlusEqualsToken as u16
                || k == SyntaxKind::MinusEqualsToken as u16
                || k == SyntaxKind::AsteriskEqualsToken as u16
                || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
                || k == SyntaxKind::SlashEqualsToken as u16
                || k == SyntaxKind::PercentEqualsToken as u16
                || k == SyntaxKind::LessThanLessThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
                || k == SyntaxKind::AmpersandEqualsToken as u16
                || k == SyntaxKind::BarEqualsToken as u16
                || k == SyntaxKind::BarBarEqualsToken as u16
                || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                || k == SyntaxKind::QuestionQuestionEqualsToken as u16
                || k == SyntaxKind::CaretEqualsToken as u16
        )
    }

    pub(crate) fn is_array_mutation_call(arena: &NodeArena, call_idx: NodeIndex) -> bool {
        let Some(call) = arena.get_call_expr_at(call_idx) else {
            return false;
        };
        let Some(access) = arena.get_access_expr_at(call.expression) else {
            return false;
        };
        if access.question_dot_token {
            return false;
        }
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return false;
        };
        let name = if let Some(ident) = arena.get_identifier(name_node) {
            ident.escaped_text.as_str()
        } else if let Some(literal) = arena.get_literal(name_node) {
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                literal.text.as_str()
            } else {
                return false;
            }
        } else {
            return false;
        };

        matches!(
            name,
            "copyWithin"
                | "fill"
                | "pop"
                | "push"
                | "reverse"
                | "shift"
                | "sort"
                | "splice"
                | "unshift"
        )
    }

    // Avoid deep recursion on large left-associative binary expression chains.
    /// Bind a short-circuit binary expression (&&, ||, ??) with intermediate
    /// flow condition nodes.
    ///
    /// For `a && b`: the right operand `b` is only evaluated when `a` is truthy,
    /// so we create a `TRUE_CONDITION` node for `a` before binding `b`. This allows
    /// references in `b` to see type narrowing from `a`.
    ///
    /// For `a || b` and `a ?? b`: the right operand `b` is only evaluated when `a`
    /// is falsy/nullish, so we create a `FALSE_CONDITION` node for `a` before binding `b`.
    pub(crate) fn bind_short_circuit_expression(
        &mut self,
        arena: &NodeArena,
        idx: NodeIndex,
        left: NodeIndex,
        right: NodeIndex,
        operator: u16,
    ) {
        self.record_flow(idx);

        // Bind the left operand
        self.bind_expression(arena, left);
        let after_left_flow = self.current_flow;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16 {
            // For &&: right side is only evaluated when left is truthy
            let true_condition =
                self.create_flow_condition(flow_flags::TRUE_CONDITION, after_left_flow, left);
            self.current_flow = true_condition;
            self.bind_expression(arena, right);
            let after_right_flow = self.current_flow;

            // Short-circuit path: left is falsy, right is not evaluated
            let false_condition =
                self.create_flow_condition(flow_flags::FALSE_CONDITION, after_left_flow, left);

            // Merge both paths
            let merge = self.create_branch_label();
            self.add_antecedent(merge, after_right_flow);
            self.add_antecedent(merge, false_condition);
            self.current_flow = merge;
        } else {
            // For || and ??: right side is only evaluated when left is falsy/nullish
            let false_condition =
                self.create_flow_condition(flow_flags::FALSE_CONDITION, after_left_flow, left);
            self.current_flow = false_condition;
            self.bind_expression(arena, right);
            let after_right_flow = self.current_flow;

            // Short-circuit path: left is truthy, right is not evaluated
            let true_condition =
                self.create_flow_condition(flow_flags::TRUE_CONDITION, after_left_flow, left);

            // Merge both paths
            let merge = self.create_branch_label();
            self.add_antecedent(merge, after_right_flow);
            self.add_antecedent(merge, true_condition);
            self.current_flow = merge;
        }
    }

    pub(crate) fn bind_binary_expression_flow_iterative(
        &mut self,
        arena: &NodeArena,
        root: NodeIndex,
    ) {
        enum WorkItem {
            Visit(NodeIndex),
            PostAssign(NodeIndex),
        }

        let mut stack = vec![WorkItem::Visit(root)];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Visit(idx) => {
                    let Some(node) = arena.get(idx) else {
                        continue;
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        self.record_flow(idx);
                        if let Some(bin) = arena.get_binary_expr(node) {
                            if Self::is_assignment_operator(bin.operator_token) {
                                stack.push(WorkItem::PostAssign(idx));
                                if bin.right.is_some() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if bin.left.is_some() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                                continue;
                            }
                            // Delegate short-circuit operators to proper flow handling
                            if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                                || bin.operator_token == SyntaxKind::BarBarToken as u16
                                || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                            {
                                self.bind_short_circuit_expression(
                                    arena,
                                    idx,
                                    bin.left,
                                    bin.right,
                                    bin.operator_token,
                                );
                                continue;
                            }
                            if bin.right.is_some() {
                                stack.push(WorkItem::Visit(bin.right));
                            }
                            if bin.left.is_some() {
                                stack.push(WorkItem::Visit(bin.left));
                            }
                        }
                        continue;
                    }

                    self.bind_expression(arena, idx);
                }
                WorkItem::PostAssign(idx) => {
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                }
            }
        }
    }

    /// Bind an expression and record flow positions for identifiers.
    /// This is used for condition expressions in if/while/for statements.
    pub(crate) fn bind_expression(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = arena.get_binary_expr(node) {
                if Self::is_assignment_operator(bin.operator_token) {
                    self.record_flow(idx);
                    self.bind_expression(arena, bin.left);
                    self.bind_expression(arena, bin.right);
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                    // Detect expando property assignments (X.prop = value)
                    if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                        self.detect_expando_assignment(arena, bin.left);
                    }
                    return;
                }

                // Handle short-circuit operators (&&, ||, ??) with intermediate
                // flow condition nodes so that the right operand sees narrowing
                // from the left operand.
                if bin.operator_token == SyntaxKind::AmpersandAmpersandToken as u16
                    || bin.operator_token == SyntaxKind::BarBarToken as u16
                    || bin.operator_token == SyntaxKind::QuestionQuestionToken as u16
                {
                    self.bind_short_circuit_expression(
                        arena,
                        idx,
                        bin.left,
                        bin.right,
                        bin.operator_token,
                    );
                    return;
                }
            }
            self.bind_binary_expression_flow_iterative(arena, idx);
            return;
        }

        // Record flow position for this node
        self.record_flow(idx);

        match node.kind {
            // Identifiers - record flow position for type narrowing
            k if k == SyntaxKind::Identifier as u16 => {
                // Already recorded above
                return;
            }

            // Prefix unary (e.g., typeof x, !x)
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = arena.get_unary_expr(node) {
                    self.bind_expression(arena, unary.operand);
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Property access (e.g., x.foo) or element access (e.g., x[0])
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);
                    // For element access, also bind the argument
                    if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                        self.bind_expression(arena, access.name_or_argument);
                    }
                }
                return;
            }

            // Call expression (e.g., isString(x))
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    self.bind_expression(arena, call.expression);
                    if let Some(args) = &call.arguments {
                        for &arg in &args.nodes {
                            self.bind_expression(arena, arg);
                        }
                    }
                    // Create CALL flow node for all call expressions
                    let flow = self.create_flow_call(idx);
                    self.current_flow = flow;
                    // Also create ARRAY_MUTATION flow node if it's an array mutation
                    if Self::is_array_mutation_call(arena, idx) {
                        let flow = self.create_flow_array_mutation(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Parenthesized expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = arena.get_parenthesized(node) {
                    self.bind_expression(arena, paren.expression);
                }
                return;
            }

            // Type assertion (e.g., x as string, <T>x, x satisfies T)
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if let Some(assertion) = arena.get_type_assertion(node) {
                    self.bind_expression(arena, assertion.expression);
                }
                return;
            }

            // Conditional expression (ternary) - build flow graph for type narrowing
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = arena.get_conditional_expr(node) {
                    // Bind the condition expression
                    self.bind_expression(arena, cond.condition);

                    // Save pre-condition flow
                    let pre_condition_flow = self.current_flow;

                    // Create TRUE_CONDITION flow for when_true branch
                    let true_flow = self.create_flow_condition(
                        flow_flags::TRUE_CONDITION,
                        pre_condition_flow,
                        cond.condition,
                    );
                    self.current_flow = true_flow;
                    self.bind_expression(arena, cond.when_true);
                    let after_true_flow = self.current_flow;

                    // Create FALSE_CONDITION flow for when_false branch
                    let false_flow = self.create_flow_condition(
                        flow_flags::FALSE_CONDITION,
                        pre_condition_flow,
                        cond.condition,
                    );
                    self.current_flow = false_flow;
                    self.bind_expression(arena, cond.when_false);
                    let after_false_flow = self.current_flow;

                    // Create merge point for both branches
                    let merge_label = self.create_branch_label();
                    self.add_antecedent(merge_label, after_true_flow);
                    self.add_antecedent(merge_label, after_false_flow);
                    self.current_flow = merge_label;
                }
                return;
            }

            _ => {}
        }

        self.bind_node(arena, idx);
    }

    /// Detect expando property assignments of the form `X.prop = value`.
    /// Only tracks single-level property accesses where `X` is a simple identifier
    /// bound to a function, class, or variable in `file_locals`.
    fn detect_expando_assignment(&mut self, arena: &NodeArena, lhs: NodeIndex) {
        let Some(lhs_node) = arena.get(lhs) else {
            return;
        };
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        let Some(access) = arena.get_access_expr(lhs_node) else {
            return;
        };
        // Get the property name from the name_or_argument (must be an identifier)
        let Some(name_node) = arena.get(access.name_or_argument) else {
            return;
        };
        let Some(name_ident) = arena.get_identifier(name_node) else {
            return;
        };
        let prop_name = name_ident.escaped_text.clone();

        // Check that the object expression is a simple identifier (single-level only)
        let Some(obj_node) = arena.get(access.expression) else {
            return;
        };
        if obj_node.kind != SyntaxKind::Identifier as u16 {
            return;
        }
        let Some(obj_ident) = arena.get_identifier(obj_node) else {
            return;
        };
        let obj_name = &obj_ident.escaped_text;

        // Look up the identifier in file_locals (covers hoisted vars/functions)
        let Some(sym_id) = self.file_locals.get(obj_name) else {
            return;
        };
        let Some(symbol) = self.symbols.get(sym_id) else {
            return;
        };

        // Track for functions and classes
        if (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0 {
            self.expando_properties
                .entry(obj_name.clone())
                .or_default()
                .insert(prop_name);
            return;
        }

        // Also track for variables initialized with function/class/object-literal expressions
        // (e.g. `var X = function(){}; X.prop = 1` or `var X = {}; X.prop = 1`)
        // but NOT variables with explicit type annotations (e.g. `let p: Person`)
        if (symbol.flags & symbol_flags::VARIABLE) != 0 {
            let decl_idx = symbol.value_declaration;
            if decl_idx.is_none() {
                return;
            }
            let Some(decl_node) = arena.get(decl_idx) else {
                return;
            };
            let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
                return;
            };
            // Skip if has explicit type annotation — expando doesn't apply
            if var_decl.type_annotation.is_some() {
                return;
            }
            if var_decl.initializer.is_none() {
                return;
            }
            let Some(init_node) = arena.get(var_decl.initializer) else {
                return;
            };
            if init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                self.expando_properties
                    .entry(obj_name.clone())
                    .or_default()
                    .insert(prop_name);
            }
        }
    }

    /// Run post-binding validation checks on the symbol table.
    /// Returns a list of validation errors found.
    pub fn validate_symbol_table(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (&node_idx, &sym_id) in &self.node_symbols {
            if self.symbols.get(sym_id).is_none() {
                errors.push(ValidationError::BrokenSymbolLink {
                    node_index: node_idx,
                    symbol_id: sym_id.0,
                });
            }
        }

        for sym in self.symbols.iter() {
            if sym.declarations.is_empty() {
                errors.push(ValidationError::OrphanedSymbol {
                    symbol_id: sym.id.0,
                    name: sym.escaped_name.clone(),
                });
            }
        }

        for sym in self.symbols.iter() {
            if sym.value_declaration.is_some() {
                let has_node_mapping = self.node_symbols.contains_key(&sym.value_declaration.0);
                if !has_node_mapping {
                    errors.push(ValidationError::InvalidValueDeclaration {
                        symbol_id: sym.id.0,
                        name: sym.escaped_name.clone(),
                    });
                }
            }
        }

        errors
    }

    /// Check if the symbol table has any validation errors.
    pub fn is_symbol_table_valid(&self) -> bool {
        self.validate_symbol_table().is_empty()
    }

    // ========================================================================
    // Lib Symbol Validation (P0 Task - Improve Test Runner Lib Injection)
    // ========================================================================

    /// Expected global symbols that should be available from lib.d.ts.
    /// These are core ECMAScript globals that should always be present.
    const EXPECTED_GLOBAL_SYMBOLS: &'static [&'static str] = &[
        // Core types
        "Object",
        "Function",
        "Array",
        "String",
        "Number",
        "Boolean",
        "Symbol",
        "BigInt",
        // Error types
        "Error",
        "EvalError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "TypeError",
        "URIError",
        // Collections
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
        // Promises and async
        "Promise",
        // Object reflection
        "Reflect",
        "Proxy",
        // Global functions
        "eval",
        "isNaN",
        "isFinite",
        "parseFloat",
        "parseInt",
        // Global values
        "Infinity",
        "NaN",
        "undefined",
        // Console (if DOM lib is loaded)
        "console",
    ];

    /// Validate that expected global symbols are present after binding.
    ///
    /// This method should be called after `bind_source_file_with_libs` to ensure
    /// that lib symbols were properly loaded and merged into the binder.
    ///
    /// Returns a list of missing symbol names. Empty list means all expected symbols are present.
    ///
    /// # Example
    /// ```ignore
    /// binder.bind_source_file_with_libs(arena, root, &lib_files);
    /// let missing = binder.validate_global_symbols();
    /// if !missing.is_empty() {
    ///     eprintln!("WARNING: Missing global symbols: {:?}", missing);
    /// }
    /// ```
    pub fn validate_global_symbols(&self) -> Vec<String> {
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            // Check if the symbol is available via resolve_identifier
            // (which checks both file_locals and lib_binders)
            let is_available = self.file_locals.has(symbol_name)
                || self
                    .lib_binders
                    .iter()
                    .any(|b| b.file_locals.has(symbol_name));

            if !is_available {
                missing.push(symbol_name.to_string());
            }
        }

        missing
    }

    /// Get a detailed report of lib symbol availability.
    ///
    /// Returns a human-readable string showing:
    /// - Which expected symbols are present
    /// - Which expected symbols are missing
    /// - Total symbol count from `file_locals` and `lib_binders`
    pub fn get_lib_symbol_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Lib Symbol Availability Report ===\n\n");

        // Count total symbols
        let file_local_count = self.file_locals.len();
        let lib_binder_count: usize = self.lib_binders.iter().map(|b| b.file_locals.len()).sum();

        let _ = writeln!(report, "File locals: {file_local_count} symbols");
        let _ = writeln!(
            report,
            "Lib binders: {} symbols ({} binders)",
            lib_binder_count,
            self.lib_binders.len()
        );
        report.push('\n');

        // Check each expected symbol
        let mut present = Vec::new();
        let mut missing = Vec::new();

        for &symbol_name in Self::EXPECTED_GLOBAL_SYMBOLS {
            let is_available = self.file_locals.has(symbol_name)
                || self
                    .lib_binders
                    .iter()
                    .any(|b| b.file_locals.has(symbol_name));

            if is_available {
                present.push(symbol_name);
            } else {
                missing.push(symbol_name);
            }
        }

        let _ = writeln!(
            report,
            "Expected symbols present: {}/{}",
            present.len(),
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        );
        if !missing.is_empty() {
            report.push_str("\nMissing symbols:\n");
            for name in &missing {
                let _ = writeln!(report, "  - {name}");
            }
        }

        // Show which lib binders contribute symbols
        if !self.lib_binders.is_empty() {
            report.push_str("\nLib binder contributions:\n");
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                let _ = writeln!(
                    report,
                    "  Lib binder {}: {} symbols",
                    i,
                    lib_binder.file_locals.len()
                );
            }
        }

        report
    }

    /// Log missing lib symbols with debug context.
    ///
    /// This should be called at test start to warn about missing lib symbols
    /// that might cause test failures.
    ///
    /// Returns true if any expected symbols are missing.
    pub fn log_missing_lib_symbols(&self) -> bool {
        let missing = self.validate_global_symbols();

        if missing.is_empty() {
            debug!(
                "[LIB_SYMBOL_INFO] All {} expected global symbols are present.",
                Self::EXPECTED_GLOBAL_SYMBOLS.len()
            );
            false
        } else {
            warn!(
                "[LIB_SYMBOL_WARNING] Missing {} expected global symbols: {:?}",
                missing.len(),
                missing
            );
            warn!("[LIB_SYMBOL_WARNING] This may cause test failures due to unresolved symbols.");
            warn!(
                "[LIB_SYMBOL_WARNING] Ensure lib.d.ts is loaded via addLibFile() before binding."
            );
            true
        }
    }

    /// Verify that lib symbols from multiple test files are properly merged.
    ///
    /// This method checks that symbols from multiple lib files are all accessible
    /// through the binder's symbol resolution chain.
    ///
    /// # Arguments
    /// * `lib_files` - The lib files that were supposed to be merged
    ///
    /// Returns a list of lib file names whose symbols are not fully accessible.
    pub fn verify_lib_symbol_merge(&self, lib_files: &[Arc<lib_loader::LibFile>]) -> Vec<String> {
        let mut inaccessible = Vec::new();

        for lib_file in lib_files {
            let file_name = lib_file.file_name.clone();

            // Check if symbols from this lib file are accessible
            let mut has_accessible_symbols = false;
            for (name, &_sym_id) in lib_file.binder.file_locals.iter() {
                // Try to resolve the symbol through our binder
                if self.file_locals.get(name).is_some()
                    || self
                        .lib_binders
                        .iter()
                        .any(|b| b.file_locals.get(name).is_some())
                {
                    has_accessible_symbols = true;
                    break;
                }
            }

            if !has_accessible_symbols && !lib_file.binder.file_locals.is_empty() {
                inaccessible.push(file_name);
            }
        }

        inaccessible
    }

    // ========================================================================
    // Symbol Resolution Statistics (P1 Task - Debug Logging)
    // ========================================================================

    /// Get a snapshot of current symbol resolution statistics.
    ///
    /// This method scans the binder state to provide statistics about
    /// symbol resolution capability, including:
    /// - Available symbols by source (scopes, `file_locals`, `lib_binders`)
    /// - Potential resolution paths
    pub fn get_resolution_stats(&self) -> ResolutionStats {
        // Count symbols in each resolution tier
        let scope_symbols: u64 = self.scopes.iter().map(|s| s.table.len() as u64).sum();

        let file_local_symbols = self.file_locals.len() as u64;

        let lib_binder_symbols: u64 = self
            .lib_binders
            .iter()
            .map(|b| b.file_locals.len() as u64)
            .sum();

        ResolutionStats {
            attempts: 0, // Would need runtime tracking
            scope_hits: scope_symbols,
            file_local_hits: file_local_symbols,
            lib_binder_hits: lib_binder_symbols,
            failures: 0, // Would need runtime tracking
        }
    }

    /// Get a human-readable summary of resolution statistics.
    pub fn get_resolution_summary(&self) -> String {
        let stats = self.get_resolution_stats();
        format!(
            "Symbol Resolution Summary:\n\
             - Scope symbols: {}\n\
             - File local symbols: {}\n\
             - Lib binder symbols: {} (from {} binders)\n\
             - Total accessible symbols: {}\n\
             - Expected global symbols: {}",
            stats.scope_hits,
            stats.file_local_hits,
            stats.lib_binder_hits,
            self.lib_binders.len(),
            stats.scope_hits + stats.file_local_hits + stats.lib_binder_hits,
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        )
    }

    /// Check if the current scope is the global (file-level) scope.
    fn is_global_scope(&self) -> bool {
        // Global scope is ScopeId(0) in script files
        self.current_scope_id == crate::ScopeId(0)
    }

    /// Check if a type name is a built-in global type that can be augmented.
    ///
    /// These are types from lib.d.ts that TypeScript allows augmenting through
    /// top-level interface declarations in script files (without `declare global`).
    fn is_built_in_global_type(name: &str) -> bool {
        matches!(
            name,
            "Array"
                | "ReadonlyArray"
                | "Promise"
                | "PromiseLike"
                | "Map"
                | "ReadonlyMap"
                | "WeakMap"
                | "Set"
                | "ReadonlySet"
                | "WeakSet"
                | "ArrayConstructor"
                | "MapConstructor"
                | "SetConstructor"
                | "WeakMapConstructor"
                | "WeakSetConstructor"
                | "PromiseConstructor"
                | "ProxyHandler"
                | "ProxyConstructor"
                | "Reflect"
                | "Generator"
                | "GeneratorFunction"
                | "AsyncGenerator"
                | "AsyncGeneratorFunction"
                | "AsyncIterable"
                | "AsyncIterableIterator"
                | "AsyncIterator"
                | "Iterable"
                | "Iterator"
                | "IterableIterator"
                | "Symbol"
                | "SymbolConstructor"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Uint16Array"
                | "Uint32Array"
                | "Int8Array"
                | "Int16Array"
                | "Int32Array"
                | "Float32Array"
                | "Float64Array"
                | "ArrayBuffer"
                | "SharedArrayBuffer"
                | "DataView"
                | "RegExp"
                | "RegExpConstructor"
                | "Date"
                | "DateConstructor"
                | "Error"
                | "ErrorConstructor"
                | "EvalError"
                | "RangeError"
                | "ReferenceError"
                | "SyntaxError"
                | "TypeError"
                | "URIError"
                | "Boolean"
                | "Number"
                | "String"
                | "Object"
                | "ObjectConstructor"
                | "Function"
                | "IArguments"
                | "JSON"
                | "Math"
                | "Console"
        )
    }
}

impl Default for BinderState {
    fn default() -> Self {
        Self::new()
    }
}
