//! Binder declaration binding, accessors, flow graph, validation, and statistics.
//!
//! This file contains the second half of the `impl BinderState` block,
//! split from `state.rs` for maintainability.

use crate::binder::{
    ContainerKind, FlowNodeId, Symbol, SymbolArena, SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use crate::lib_loader;
use crate::parser::node::{Node, NodeArena};
use crate::parser::node_flags;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use std::sync::Arc;
use tracing::{debug, warn};

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
            let mut decl_flags = node.flags as u32;
            if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0
                && let Some(ext) = arena.get_extended(idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                decl_flags |= parent_node.flags as u32;
            }
            let is_block_scoped = (decl_flags & (node_flags::LET | node_flags::CONST)) != 0;
            if let Some(name) = self.get_identifier_name(arena, decl.name) {
                // Determine if block-scoped (let/const) or function-scoped (var)
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };

                // Check if exported BEFORE allocating symbol
                let is_exported = self.is_node_exported(arena, idx);

                let sym_id = self.declare_symbol(name, flags, idx, is_exported);
                self.node_symbols.insert(decl.name.0, sym_id);
            } else {
                let flags = if is_block_scoped {
                    symbol_flags::BLOCK_SCOPED_VARIABLE
                } else {
                    symbol_flags::FUNCTION_SCOPED_VARIABLE
                };
                let is_exported = self.is_node_exported(arena, idx);

                let mut names = Vec::new();
                self.collect_binding_identifiers(arena, decl.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(name, flags, ident_idx, is_exported);
                    }
                }
            }

            if !decl.initializer.is_none() {
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
            self.bind_modifiers(arena, &func.modifiers);
            // Function declaration creates a symbol in the current scope
            if let Some(name) = self.get_identifier_name(arena, func.name) {
                let is_exported = self.has_export_modifier(arena, &func.modifiers);
                self.declare_symbol(name, symbol_flags::FUNCTION, idx, is_exported);
            }

            // Enter function scope and bind body
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            self.with_fresh_flow(|binder| {
                // Bind parameters
                for &param_idx in &func.parameters.nodes {
                    binder.bind_parameter(arena, param_idx);
                }

                // Hoisting: Collect var declarations from the function body
                // This ensures var declarations are accessible throughout the function scope
                // before their actual declaration point (JavaScript hoisting behavior)
                //
                // Note: We do NOT hoist function declarations from blocks in ES6+.
                // In ES6 strict mode, function declarations inside blocks are block-scoped.
                binder.collect_hoisted_from_node(arena, func.body);
                binder.process_hoisted_vars(arena);

                // Bind body
                binder.bind_node(arena, func.body);
            });

            self.exit_scope(arena);
        }
    }

    pub(crate) fn bind_parameter(&mut self, arena: &NodeArena, idx: NodeIndex) {
        if let Some(node) = arena.get(idx)
            && let Some(param) = arena.get_parameter(node)
        {
            self.bind_modifiers(arena, &param.modifiers);
            if let Some(name) = self.get_identifier_name(arena, param.name) {
                let sym_id =
                    self.declare_symbol(name, symbol_flags::FUNCTION_SCOPED_VARIABLE, idx, false);
                self.node_symbols.insert(param.name.0, sym_id);
            } else {
                let mut names = Vec::new();
                self.collect_binding_identifiers(arena, param.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = self.get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(
                            name,
                            symbol_flags::FUNCTION_SCOPED_VARIABLE,
                            ident_idx,
                            false,
                        );
                    }
                }
            }

            if !param.initializer.is_none() {
                self.bind_node(arena, param.initializer);
            }
        }
    }

    /// Bind an arrow function expression - creates a scope and binds the body.
    pub(crate) fn bind_arrow_function(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        if let Some(func) = arena.get_function(node) {
            self.bind_modifiers(arena, &func.modifiers);
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Hoisting: Collect var declarations from the function body
                    // Note: We do NOT hoist function declarations from blocks in ES6+.
                    binder.collect_hoisted_from_node(arena, func.body);
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
            self.bind_modifiers(arena, &func.modifiers);
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);
            self.declare_arguments_symbol();

            // Capture enclosing flow for closures (preserves narrowing for const/let variables)
            self.with_fresh_flow_inner(
                |binder| {
                    // Bind parameters
                    for &param_idx in &func.parameters.nodes {
                        binder.bind_parameter(arena, param_idx);
                    }

                    // Hoisting: Collect var declarations from the function body
                    // Note: We do NOT hoist function declarations from blocks in ES6+.
                    binder.collect_hoisted_from_node(arena, func.body);
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

            if !body.is_none() {
                binder.bind_node(arena, body);
            }
        });

        self.exit_scope(arena);
    }

    pub(crate) fn bind_modifiers(&mut self, arena: &NodeArena, modifiers: &Option<NodeList>) {
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
            self.bind_modifiers(arena, &class.modifiers);
            if let Some(name) = self.get_identifier_name(arena, class.name) {
                // Start with CLASS flag
                let mut flags = symbol_flags::CLASS;

                // Add ABSTRACT flag if class has 'abstract' modifier
                if self.has_abstract_modifier(arena, &class.modifiers) {
                    flags |= symbol_flags::ABSTRACT;
                }

                // Check if exported BEFORE allocating symbol
                let is_exported = self.has_export_modifier(arena, &class.modifiers);

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
            self.bind_modifiers(arena, &class.modifiers);
            self.enter_scope(ContainerKind::Class, idx);

            if let Some(name) = self.get_identifier_name(arena, class.name) {
                let mut flags = symbol_flags::CLASS;
                if self.has_abstract_modifier(arena, &class.modifiers) {
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
                        self.bind_modifiers(arena, &method.modifiers);
                        if let Some(name_node) = arena.get(method.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, method.name);
                        }
                        if let Some(name) = self.get_identifier_name(arena, method.name) {
                            let mut flags = symbol_flags::METHOD;
                            if self.has_abstract_modifier(arena, &method.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &method.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(method.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &method.parameters, method.body, idx);
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = arena.get_property_decl(node) {
                        self.bind_modifiers(arena, &prop.modifiers);
                        if let Some(name_node) = arena.get(prop.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, prop.name);
                        }
                        if let Some(name) = self.get_identifier_name(arena, prop.name) {
                            let mut flags = symbol_flags::PROPERTY;
                            if self.has_abstract_modifier(arena, &prop.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &prop.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(prop.name.0, sym_id);
                        }

                        if !prop.initializer.is_none() {
                            self.bind_node(arena, prop.initializer);
                        }
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = arena.get_accessor(node) {
                        self.bind_modifiers(arena, &accessor.modifiers);
                        if let Some(name_node) = arena.get(accessor.name)
                            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        {
                            self.bind_node(arena, accessor.name);
                        }
                        if let Some(name) = self.get_identifier_name(arena, accessor.name) {
                            let mut flags = if node.kind == syntax_kind_ext::GET_ACCESSOR {
                                symbol_flags::GET_ACCESSOR
                            } else {
                                symbol_flags::SET_ACCESSOR
                            };
                            if self.has_abstract_modifier(arena, &accessor.modifiers) {
                                flags |= symbol_flags::ABSTRACT;
                            }
                            if self.has_static_modifier(arena, &accessor.modifiers) {
                                flags |= symbol_flags::STATIC;
                            }
                            let sym_id = self.declare_symbol(name, flags, idx, false);
                            self.node_symbols.insert(accessor.name.0, sym_id);
                        }
                        self.bind_callable_body(arena, &accessor.parameters, accessor.body, idx);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    self.declare_symbol("constructor", symbol_flags::CONSTRUCTOR, idx, false);
                    if let Some(ctor) = arena.get_constructor(node) {
                        self.bind_modifiers(arena, &ctor.modifiers);
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
            && let Some(name) = self.get_identifier_name(arena, iface.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = self.has_export_modifier(arena, &iface.modifiers);

            // If we're inside a global augmentation block, track this as an augmentation
            // that should merge with lib.d.ts symbols at type resolution time
            if self.in_global_augmentation {
                self.global_augmentations
                    .entry(name.to_string())
                    .or_default()
                    .push(idx);
            }

            // Rule #44: Track module augmentation interfaces
            // These will be merged with the target module's interface at type resolution time
            if self.in_module_augmentation {
                if let Some(ref module_spec) = self.current_augmented_module {
                    self.module_augmentations
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::binder::state::ModuleAugmentation::new(
                            name.to_string(),
                            idx,
                        ));
                }
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
            && let Some(name) = self.get_identifier_name(arena, alias.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = self.has_export_modifier(arena, &alias.modifiers);

            // If we're inside a global augmentation block, track this as an augmentation
            // that should merge with lib.d.ts symbols at type resolution time
            if self.in_global_augmentation {
                self.global_augmentations
                    .entry(name.to_string())
                    .or_default()
                    .push(idx);
            }

            // Rule #44: Track module augmentation type aliases
            if self.in_module_augmentation {
                if let Some(ref module_spec) = self.current_augmented_module {
                    self.module_augmentations
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::binder::state::ModuleAugmentation::new(
                            name.to_string(),
                            idx,
                        ));
                }
            }

            self.declare_symbol(name, symbol_flags::TYPE_ALIAS, idx, is_exported);
        }
    }

    pub(crate) fn bind_enum_declaration(&mut self, arena: &NodeArena, node: &Node, idx: NodeIndex) {
        if let Some(enum_decl) = arena.get_enum(node)
            && let Some(name) = self.get_identifier_name(arena, enum_decl.name)
        {
            // Check if exported BEFORE allocating symbol
            let is_exported = self.has_export_modifier(arena, &enum_decl.modifiers);

            // Check if this is a const enum
            let is_const = self.has_const_modifier(arena, &enum_decl.modifiers);
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
            for &member_idx in &enum_decl.members.nodes {
                if let Some(member_node) = arena.get(member_idx)
                    && let Some(member) = arena.get_enum_member(member_node)
                    && let Some(member_name) = self.get_identifier_name(arena, member.name)
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

            // Case block contains case clauses
            if let Some(case_block_node) = arena.get(switch_data.case_block)
                && let Some(case_block) = arena.get_block(case_block_node)
            {
                for &clause_idx in &case_block.statements.nodes {
                    if let Some(clause_node) = arena.get(clause_idx)
                        && let Some(clause) = arena.get_case_clause(clause_node)
                    {
                        self.switch_clause_to_switch.insert(clause_idx.0, idx);

                        self.current_flow = pre_switch_flow;
                        if !clause.expression.is_none() {
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

                        if self.clause_allows_fallthrough(arena, clause) {
                            fallthrough_flow = self.current_flow;
                        } else {
                            fallthrough_flow = FlowNodeId::NONE;
                        }
                    }
                }
            }

            self.current_flow = end_label;
        }
    }

    pub(crate) fn clause_allows_fallthrough(
        &self,
        arena: &NodeArena,
        clause: &crate::parser::node::CaseClauseData,
    ) -> bool {
        let Some(&last_stmt_idx) = clause.statements.nodes.last() else {
            return true;
        };

        let Some(stmt_node) = arena.get(last_stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT =>
            {
                false
            }
            _ => true,
        }
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
            if !try_data.catch_clause.is_none()
                && let Some(catch_node) = arena.get(try_data.catch_clause)
                && let Some(catch) = arena.get_catch_clause(catch_node)
            {
                self.enter_scope(ContainerKind::Block, idx);

                // Catch can be entered from any point in try.
                self.current_flow = pre_try_flow;

                // Bind catch variable and mark it assigned.
                if !catch.variable_declaration.is_none() {
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
            if !try_data.finally_block.is_none() {
                self.current_flow = end_label;
                self.bind_node(arena, try_data.finally_block);
            } else {
                self.current_flow = end_label;
            }
        }
    }

    pub(crate) fn bind_import_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        _idx: NodeIndex,
    ) {
        if let Some(import) = arena.get_import_decl(node) {
            // Get module specifier for cross-file module resolution
            let module_specifier = if !import.module_specifier.is_none() {
                arena
                    .get(import.module_specifier)
                    .and_then(|spec_node| arena.get_literal(spec_node))
                    .map(|lit| lit.text.clone())
            } else {
                None
            };

            if let Some(clause_node) = arena.get(import.import_clause)
                && let Some(clause) = arena.get_import_clause(clause_node)
            {
                let clause_type_only = clause.is_type_only;
                // Default import
                if !clause.name.is_none()
                    && let Some(name) = self.get_identifier_name(arena, clause.name)
                {
                    let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.declarations.push(clause.name);
                        sym.is_type_only = clause_type_only;
                        // Track module for cross-file resolution
                        if let Some(ref specifier) = module_specifier {
                            sym.import_module = Some(specifier.clone());
                            // Default imports (`import X from "mod"`) resolve the module's
                            // **default** export, regardless of the local binding name.
                            sym.import_name = Some("default".to_string());
                        }
                    }
                    self.current_scope.set(name.to_string(), sym_id);
                    self.node_symbols.insert(clause.name.0, sym_id);
                }

                // Named imports
                if !clause.named_bindings.is_none()
                    && let Some(bindings_node) = arena.get(clause.named_bindings)
                {
                    if bindings_node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(name) = self.get_identifier_name(arena, clause.named_bindings) {
                            let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.declarations.push(clause.named_bindings);
                                sym.is_type_only = clause_type_only;
                                // Track module for cross-file resolution
                                if let Some(ref specifier) = module_specifier {
                                    sym.import_module = Some(specifier.clone());
                                }
                            }
                            self.current_scope.set(name.to_string(), sym_id);
                            self.node_symbols.insert(clause.named_bindings.0, sym_id);
                        }
                    } else if let Some(named) = arena.get_named_imports(bindings_node) {
                        // Handle namespace import: import * as ns from 'module'
                        if !named.name.is_none()
                            && let Some(name) = self.get_identifier_name(arena, named.name)
                        {
                            let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                            if let Some(sym) = self.symbols.get_mut(sym_id) {
                                sym.declarations.push(named.name);
                                sym.is_type_only = clause_type_only;
                                // Track module for cross-file resolution
                                if let Some(ref specifier) = module_specifier {
                                    sym.import_module = Some(specifier.clone());
                                }
                            }
                            self.current_scope.set(name.to_string(), sym_id);
                            self.node_symbols.insert(named.name.0, sym_id);
                            self.node_symbols.insert(clause.named_bindings.0, sym_id);
                        }
                        // Handle named imports: import { foo, bar } from 'module'
                        for &spec_idx in &named.elements.nodes {
                            if let Some(spec_node) = arena.get(spec_idx)
                                && let Some(spec) = arena.get_specifier(spec_node)
                            {
                                let spec_type_only = clause_type_only || spec.is_type_only;
                                let local_ident = if !spec.name.is_none() {
                                    spec.name
                                } else {
                                    spec.property_name
                                };
                                let local_name = self.get_identifier_name(arena, local_ident);

                                if let Some(name) = local_name {
                                    let sym_id =
                                        self.symbols.alloc(symbol_flags::ALIAS, name.to_string());

                                    // Get property name before mutable borrow to avoid borrow checker error
                                    let prop_name =
                                        if !spec.name.is_none() && !spec.property_name.is_none() {
                                            self.get_identifier_name(arena, spec.property_name)
                                        } else {
                                            None
                                        };

                                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                                        sym.declarations.push(local_ident);
                                        sym.is_type_only = spec_type_only;
                                        // Track module and original name for cross-file resolution
                                        if let Some(ref specifier) = module_specifier {
                                            sym.import_module = Some(specifier.clone());
                                            // For renamed imports (import { foo as bar }), track original name
                                            if let Some(prop_name) = prop_name {
                                                sym.import_name = Some(prop_name.to_string());
                                            }
                                        }
                                    }
                                    self.current_scope.set(name.to_string(), sym_id);
                                    self.node_symbols.insert(spec_idx.0, sym_id);
                                    self.node_symbols.insert(local_ident.0, sym_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Bind import equals declaration: import x = ns.member or import x = require("...")
    pub(crate) fn bind_import_equals_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(import) = arena.get_import_decl(node) {
            // import_clause holds the alias name (e.g., 'x' in 'import x = ...')
            if let Some(name) = self.get_identifier_name(arena, import.import_clause) {
                // Check if exported (for export import x = ns.member)
                let is_exported = self.has_export_modifier(arena, &import.modifiers);

                // Get module specifier for external module require imports
                // e.g., import ts = require("typescript") -> module_specifier = "typescript"
                let module_specifier = if !import.module_specifier.is_none() {
                    arena.get(import.module_specifier).and_then(|spec_node| {
                        if spec_node.kind == SyntaxKind::StringLiteral as u16 {
                            arena.get_literal(spec_node).map(|lit| lit.text.clone())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };

                // Create symbol with ALIAS flag
                let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());

                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    sym.declarations.push(idx);
                    sym.value_declaration = idx;
                    sym.is_exported = is_exported;
                    // Track module for cross-file resolution and unresolved import detection
                    if let Some(ref specifier) = module_specifier {
                        sym.import_module = Some(specifier.clone());
                    }
                }

                self.current_scope.set(name.to_string(), sym_id);
                self.node_symbols.insert(idx.0, sym_id);
                // Also add to persistent scope for checker lookup
                self.declare_in_persistent_scope(name.to_string(), sym_id);
            }
        }
    }

    /// Mark symbols associated with a declaration node as exported.
    /// This is required because the parser wraps exported declarations in ExportDeclaration
    /// nodes instead of attaching modifiers to the declaration itself.
    pub(crate) fn mark_exported_symbols(&mut self, arena: &NodeArena, idx: NodeIndex) {
        // 1. Try direct symbol lookup (Function, Class, Enum, Module, Interface, TypeAlias)
        if let Some(sym_id) = self.node_symbols.get(&idx.0) {
            if let Some(sym) = self.symbols.get_mut(*sym_id) {
                sym.is_exported = true;
            }
            return;
        }

        // 2. Handle VariableStatement -> VariableDeclarationList -> VariableDeclaration
        // Variable statements don't have a symbol; their declarations do.
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var) = arena.get_variable(node)
        {
            for &list_idx in &var.declarations.nodes {
                if let Some(list_node) = arena.get(list_idx)
                    && let Some(list) = arena.get_variable(list_node)
                {
                    for &decl_idx in &list.declarations.nodes {
                        if let Some(sym_id) = self.node_symbols.get(&decl_idx.0)
                            && let Some(sym) = self.symbols.get_mut(*sym_id)
                        {
                            sym.is_exported = true;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn bind_export_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        _idx: NodeIndex,
    ) {
        if let Some(export) = arena.get_export_decl(node) {
            // Export clause can be:
            // - NamedExports: export { foo, bar }
            // - NamespaceExport: export * as ns from 'mod'
            // - Declaration: export function/class/const/etc
            // - or NONE for: export * from 'mod'

            // Check if the entire export declaration is type-only: export type { ... }
            let export_type_only = export.is_type_only;

            // export default ...
            //
            // Note: the parser represents `export default ...` as an EXPORT_DECLARATION with
            // `is_default_export = true`, so we must handle it *before* the "namespace export"
            // fallback that matches any identifier clause.
            if export.is_default_export {
                // Always bind the exported expression/declaration so inner references are visited.
                self.bind_node(arena, export.export_clause);

                // Synthesize a "default" export symbol for cross-file import resolution.
                // This enables `import X from './file'` to resolve the default export.
                let default_sym_id = self.symbols.alloc(
                    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
                    "default".to_string(),
                );
                if let Some(default_sym) = self.symbols.get_mut(default_sym_id) {
                    default_sym.is_exported = true;
                    default_sym.is_type_only = export_type_only;
                    default_sym.declarations.push(export.export_clause);
                    default_sym.value_declaration = export.export_clause;
                }
                // Add to file_locals so it can be found during import resolution
                self.file_locals.set("default".to_string(), default_sym_id);
                self.node_symbols
                    .insert(export.export_clause.0, default_sym_id);

                // Also mark the underlying local symbol as exported if it exists
                if let Some(name) = self.get_identifier_name(arena, export.export_clause) {
                    if let Some(sym_id) = self
                        .current_scope
                        .get(name)
                        .or_else(|| self.file_locals.get(name))
                        && let Some(sym) = self.symbols.get_mut(sym_id)
                    {
                        sym.is_exported = true;
                        sym.is_type_only = export_type_only;
                    }
                } else if let Some(clause_node) = arena.get(export.export_clause)
                    && self.is_declaration(clause_node.kind)
                {
                    self.mark_exported_symbols(arena, export.export_clause);
                }

                return;
            }

            if !export.export_clause.is_none()
                && let Some(clause_node) = arena.get(export.export_clause)
            {
                // Check if it's named exports { foo, bar }
                if let Some(named) = arena.get_named_imports(clause_node) {
                    // Check if this is a re-export: export { foo } from 'module'
                    if !export.module_specifier.is_none() {
                        // Get the module name from module_specifier
                        let module_name = if export.module_specifier.is_some() {
                            let idx = export.module_specifier;
                            arena
                                .get(idx)
                                .and_then(|node| arena.get_literal(node))
                                .map(|lit| lit.text.clone())
                        } else {
                            None
                        };

                        if let Some(source_module) = module_name {
                            let current_file = self.debugger.current_file.clone();

                            // Collect all the export mappings first (before mutable borrow)
                            // Also collect node indices and names for creating symbols
                            let mut export_mappings: Vec<(String, Option<String>, NodeIndex)> =
                                Vec::new();
                            for &spec_idx in &named.elements.nodes {
                                if let Some(spec_node) = arena.get(spec_idx)
                                    && let Some(spec) = arena.get_specifier(spec_node)
                                {
                                    // Get the original name (property_name) and exported name (name)
                                    let original_name = if spec.property_name.is_some() {
                                        self.get_identifier_name(arena, spec.property_name)
                                    } else {
                                        None
                                    };
                                    let exported_name = if spec.name.is_some() {
                                        self.get_identifier_name(arena, spec.name)
                                    } else {
                                        None
                                    };

                                    if let Some(exported) = exported_name.or(original_name) {
                                        export_mappings.push((
                                            exported.to_string(),
                                            original_name.map(|s| s.to_string()),
                                            spec_idx,
                                        ));
                                    }
                                }
                            }

                            // Create symbols for re-export specifiers so they can be tracked
                            // in the compilation cache for incremental invalidation
                            for (exported, _, spec_idx) in &export_mappings {
                                // Use declare_symbol to add to file_locals
                                let sym_id = self.declare_symbol(
                                    exported,
                                    symbol_flags::ALIAS | symbol_flags::EXPORT_VALUE,
                                    *spec_idx,
                                    true, // re-exports are always "exported"
                                );
                                if let Some(sym) = self.symbols.get_mut(sym_id) {
                                    sym.is_exported = true;
                                    sym.is_type_only = export_type_only;
                                }
                                self.node_symbols.insert(spec_idx.0, sym_id);
                            }

                            // Now apply the mutable borrow to insert the mappings
                            let file_reexports = self.reexports.entry(current_file).or_default();
                            for (exported, original, _) in export_mappings {
                                file_reexports.insert(exported, (source_module.clone(), original));
                            }
                        }
                    } else {
                        // Regular export { foo, bar } without 'from' clause
                        // This can be either:
                        // 1. Top-level exports from a module
                        // 2. Namespace member re-exports: namespace N { export { x } }
                        //
                        // For namespaces, we need to add the exported symbols to the namespace's exports table
                        // so they can be accessed as N.x

                        // Check if we're inside a namespace
                        let current_namespace_sym_id = self
                            .scope_chain
                            .get(self.current_scope_idx)
                            .and_then(|ctx| {
                                if ctx.container_kind == ContainerKind::Module {
                                    Some(ctx.container_node)
                                } else {
                                    None
                                }
                            })
                            .and_then(|container_idx| {
                                self.node_symbols.get(&container_idx.0).copied()
                            });

                        for &spec_idx in &named.elements.nodes {
                            if let Some(spec_node) = arena.get(spec_idx)
                                && let Some(spec) = arena.get_specifier(spec_node)
                            {
                                // Determine if this specifier is type-only
                                // (either from export type { ... } or export { type foo })
                                let spec_type_only = export_type_only || spec.is_type_only;

                                // For export { foo }, property_name is NONE, name is "foo"
                                // For export { foo as bar }, property_name is "foo", name is "bar"
                                let original_name = if !spec.property_name.is_none() {
                                    self.get_identifier_name(arena, spec.property_name)
                                } else {
                                    self.get_identifier_name(arena, spec.name)
                                };

                                let exported_name = if !spec.name.is_none() {
                                    self.get_identifier_name(arena, spec.name)
                                } else {
                                    original_name
                                };

                                if let (Some(orig), Some(exp)) = (original_name, exported_name) {
                                    // Resolve the original symbol in the current scope
                                    let resolved_sym_id = self
                                        .current_scope
                                        .get(orig)
                                        .or_else(|| self.file_locals.get(orig));

                                    if let Some(sym_id) = resolved_sym_id {
                                        // Create export symbol (EXPORT_VALUE for value exports)
                                        let export_sym_id = self
                                            .symbols
                                            .alloc(symbol_flags::EXPORT_VALUE, exp.to_string());
                                        // Set is_type_only and is_exported on the symbol
                                        if let Some(sym) = self.symbols.get_mut(export_sym_id) {
                                            sym.is_exported = true;
                                            sym.is_type_only = spec_type_only;
                                            // Store the target symbol for re-exports within namespaces
                                            if let Some(ns_sym_id) = current_namespace_sym_id {
                                                // This is a namespace re-export - add to namespace's exports
                                                if let Some(ns_sym) =
                                                    self.symbols.get_mut(ns_sym_id)
                                                {
                                                    let exports =
                                                        ns_sym.exports.get_or_insert_with(|| {
                                                            Box::new(SymbolTable::new())
                                                        });
                                                    exports.set(exp.to_string(), sym_id);
                                                }
                                            }
                                        }
                                        self.node_symbols.insert(spec_idx.0, export_sym_id);
                                    }
                                }
                            }
                        }
                    }
                }
                // Check if it's an exported declaration (function, class, variable, etc.)
                else if self.is_declaration(clause_node.kind) {
                    // Recursively bind the declaration
                    // This handles: export function foo() {}, export class Bar {}, export const x = 1
                    self.bind_node(arena, export.export_clause);

                    // FIX: Explicitly mark the bound symbol(s) as exported
                    // because the inner declaration node lacks the 'export' modifier
                    self.mark_exported_symbols(arena, export.export_clause);
                }
                // Namespace export: export * as ns from 'mod'
                else if let Some(name) = self.get_identifier_name(arena, export.export_clause) {
                    let sym_id = self.symbols.alloc(symbol_flags::ALIAS, name.to_string());
                    // Set is_type_only and is_exported for namespace exports
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        sym.is_exported = true;
                        sym.is_type_only = export_type_only;
                    }
                    self.current_scope.set(name.to_string(), sym_id);
                    self.node_symbols.insert(export.export_clause.0, sym_id);
                }
            }

            // Handle `export * from 'module'` (wildcard re-exports)
            // This is when export_clause is None but module_specifier is not None
            if export.export_clause.is_none() && !export.module_specifier.is_none() {
                let module_name = if export.module_specifier.is_some() {
                    let idx = export.module_specifier;
                    arena
                        .get(idx)
                        .and_then(|node| arena.get_literal(node))
                        .map(|lit| lit.text.clone())
                } else {
                    None
                };

                if let Some(source_module) = module_name {
                    let current_file = self.debugger.current_file.clone();
                    // Add to wildcard_reexports list - a file can have multiple export * from
                    self.wildcard_reexports
                        .entry(current_file)
                        .or_default()
                        .push(source_module);
                }
            }
        }
    }

    /// Check if a node kind is a declaration that should be bound
    pub(crate) fn is_declaration(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::ENUM_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
            || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
    }

    pub(crate) fn bind_module_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(module) = arena.get_module(node) {
            let is_global_augmentation = (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
                || arena
                    .get(module.name)
                    .and_then(|name_node| {
                        if let Some(ident) = arena.get_identifier(name_node) {
                            return Some(ident.escaped_text == "global");
                        }
                        if name_node.kind == SyntaxKind::GlobalKeyword as u16 {
                            return Some(true);
                        }
                        None
                    })
                    .unwrap_or(false);

            if is_global_augmentation {
                if !module.body.is_none() {
                    self.node_scope_ids
                        .insert(module.body.0, self.current_scope_id);
                    // Set flag so interface declarations inside are tracked as augmentations
                    let was_in_global_augmentation = self.in_global_augmentation;
                    self.in_global_augmentation = true;
                    self.bind_node(arena, module.body);
                    self.in_global_augmentation = was_in_global_augmentation;
                }
                return;
            }

            if let Some(name_node) = arena.get(module.name)
                && (name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
            {
                // Ambient module declaration with string literal name
                if let Some(lit) = arena.get_literal(name_node)
                    && !lit.text.is_empty()
                {
                    let module_specifier = lit.text.clone();

                    // Rule #44: Detect module augmentation
                    // A `declare module "x"` in an external module (file with imports/exports)
                    // is a module augmentation if it references an existing or external module.
                    let is_augmentation = self.is_external_module
                        && self.is_potential_module_augmentation(&module_specifier);

                    if is_augmentation {
                        // Track as module augmentation - bind body with augmentation context
                        if !module.body.is_none() {
                            self.node_scope_ids
                                .insert(module.body.0, self.current_scope_id);
                            let was_in_augmentation = self.in_module_augmentation;
                            let prev_module = self.current_augmented_module.take();
                            self.in_module_augmentation = true;
                            self.current_augmented_module = Some(module_specifier.clone());
                            self.bind_node(arena, module.body);
                            self.in_module_augmentation = was_in_augmentation;
                            self.current_augmented_module = prev_module;
                        }
                        return;
                    }

                    // Not an augmentation - track as ambient module declaration
                    self.declared_modules.insert(module_specifier);
                }
            }

            let name = self
                .get_identifier_name(arena, module.name)
                .map(str::to_string)
                .or_else(|| {
                    arena
                        .get(module.name)
                        .and_then(|name_node| arena.get_literal(name_node))
                        .map(|lit| lit.text.clone())
                });
            let mut prior_exports: Option<SymbolTable> = None;
            let mut module_symbol_id = SymbolId::NONE;
            if let Some(name) = name {
                let mut is_exported = self.has_export_modifier(arena, &module.modifiers);
                if !is_exported && let Some(ext) = arena.get_extended(idx) {
                    let parent_idx = ext.parent;
                    if let Some(parent_node) = arena.get(parent_idx)
                        && parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(parent_module) = arena.get_module(parent_node)
                        && parent_module.body == idx
                    {
                        is_exported = true;
                    }
                }
                let flags = symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE;
                module_symbol_id = self.declare_symbol(&name, flags, idx, is_exported);
                prior_exports = self
                    .symbols
                    .get(module_symbol_id)
                    .and_then(|symbol| symbol.exports.as_ref())
                    .map(|exports| exports.as_ref().clone());
            }

            // Enter module scope
            self.enter_scope(ContainerKind::Module, idx);

            if let Some(exports) = prior_exports {
                for (name, &child_id) in exports.iter() {
                    self.current_scope.set(name.clone(), child_id);
                }
            }

            // Also register the MODULE_BLOCK body node with the same scope
            // so that identifiers inside the namespace can find their enclosing scope
            // when walking up through the parent chain (identifier -> ... -> MODULE_BLOCK -> MODULE_DECLARATION)
            if !module.body.is_none() {
                self.node_scope_ids
                    .insert(module.body.0, self.current_scope_id);
            } else {
                // Shorthand ambient module declaration: `declare module "foo"` without body
                // Track this so imports from these modules are typed as `any`
                if let Some(name_node) = arena.get(module.name)
                    && let Some(lit) = arena.get_literal(name_node)
                    && !lit.text.is_empty()
                {
                    self.shorthand_ambient_modules.insert(lit.text.clone());
                }
            }

            self.bind_node(arena, module.body);

            // Populate exports for the module symbol
            if !module_symbol_id.is_none() && !module.body.is_none() {
                self.populate_module_exports(arena, module.body, module_symbol_id);
            }

            self.exit_scope(arena);
        }
    }

    /// Populate the exports table of a module/namespace symbol based on exported declarations in its body.
    pub(crate) fn populate_module_exports(
        &mut self,
        arena: &NodeArena,
        body_idx: NodeIndex,
        module_symbol_id: SymbolId,
    ) {
        let Some(node) = arena.get(body_idx) else {
            return;
        };

        // Get the module block statements
        let statements = if let Some(module_block) = arena.get_module_block(node) {
            if let Some(stmts) = &module_block.statements {
                &stmts.nodes
            } else {
                return;
            }
        } else {
            return;
        };

        for &stmt_idx in statements {
            if let Some(stmt_node) = arena.get(stmt_idx) {
                // Check for export modifier
                let is_exported = match stmt_node.kind {
                    syntax_kind_ext::VARIABLE_STATEMENT => arena
                        .get_variable(stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::FUNCTION_DECLARATION => arena
                        .get_function(stmt_node)
                        .and_then(|f| f.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::CLASS_DECLARATION => arena
                        .get_class(stmt_node)
                        .and_then(|c| c.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::INTERFACE_DECLARATION => arena
                        .get_interface(stmt_node)
                        .and_then(|i| i.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                        .get_type_alias(stmt_node)
                        .and_then(|t| t.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::ENUM_DECLARATION => arena
                        .get_enum(stmt_node)
                        .and_then(|e| e.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::MODULE_DECLARATION => arena
                        .get_module(stmt_node)
                        .and_then(|m| m.modifiers.as_ref())
                        .is_some_and(|mods| self.has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::EXPORT_DECLARATION => true, // export { x }
                    _ => false,
                };

                if is_exported {
                    // Collect the exported names first
                    let mut exported_names = Vec::new();

                    match stmt_node.kind {
                        syntax_kind_ext::VARIABLE_STATEMENT => {
                            if let Some(var_stmt) = arena.get_variable(stmt_node) {
                                for &decl_idx in &var_stmt.declarations.nodes {
                                    if let Some(decl_node) = arena.get(decl_idx)
                                        && let Some(decl) =
                                            arena.get_variable_declaration(decl_node)
                                        && let Some(name_node) = arena.get(decl.name)
                                        && let Some(ident) = arena.get_identifier(name_node)
                                    {
                                        exported_names.push(ident.escaped_text.to_string());
                                    }
                                }
                            }
                        }
                        syntax_kind_ext::FUNCTION_DECLARATION => {
                            if let Some(func) = arena.get_function(stmt_node)
                                && let Some(name) = self.get_identifier_name(arena, func.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::CLASS_DECLARATION => {
                            if let Some(class) = arena.get_class(stmt_node)
                                && let Some(name) = self.get_identifier_name(arena, class.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::ENUM_DECLARATION => {
                            if let Some(enm) = arena.get_enum(stmt_node)
                                && let Some(name) = self.get_identifier_name(arena, enm.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::INTERFACE_DECLARATION => {
                            if let Some(iface) = arena.get_interface(stmt_node)
                                && let Some(name) = self.get_identifier_name(arena, iface.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                            if let Some(alias) = arena.get_type_alias(stmt_node)
                                && let Some(name) = self.get_identifier_name(arena, alias.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::MODULE_DECLARATION => {
                            if let Some(module) = arena.get_module(stmt_node) {
                                let name = self
                                    .get_identifier_name(arena, module.name)
                                    .map(str::to_string)
                                    .or_else(|| {
                                        arena
                                            .get(module.name)
                                            .and_then(|name_node| arena.get_literal(name_node))
                                            .map(|lit| lit.text.clone())
                                    });
                                if let Some(name) = name {
                                    exported_names.push(name);
                                }
                            }
                        }
                        _ => {}
                    }

                    // Now add them to exports
                    for name in &exported_names {
                        if let Some(sym_id) = self.current_scope.get(name) {
                            if let Some(module_sym) = self.symbols.get_mut(module_symbol_id) {
                                let exports = module_sym
                                    .exports
                                    .get_or_insert_with(|| Box::new(SymbolTable::new()));
                                exports.set(name.clone(), sym_id);
                            }
                            // Mark the child symbol as exported
                            if let Some(child_sym) = self.symbols.get_mut(sym_id) {
                                child_sym.is_exported = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check if any modifier in a NodeList is the export keyword.
    pub(crate) fn has_export_modifier_any(&self, arena: &NodeArena, modifiers: &NodeList) -> bool {
        for &mod_idx in &modifiers.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == SyntaxKind::ExportKeyword as u16
            {
                return true;
            }
        }
        false
    }

    // Public accessors

    /// Check if lib symbols have been merged into this binder's local arena.
    pub fn lib_symbols_are_merged(&self) -> bool {
        self.lib_symbols_merged
    }

    /// Set the lib_symbols_merged flag.
    ///
    /// This should be called when a binder is reconstructed from a MergedProgram
    /// where all lib symbols have already been remapped to unique global IDs.
    pub fn set_lib_symbols_merged(&mut self, merged: bool) {
        self.lib_symbols_merged = merged;
    }

    pub fn get_symbol(&self, id: SymbolId) -> Option<&Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
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

        // Try local symbols
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
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
        lib_binders: &'a [Arc<BinderState>],
    ) -> Option<&'a Symbol> {
        // Fast path: If lib symbols are merged, all symbols are in the local arena
        // with unique IDs - no need to check lib_binders.
        if self.lib_symbols_merged {
            return self.symbols.get(id);
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

        // First try local symbols
        if let Some(sym) = self.symbols.get(id) {
            return Some(sym);
        }

        // Then try lib binders
        for lib_binder in lib_binders {
            if let Some(sym) = lib_binder.symbols.get(id) {
                return Some(sym);
            }
        }

        None
    }

    /// Look up a global type by name from file_locals and lib binders.
    ///
    /// This method is used by the checker to find built-in types like Array, Object,
    /// Function, Promise, etc. It checks:
    /// 1. Local file_locals (for user-defined globals or merged lib symbols)
    /// 2. Lib binders (only when lib_symbols_merged is false)
    ///
    /// Returns the SymbolId if found, None otherwise.
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
        lib_binders: &[Arc<BinderState>],
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

    /// Check if a global type exists (in file_locals or lib binders).
    ///
    /// This is a convenience method for checking type availability without
    /// actually retrieving the symbol.
    pub fn has_global_type(&self, name: &str) -> bool {
        self.get_global_type(name).is_some()
    }

    pub fn get_node_symbol(&self, node: NodeIndex) -> Option<SymbolId> {
        self.node_symbols.get(&node.0).copied()
    }

    pub fn get_symbols(&self) -> &SymbolArena {
        &self.symbols
    }

    /// Check if the current source file is an external module (has top-level import/export).
    /// This is used by the checker to determine if ES module semantics apply.
    pub fn is_external_module(&self) -> bool {
        self.is_external_module
    }

    /// Check if a module specifier likely refers to an existing module that can be augmented.
    /// Rule #44: Module augmentation vs ambient module declaration detection.
    ///
    /// Returns true if:
    /// - The module specifier refers to an already declared module
    /// - The specifier looks like an external package (not a relative path)
    pub(crate) fn is_potential_module_augmentation(&self, module_specifier: &str) -> bool {
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
        if !self.current_flow.is_none() {
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
    /// If capture_enclosing is true, the START node will point to the enclosing flow, allowing
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
            && !prev_flow.is_none()
            && let Some(start_node) = self.flow_nodes.get_mut(start_flow)
        {
            start_node.antecedent.push(prev_flow);
        }

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
    }

    // =========================================================================
    // Flow graph construction helpers
    // =========================================================================

    /// Create a branch label flow node for merging control flow paths.
    pub(crate) fn create_branch_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::BRANCH_LABEL)
    }

    /// Create a loop label flow node for back-edges.
    pub(crate) fn create_loop_label(&mut self) -> FlowNodeId {
        self.flow_nodes.alloc(flow_flags::LOOP_LABEL)
    }

    /// Create a flow condition node for tracking type narrowing.
    pub(crate) fn create_flow_condition(
        &mut self,
        flags: u32,
        antecedent: FlowNodeId,
        condition: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flags);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.antecedent.push(antecedent);
            node.node = condition;
        }
        id
    }

    /// Create a flow node for a switch clause with optional fallthrough.
    pub(crate) fn create_switch_clause_flow(
        &mut self,
        pre_switch: FlowNodeId,
        fallthrough: FlowNodeId,
        clause: NodeIndex,
    ) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::SWITCH_CLAUSE);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = clause;
        }
        self.add_antecedent(id, pre_switch);
        self.add_antecedent(id, fallthrough);
        id
    }

    /// Create a flow node for an assignment.
    pub(crate) fn create_flow_assignment(&mut self, assignment: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ASSIGNMENT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = assignment;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for a call expression.
    pub(crate) fn create_flow_call(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::CALL);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for array mutation (e.g. push/splice).
    pub(crate) fn create_flow_array_mutation(&mut self, call: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::ARRAY_MUTATION);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = call;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for await expression (async suspension point).
    pub(crate) fn create_flow_await_point(&mut self, await_expr: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::AWAIT_POINT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = await_expr;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Create a flow node for yield expression (generator suspension point).
    pub(crate) fn create_flow_yield_point(&mut self, yield_expr: NodeIndex) -> FlowNodeId {
        let id = self.flow_nodes.alloc(flow_flags::YIELD_POINT);
        if let Some(node) = self.flow_nodes.get_mut(id) {
            node.node = yield_expr;
            if !self.current_flow.is_none() {
                node.antecedent.push(self.current_flow);
            }
        }
        id
    }

    /// Add an antecedent to a flow node (for merging branches).
    pub(crate) fn add_antecedent(&mut self, label: FlowNodeId, antecedent: FlowNodeId) {
        if antecedent.is_none() || antecedent == self.unreachable_flow {
            return;
        }
        if let Some(node) = self.flow_nodes.get_mut(label)
            && !node.antecedent.contains(&antecedent)
        {
            node.antecedent.push(antecedent);
        }
    }

    // =========================================================================
    // Expression binding for flow analysis
    // =========================================================================

    pub(crate) fn is_assignment_operator(&self, operator: u16) -> bool {
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

    pub(crate) fn is_array_mutation_call(&self, arena: &NodeArena, call_idx: NodeIndex) -> bool {
        let Some(call_node) = arena.get(call_idx) else {
            return false;
        };
        let Some(call) = arena.get_call_expr(call_node) else {
            return false;
        };
        let Some(callee_node) = arena.get(call.expression) else {
            return false;
        };
        let Some(access) = arena.get_access_expr(callee_node) else {
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
    pub(crate) fn bind_binary_expression_iterative(&mut self, arena: &NodeArena, root: NodeIndex) {
        enum WorkItem {
            Visit(NodeIndex),
            PostAssign(NodeIndex),
        }

        let mut stack = vec![WorkItem::Visit(root)];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Visit(idx) => {
                    let node = match arena.get(idx) {
                        Some(n) => n,
                        None => continue,
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        if let Some(bin) = arena.get_binary_expr(node) {
                            if self.is_assignment_operator(bin.operator_token) {
                                stack.push(WorkItem::PostAssign(idx));
                                if !bin.right.is_none() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if !bin.left.is_none() {
                                    stack.push(WorkItem::Visit(bin.left));
                                }
                                continue;
                            }
                            if !bin.right.is_none() {
                                stack.push(WorkItem::Visit(bin.right));
                            }
                            if !bin.left.is_none() {
                                stack.push(WorkItem::Visit(bin.left));
                            }
                        }
                        continue;
                    }

                    self.bind_node(arena, idx);
                }
                WorkItem::PostAssign(idx) => {
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
                }
            }
        }
    }

    /// Bind a short-circuit binary expression (&&, ||, ??) with intermediate
    /// flow condition nodes.
    ///
    /// For `a && b`: the right operand `b` is only evaluated when `a` is truthy,
    /// so we create a TRUE_CONDITION node for `a` before binding `b`. This allows
    /// references in `b` to see type narrowing from `a`.
    ///
    /// For `a || b` and `a ?? b`: the right operand `b` is only evaluated when `a`
    /// is falsy/nullish, so we create a FALSE_CONDITION node for `a` before binding `b`.
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
                    let node = match arena.get(idx) {
                        Some(n) => n,
                        None => continue,
                    };

                    if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                        self.record_flow(idx);
                        if let Some(bin) = arena.get_binary_expr(node) {
                            if self.is_assignment_operator(bin.operator_token) {
                                stack.push(WorkItem::PostAssign(idx));
                                if !bin.right.is_none() {
                                    stack.push(WorkItem::Visit(bin.right));
                                }
                                if !bin.left.is_none() {
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
                            if !bin.right.is_none() {
                                stack.push(WorkItem::Visit(bin.right));
                            }
                            if !bin.left.is_none() {
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

        let node = match arena.get(idx) {
            Some(n) => n,
            None => return,
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin) = arena.get_binary_expr(node) {
                if self.is_assignment_operator(bin.operator_token) {
                    self.record_flow(idx);
                    self.bind_expression(arena, bin.left);
                    self.bind_expression(arena, bin.right);
                    let flow = self.create_flow_assignment(idx);
                    self.current_flow = flow;
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
                    if self.is_array_mutation_call(arena, idx) {
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

            // Type assertion (e.g., x as string)
            k if k == syntax_kind_ext::AS_EXPRESSION || k == syntax_kind_ext::TYPE_ASSERTION => {
                if let Some(as_expr) = arena.get_access_expr(node) {
                    self.bind_expression(arena, as_expr.expression);
                }
                return;
            }

            // Conditional expression (ternary)
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = arena.get_conditional_expr(node) {
                    self.bind_expression(arena, cond.condition);
                    self.bind_expression(arena, cond.when_true);
                    self.bind_expression(arena, cond.when_false);
                }
                return;
            }

            _ => {}
        }

        self.bind_node(arena, idx);
    }

    /// Run post-binding validation checks on the symbol table.
    /// Returns a list of validation errors found.
    pub fn validate_symbol_table(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        for (&node_idx, &sym_id) in self.node_symbols.iter() {
            if self.symbols.get(sym_id).is_none() {
                errors.push(ValidationError::BrokenSymbolLink {
                    node_index: node_idx,
                    symbol_id: sym_id.0,
                });
            }
        }

        for i in 0..self.symbols.len() {
            let sym_id = crate::binder::SymbolId(i as u32);
            if let Some(sym) = self.symbols.get(sym_id)
                && sym.declarations.is_empty()
            {
                errors.push(ValidationError::OrphanedSymbol {
                    symbol_id: i as u32,
                    name: sym.escaped_name.clone(),
                });
            }
        }

        for i in 0..self.symbols.len() {
            let sym_id = crate::binder::SymbolId(i as u32);
            if let Some(sym) = self.symbols.get(sym_id)
                && !sym.value_declaration.is_none()
            {
                let has_node_mapping = self.node_symbols.contains_key(&sym.value_declaration.0);
                if !has_node_mapping {
                    errors.push(ValidationError::InvalidValueDeclaration {
                        symbol_id: i as u32,
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
    /// - Total symbol count from file_locals and lib_binders
    pub fn get_lib_symbol_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== Lib Symbol Availability Report ===\n\n");

        // Count total symbols
        let file_local_count = self.file_locals.len();
        let lib_binder_count: usize = self.lib_binders.iter().map(|b| b.file_locals.len()).sum();

        report.push_str(&format!("File locals: {} symbols\n", file_local_count));
        report.push_str(&format!(
            "Lib binders: {} symbols ({} binders)\n\n",
            lib_binder_count,
            self.lib_binders.len()
        ));

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

        report.push_str(&format!(
            "Expected symbols present: {}/{}\n",
            present.len(),
            Self::EXPECTED_GLOBAL_SYMBOLS.len()
        ));
        if !missing.is_empty() {
            report.push_str("\nMissing symbols:\n");
            for name in &missing {
                report.push_str(&format!("  - {}\n", name));
            }
        }

        // Show which lib binders contribute symbols
        if !self.lib_binders.is_empty() {
            report.push_str("\nLib binder contributions:\n");
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                report.push_str(&format!(
                    "  Lib binder {}: {} symbols\n",
                    i,
                    lib_binder.file_locals.len()
                ));
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

        if !missing.is_empty() {
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
        } else {
            debug!(
                "[LIB_SYMBOL_INFO] All {} expected global symbols are present.",
                Self::EXPECTED_GLOBAL_SYMBOLS.len()
            );
            false
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
    /// - Available symbols by source (scopes, file_locals, lib_binders)
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
}

impl Default for BinderState {
    fn default() -> Self {
        Self::new()
    }
}
