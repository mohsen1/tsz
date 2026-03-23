//! Binder declaration binding, accessors, and flow graph construction.
//!
//! Validation, diagnostics reporting, and resolution statistics live in
//! `validation.rs`.

use crate::state::FileFeatures;
use crate::{
    ContainerKind, FlowNodeId, Symbol, SymbolArena, SymbolId, SymbolTable, flow_flags, symbol_flags,
};
use std::sync::Arc;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use crate::state::BinderState;

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

                // Track variable declarations inside `declare global { }` blocks
                // as global augmentations, just like interfaces and namespaces.
                // This enables cross-file conflict detection with UMD exports.
                if self.in_global_augmentation {
                    self.global_augmentations
                        .entry(name.to_string())
                        .or_default()
                        .push(crate::state::GlobalAugmentation::new(idx));
                }

                let sym_id = self.declare_symbol(name, flags, idx, is_exported);
                self.node_symbols.insert(decl.name.0, sym_id);
                self.record_semantic_def(
                    sym_id,
                    crate::state::SemanticDefKind::Variable,
                    name,
                    idx,
                    0,
                    Vec::new(),
                    is_exported,
                );

                // Hoist global augmentation variables to file_locals for cross-file
                // visibility. Without this, `declare global { const X }` variables are
                // invisible to cross-file duplicate detection (e.g., UMD `export as
                // namespace X` conflicting with `declare global { const X }`).
                // This mirrors the interface hoisting at bind_interface_declaration.
                if self.in_global_augmentation {
                    self.file_locals.set(name.to_string(), sym_id);
                    self.global_augmentations
                        .entry(name.to_string())
                        .or_default()
                        .push(crate::state::GlobalAugmentation::new(idx));
                }
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
                        let sym_id = self.declare_symbol(name, flags, ident_idx, is_exported);
                        self.record_semantic_def(
                            sym_id,
                            crate::state::SemanticDefKind::Variable,
                            name,
                            ident_idx,
                            0,
                            Vec::new(),
                            is_exported,
                        );
                        if self.in_global_augmentation {
                            self.file_locals.set(name.to_string(), sym_id);
                            self.global_augmentations
                                .entry(name.to_string())
                                .or_default()
                                .push(crate::state::GlobalAugmentation::new(ident_idx));
                        }
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

                let sym_id = self.declare_symbol(name, symbol_flags::FUNCTION, idx, is_exported);
                let tp_count = func
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names = Self::collect_type_param_names(arena, func.type_parameters.as_ref());
                self.record_semantic_def(
                    sym_id,
                    crate::state::SemanticDefKind::Function,
                    name,
                    idx,
                    tp_count,
                    tp_names,
                    is_exported,
                );
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
                // Walk binding element initializers so that nested functions
                // (e.g. arrow functions used as default values in destructuring)
                // get their scopes and parameter symbols created.
                self.bind_binding_element_initializers(arena, param.name);
            }

            if param.initializer.is_some() {
                self.bind_node(arena, param.initializer);
            }
        }
    }

    /// Declare PROPERTY symbols in the current (class) scope for constructor
    /// parameter properties. Called before entering the constructor's function scope
    /// so that the property symbols live in the class scope and can be tracked for
    /// TS6138 unused property checking.
    ///
    /// If an explicit property declaration with the same name already exists in the
    /// class scope, skip the parameter property declaration to avoid duplicate symbols.
    pub(crate) fn bind_parameter_properties(&mut self, arena: &NodeArena, parameters: &NodeList) {
        for &param_idx in &parameters.nodes {
            let Some(param_node) = arena.get(param_idx) else {
                continue;
            };
            let Some(param) = arena.get_parameter(param_node) else {
                continue;
            };

            // Only parameters with property modifiers (public/private/protected/readonly)
            if !Self::has_parameter_property_modifier(arena, param.modifiers.as_ref()) {
                continue;
            }

            let Some(name) = Self::get_identifier_name(arena, param.name) else {
                continue;
            };

            // Skip if there's already a symbol with this name in the class scope
            // (e.g., an explicit property declaration like `y: number;`).
            if self.current_scope.get(name).is_some() {
                continue;
            }

            let mut flags = symbol_flags::PROPERTY;
            if Self::has_private_modifier(arena, param.modifiers.as_ref()) {
                flags |= symbol_flags::PRIVATE;
            }
            if Self::has_protected_modifier(arena, param.modifiers.as_ref()) {
                flags |= symbol_flags::PROTECTED;
            }
            // Use the parameter node as the declaration so the checker can
            // distinguish parameter-property PROPERTY symbols from regular ones.
            self.declare_symbol(name, flags, param_idx, false);
        }
    }

    /// Recursively walk a binding pattern and call `bind_node` on each
    /// binding element's initializer.  This ensures that function expressions
    /// and arrow functions used as default values inside destructuring patterns
    /// are properly bound (scopes created, parameters declared).
    fn bind_binding_element_initializers(&mut self, arena: &NodeArena, pattern_idx: NodeIndex) {
        let Some(pattern_node) = arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = arena.get_binding_pattern(pattern_node) else {
            return;
        };
        for &elem_idx in &pattern_data.elements.nodes {
            let Some(elem_node) = arena.get(elem_idx) else {
                continue;
            };
            let Some(elem_data) = arena.get_binding_element(elem_node) else {
                continue;
            };
            // Bind the initializer expression (e.g., arrow functions as defaults)
            if elem_data.initializer.is_some() {
                self.bind_node(arena, elem_data.initializer);
            }
            // Recurse into nested binding patterns
            if let Some(name_node) = arena.get(elem_data.name)
                && name_node.is_binding_pattern()
            {
                self.bind_binding_element_initializers(arena, elem_data.name);
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
                    let sym_id =
                        self.declare_symbol(name, symbol_flags::TYPE_PARAMETER, param_idx, false);
                    self.node_symbols.insert(type_param.name.0, sym_id);
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

            // Arrow functions are never generators (no asterisk_token).
            // Check async + immediately-invoked for IIFE treatment.
            let is_iife = !func.is_async && arena.is_immediately_invoked(idx);

            self.bind_modifiers(arena, func.modifiers.as_ref());
            // Enter function scope
            self.enter_scope(ContainerKind::Function, idx);

            // Bind type parameters (e.g., <T> in arrow functions)
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            if is_iife {
                // IIFE: bind body inline in the outer flow context (no FlowStart node).
                let return_label = self.create_branch_label();
                self.return_targets.push(return_label);

                tracing::debug!(
                    param_count = func.parameters.nodes.len(),
                    "Binding arrow IIFE parameters"
                );
                for &param_idx in &func.parameters.nodes {
                    self.bind_parameter(arena, param_idx);
                }
                self.collect_hoisted_from_node(arena, func.body);
                self.process_hoisted_functions(arena);
                self.process_hoisted_vars(arena);
                self.bind_node(arena, func.body);

                // Merge fall-through with return flows
                self.add_antecedent(return_label, self.current_flow);
                let return_label = self
                    .return_targets
                    .pop()
                    .expect("return_targets pushed before function body binding");

                if let Some(label_node) = self.flow_nodes.get(return_label) {
                    match label_node.antecedent.len() {
                        0 => self.current_flow = self.unreachable_flow,
                        1 => self.current_flow = label_node.antecedent[0],
                        _ => self.current_flow = return_label,
                    }
                } else {
                    self.current_flow = self.unreachable_flow;
                }
            } else {
                // Non-IIFE: isolated flow scope
                self.with_fresh_flow_inner(
                    |binder| {
                        tracing::debug!(
                            param_count = func.parameters.nodes.len(),
                            "Binding arrow function parameters"
                        );
                        for &param_idx in &func.parameters.nodes {
                            binder.bind_parameter(arena, param_idx);
                        }
                        binder.collect_hoisted_from_node(arena, func.body);
                        binder.process_hoisted_functions(arena);
                        binder.process_hoisted_vars(arena);
                        binder.bind_node(arena, func.body);
                    },
                    true,
                );
            }

            self.exit_scope(arena);
        }
    }

    /// Bind a function expression - creates a scope and binds the body.
    ///
    /// For non-async, non-generator IIFEs (Immediately Invoked Function Expressions),
    /// the body is bound inline in the outer control flow context. This means:
    /// - Narrowed variables from the outer scope remain narrowed inside the IIFE
    /// - Assignments inside the IIFE propagate to the outer scope's control flow
    /// - Return statements are redirected to a branch label (not the outer function's return)
    ///
    /// This matches tsc's behavior where IIFEs are part of the containing control flow.
    pub(crate) fn bind_function_expression(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(func) = arena.get_function(node) {
            // A non-async, non-generator IIFE is considered part of the containing
            // control flow. Return statements behave similarly to break statements
            // that exit to a label just past the statement body.
            let is_iife =
                !func.is_async && !func.asterisk_token && arena.is_immediately_invoked(idx);

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

            if is_iife {
                // IIFE: bind body inline in the outer flow context (no FlowStart node).
                // This preserves narrowing and propagates assignments to the outer scope.
                let return_label = self.create_branch_label();
                self.return_targets.push(return_label);

                for &param_idx in &func.parameters.nodes {
                    self.bind_parameter(arena, param_idx);
                }
                self.collect_hoisted_from_node(arena, func.body);
                self.process_hoisted_functions(arena);
                self.process_hoisted_vars(arena);
                self.bind_node(arena, func.body);

                // Merge the fall-through flow with the return label
                self.add_antecedent(return_label, self.current_flow);
                let return_label = self
                    .return_targets
                    .pop()
                    .expect("return_targets pushed before function body binding");

                // Finalize: if the return label has antecedents, use it as current flow.
                // This mirrors tsc's finishFlowLabel behavior.
                if let Some(label_node) = self.flow_nodes.get(return_label) {
                    match label_node.antecedent.len() {
                        0 => self.current_flow = self.unreachable_flow,
                        1 => self.current_flow = label_node.antecedent[0],
                        _ => self.current_flow = return_label,
                    }
                } else {
                    self.current_flow = self.unreachable_flow;
                }
            } else {
                // Non-IIFE: isolated flow scope with captured enclosing flow
                self.with_fresh_flow_inner(
                    |binder| {
                        for &param_idx in &func.parameters.nodes {
                            binder.bind_parameter(arena, param_idx);
                        }
                        binder.collect_hoisted_from_node(arena, func.body);
                        binder.process_hoisted_functions(arena);
                        binder.process_hoisted_vars(arena);
                        binder.bind_node(arena, func.body);
                    },
                    true,
                );
            }

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
        self.bind_callable_body_with_type_params(arena, parameters, body, idx, None);
    }

    pub(crate) fn bind_callable_body_with_type_params(
        &mut self,
        arena: &NodeArena,
        parameters: &NodeList,
        body: NodeIndex,
        idx: NodeIndex,
        type_parameters: Option<&NodeList>,
    ) {
        self.enter_scope(ContainerKind::Function, idx);
        self.declare_arguments_symbol();

        // Bind type parameters into the function scope so they're visible
        // in parameter types, return types, and body type references.
        self.bind_type_parameters(arena, type_parameters);

        // Capture enclosing flow so that const variables narrowed in an outer scope
        // preserve their narrowing inside method/accessor/constructor bodies.
        // The flow graph walker (check_flow at START nodes) uses `is_mutable_variable`
        // to decide whether to reset narrowing or traverse outward.
        self.with_fresh_flow_inner(
            |binder| {
                for &param_idx in &parameters.nodes {
                    binder.bind_parameter(arena, param_idx);
                }

                if body.is_some() {
                    binder.bind_node(arena, body);
                }
            },
            true,
        );

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

                let is_abstract = Self::has_abstract_modifier(arena, class.modifiers.as_ref());
                let sym_id = self.declare_symbol(name, flags, idx, is_exported);
                let tp_count = class
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, class.type_parameters.as_ref());
                let heritage_names =
                    Self::collect_heritage_clause_names(arena, class.heritage_clauses.as_ref());
                self.record_semantic_def_ext(
                    sym_id,
                    crate::state::SemanticDefKind::Class,
                    name,
                    idx,
                    tp_count,
                    tp_names,
                    is_exported,
                    Vec::new(),
                    false, // is_const
                    is_abstract,
                    heritage_names,
                );
            }

            // Enter class scope for members
            self.enter_scope(ContainerKind::Class, idx);

            self.bind_type_parameters(arena, class.type_parameters.as_ref());
            if let Some(ref heritage) = class.heritage_clauses {
                for &clause_idx in &heritage.nodes {
                    self.bind_node(arena, clause_idx);
                }
            }

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
            } else {
                // Anonymous class expression: create a CLASS symbol so that
                // the checker can use it as parent_id on instance properties,
                // enabling "(Anonymous class)" display in diagnostics.
                let mut flags = symbol_flags::CLASS;
                if Self::has_abstract_modifier(arena, class.modifiers.as_ref()) {
                    flags |= symbol_flags::ABSTRACT;
                }
                let sym_id = self.symbols.alloc(flags, "(Anonymous class)".to_string());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    sym.declarations.push(idx);
                    sym.value_declaration = idx;
                }
                self.node_symbols.insert(idx.0, sym_id);
            }

            self.bind_type_parameters(arena, class.type_parameters.as_ref());
            if let Some(ref heritage) = class.heritage_clauses {
                for &clause_idx in &heritage.nodes {
                    self.bind_node(arena, clause_idx);
                }
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
                        self.bind_callable_body_with_type_params(
                            arena,
                            &method.parameters,
                            method.body,
                            idx,
                            method.type_parameters.as_ref(),
                        );
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
                        // Declare PROPERTY symbols for parameter properties (public/private/
                        // protected/readonly params) in the class scope BEFORE entering the
                        // constructor's function scope. This enables reference tracking for
                        // TS6138 ("Property 'x' is declared but its value is never read").
                        self.bind_parameter_properties(arena, &ctor.parameters);
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

                // If the name already exists as an import alias in the current scope,
                // do NOT call declare_symbol — it would merge INTERFACE flags into the
                // import alias, contaminating it and causing type_reference_symbol_type
                // to build the wrong type. The augmentation is already tracked in
                // module_augmentations and will be merged at type resolution time.
                //
                // However, if the name is NEW (e.g. `interface ModelWithCache` added via
                // `declare module "backbone" { ... }`), we still need to declare it so
                // it can be found via qualified access like `Backbone.ModelWithCache`.
                let name_conflicts_with_import = self
                    .current_scope
                    .get(name)
                    .and_then(|sym_id| self.symbols.get(sym_id))
                    .is_some_and(|sym| sym.import_module.is_some());
                if name_conflicts_with_import {
                    return;
                }
            }

            let sym_id = self.declare_symbol(name, symbol_flags::INTERFACE, idx, is_exported);
            let tp_count = iface
                .type_parameters
                .as_ref()
                .map_or(0, |tp| tp.nodes.len() as u16);
            let tp_names = Self::collect_type_param_names(arena, iface.type_parameters.as_ref());
            let heritage_names =
                Self::collect_heritage_clause_names(arena, iface.heritage_clauses.as_ref());
            self.record_semantic_def_ext(
                sym_id,
                crate::state::SemanticDefKind::Interface,
                name,
                idx,
                tp_count,
                tp_names,
                is_exported,
                Vec::new(),
                false,
                false,
                heritage_names,
            );

            // Track symbols declared inside module augmentation blocks so the checker
            // can redirect self-referential type lookups (e.g., `self: Foo` inside
            // `declare module "./m" { interface Foo { self: Foo } }`) to the merged type.
            if self.in_module_augmentation
                && sym_id.is_some()
                && let Some(ref module_spec) = self.current_augmented_module
            {
                self.augmentation_target_modules
                    .insert(sym_id, module_spec.clone());
            }

            // Hoist global augmentation interfaces to file_locals for cross-file visibility.
            // Same rationale as namespace hoisting in bind_module_declaration.
            if self.in_global_augmentation && sym_id.is_some() {
                self.file_locals.set(name.to_string(), sym_id);
            }
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

            // Check if an ALIAS (namespace re-export) already occupies this name.
            // When `export * as X from "..."` comes before `export type X = ...`,
            // the ALIAS symbol is already in scope. We must create a separate
            // TYPE_ALIAS symbol and record the partnership so the checker can
            // resolve type references to the type alias body while value references
            // go through the namespace alias.
            let existing_alias_id = self
                .current_scope
                .get(name)
                .filter(|id| {
                    self.symbols
                        .get(*id)
                        .is_some_and(|s| s.flags & symbol_flags::ALIAS != 0)
                })
                .or_else(|| {
                    self.module_exports
                        .get(self.debugger.current_file.as_str())
                        .and_then(|exports| exports.get(name))
                        .filter(|id| {
                            self.symbols
                                .get(*id)
                                .is_some_and(|s| s.flags & symbol_flags::ALIAS != 0)
                        })
                });
            if let Some(alias_id) = existing_alias_id {
                let sym_id = self
                    .symbols
                    .alloc(symbol_flags::TYPE_ALIAS, name.to_string());
                if let Some(sym) = self.symbols.get_mut(sym_id) {
                    sym.declarations.push(idx);
                    sym.is_exported = is_exported;
                }
                // TYPE_ALIAS takes current_scope so type references resolve to it
                self.current_scope.set(name.to_string(), sym_id);
                if self.current_scope_id.is_some()
                    && !self.in_module_augmentation
                    && self
                        .scopes
                        .get(self.current_scope_id.0 as usize)
                        .is_some_and(|scope| scope.kind == ContainerKind::SourceFile)
                {
                    self.file_locals.set(name.to_string(), sym_id);
                }
                self.node_symbols.insert(idx.0, sym_id);
                self.declare_in_persistent_scope(name.to_string(), sym_id);
                // Record partnership: TYPE_ALIAS → ALIAS
                self.alias_partners.insert(sym_id, alias_id);
                let tp_count = alias
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, alias.type_parameters.as_ref());
                self.record_semantic_def(
                    sym_id,
                    crate::state::SemanticDefKind::TypeAlias,
                    name,
                    idx,
                    tp_count,
                    tp_names,
                    is_exported,
                );
            } else {
                let sym_id = self.declare_symbol(name, symbol_flags::TYPE_ALIAS, idx, is_exported);
                let tp_count = alias
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, alias.type_parameters.as_ref());
                self.record_semantic_def(
                    sym_id,
                    crate::state::SemanticDefKind::TypeAlias,
                    name,
                    idx,
                    tp_count,
                    tp_names,
                    is_exported,
                );
            }

            self.enter_scope(ContainerKind::Block, idx);
            self.bind_type_parameters(arena, alias.type_parameters.as_ref());
            self.exit_scope(arena);
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

            // Collect enum member names at bind time for stable identity.
            let enum_member_names: Vec<String> = enum_decl
                .members
                .nodes
                .iter()
                .filter_map(|&member_idx| {
                    let member_node = arena.get(member_idx)?;
                    let member = arena.get_enum_member(member_node)?;
                    Self::get_property_name(arena, member.name).map(|n| n.to_string())
                })
                .collect();

            self.record_semantic_def_ext(
                enum_sym_id,
                crate::state::SemanticDefKind::Enum,
                name,
                idx,
                0,
                Vec::new(), // type_param_names (enums are not generic)
                is_exported,
                enum_member_names,
                is_const,
                false,      // is_abstract
                Vec::new(), // heritage_names
            );

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

            // Seed the new scope with existing ENUM MEMBER exports from prior declarations.
            // This allows merged enum declarations to reference members from
            // earlier declarations (e.g., `enum E { a } enum E { c = a }`).
            // We filter to ENUM_MEMBER only so namespace exports don't leak in
            // (e.g., `namespace x { export let y } enum x { z = y }` should error).
            for (name, sym_id) in exports.iter() {
                if let Some(sym) = self.symbols.get(*sym_id)
                    && sym.flags & symbol_flags::ENUM_MEMBER != 0
                {
                    self.current_scope.set(name.to_string(), *sym_id);
                }
            }

            for &member_idx in &enum_decl.members.nodes {
                if let Some(member_node) = arena.get(member_idx)
                    && let Some(member) = arena.get_enum_member(member_node)
                    && let Some(member_name) = Self::get_property_name(arena, member.name)
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

        // Save and clear return_targets so that return statements inside
        // non-IIFE functions don't redirect to an enclosing IIFE's return target.
        let prev_return_targets = std::mem::take(&mut self.return_targets);

        self.current_flow = start_flow;
        bind_body(self);
        self.current_flow = prev_flow;
        self.return_targets = prev_return_targets;
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

        let is_assignment = operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || operator == SyntaxKind::BarBarEqualsToken as u16
            || operator == SyntaxKind::QuestionQuestionEqualsToken as u16;

        if operator == SyntaxKind::AmpersandAmpersandToken as u16
            || operator == SyntaxKind::AmpersandAmpersandEqualsToken as u16
        {
            // For && and &&=: right side is only evaluated when left is truthy
            let true_condition =
                self.create_flow_condition(flow_flags::TRUE_CONDITION, after_left_flow, left);
            self.current_flow = true_condition;
            self.bind_expression(arena, right);
            if is_assignment && !Self::is_inside_class_member_computed_property_name(arena, idx) {
                self.current_flow = self.create_flow_assignment(idx);
            }
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
            // For ||, ??, ||=, ??=: right side is only evaluated when left is falsy/nullish
            let false_condition =
                self.create_flow_condition(flow_flags::FALSE_CONDITION, after_left_flow, left);
            self.current_flow = false_condition;
            self.bind_expression(arena, right);
            if is_assignment && !Self::is_inside_class_member_computed_property_name(arena, idx) {
                self.current_flow = self.create_flow_assignment(idx);
            }
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
                            if bin.operator_token
                                == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                                || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
                                || bin.operator_token
                                    == SyntaxKind::QuestionQuestionEqualsToken as u16
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

                            if Self::is_assignment_operator(bin.operator_token) {
                                // For destructuring defaults (LHS is a pattern),
                                // bind RHS before LHS to match runtime eval order.
                                let lhs_is_destructuring = bin.operator_token
                                    == SyntaxKind::EqualsToken as u16
                                    && arena.get(bin.left).is_some_and(|left_node| {
                                        left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                            || left_node.kind
                                                == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                            || left_node.kind
                                                == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                            || left_node.kind
                                                == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                    });
                                stack.push(WorkItem::PostAssign(idx));
                                if lhs_is_destructuring {
                                    // Stack is LIFO: push LHS last so it runs after RHS
                                    if bin.left.is_some() {
                                        stack.push(WorkItem::Visit(bin.left));
                                    }
                                    if bin.right.is_some() {
                                        stack.push(WorkItem::Visit(bin.right));
                                    }
                                } else {
                                    if bin.right.is_some() {
                                        stack.push(WorkItem::Visit(bin.right));
                                    }
                                    if bin.left.is_some() {
                                        stack.push(WorkItem::Visit(bin.left));
                                    }
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
                    if !Self::is_inside_class_member_computed_property_name(arena, idx) {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
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
                if bin.operator_token == SyntaxKind::AmpersandAmpersandEqualsToken as u16
                    || bin.operator_token == SyntaxKind::BarBarEqualsToken as u16
                    || bin.operator_token == SyntaxKind::QuestionQuestionEqualsToken as u16
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

                if Self::is_assignment_operator(bin.operator_token) {
                    self.record_flow(idx);
                    // For destructuring assignments (LHS is array/object literal),
                    // bind the RHS (source/default) before the LHS (pattern).
                    // This matches tsc's bindDestructuringTargetFlow: at runtime,
                    // the source/default is evaluated before the pattern is applied,
                    // so flow-sensitive reads in the default must see pre-assignment
                    // values. E.g., `[{ [(a = 1)]: b } = [9, a] as const] = []`
                    // must evaluate `[9, a]` (reading `a = 0`) before `(a = 1)`.
                    let lhs_is_destructuring = bin.operator_token == SyntaxKind::EqualsToken as u16
                        && arena.get(bin.left).is_some_and(|left_node| {
                            left_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || left_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || left_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || left_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        });
                    if lhs_is_destructuring {
                        self.bind_expression(arena, bin.right);
                        self.bind_expression(arena, bin.left);
                    } else {
                        self.bind_expression(arena, bin.left);
                        self.bind_expression(arena, bin.right);
                    }
                    if !Self::is_inside_class_member_computed_property_name(arena, idx) {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                    // Detect expando property assignments (X.prop = value)
                    if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                        self.detect_expando_assignment(arena, bin.left, bin.right);
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
                    if (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                        && !Self::is_inside_class_member_computed_property_name(arena, idx)
                    {
                        let flow = self.create_flow_assignment(idx);
                        self.current_flow = flow;
                    }
                }
                return;
            }

            // Property access (e.g., x.foo)
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);
                }
                return;
            }

            // Element access (e.g., x[0], x?.[expr])
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = arena.get_access_expr(node) {
                    self.bind_expression(arena, access.expression);

                    // Optional chaining short-circuits RHS evaluation.
                    // For `obj?.[expr]`, `expr` is evaluated only when `obj` is present.
                    let expr_has_optional_chain =
                        arena.get(access.expression).is_some_and(|expr| {
                            (u32::from(expr.flags) & node_flags::OPTIONAL_CHAIN) != 0
                        });
                    if access.question_dot_token
                        || (u32::from(node.flags) & node_flags::OPTIONAL_CHAIN) != 0
                        || expr_has_optional_chain
                    {
                        let after_base = self.current_flow;

                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            after_base,
                            access.expression,
                        );
                        self.current_flow = true_flow;
                        self.bind_expression(arena, access.name_or_argument);
                        let after_element = self.current_flow;

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            after_base,
                            access.expression,
                        );

                        let merge = self.create_branch_label();
                        self.add_antecedent(merge, after_element);
                        self.add_antecedent(merge, false_flow);
                        self.current_flow = merge;
                    } else {
                        self.bind_expression(arena, access.name_or_argument);
                    }
                }
                return;
            }

            // Call expression (e.g., isString(x))
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    self.bind_expression(arena, call.expression);

                    let is_optional_call =
                        (u32::from(node.flags) & node_flags::OPTIONAL_CHAIN) != 0;
                    if is_optional_call {
                        let after_callee = self.current_flow;

                        // Optional calls short-circuit argument evaluation when callee is absent.
                        let true_flow = self.create_flow_condition(
                            flow_flags::TRUE_CONDITION,
                            after_callee,
                            call.expression,
                        );
                        self.current_flow = true_flow;
                        if let Some(args) = &call.arguments {
                            for &arg in &args.nodes {
                                self.bind_expression(arena, arg);
                            }
                        }
                        let flow = self.create_flow_call(idx);
                        self.current_flow = flow;
                        if Self::is_array_mutation_call(arena, idx) {
                            let flow = self.create_flow_array_mutation(idx);
                            self.current_flow = flow;
                        }
                        let after_call = self.current_flow;

                        let false_flow = self.create_flow_condition(
                            flow_flags::FALSE_CONDITION,
                            after_callee,
                            call.expression,
                        );

                        let merge = self.create_branch_label();
                        self.add_antecedent(merge, after_call);
                        self.add_antecedent(merge, false_flow);
                        self.current_flow = merge;
                    } else {
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
    /// Tracks both simple identifiers (`X.prop`) and dotted receiver chains
    /// (`A.B.prop`) so function members on namespaces can collect expandos.
    fn detect_expando_assignment(&mut self, arena: &NodeArena, lhs: NodeIndex, rhs: NodeIndex) {
        fn symbol_call(arena: &NodeArena, idx: NodeIndex) -> bool {
            let Some(node) = arena.get(idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return false;
            }
            let Some(call) = arena.get_call_expr(node) else {
                return false;
            };
            let Some(callee) = arena.get(call.expression) else {
                return false;
            };
            callee.kind == SyntaxKind::Identifier as u16
                && arena
                    .get_identifier(callee)
                    .is_some_and(|ident| ident.escaped_text == "Symbol")
        }

        fn is_undefined_like_rhs(arena: &NodeArena, idx: NodeIndex) -> bool {
            let Some(node) = arena.get(idx) else {
                return false;
            };

            if node.kind == SyntaxKind::Identifier as u16 {
                return arena
                    .get_identifier(node)
                    .is_some_and(|ident| ident.escaped_text == "undefined");
            }

            if node.kind != syntax_kind_ext::VOID_EXPRESSION
                && node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            {
                return false;
            }

            let Some(unary) = arena.get_unary_expr(node) else {
                return false;
            };
            if unary.operator != SyntaxKind::VoidKeyword as u16 {
                return false;
            }
            let Some(expr) = arena.get(unary.operand) else {
                return false;
            };
            matches!(expr.kind, k if k == SyntaxKind::NumericLiteral as u16)
                && arena.get_literal(expr).is_some_and(|lit| lit.text == "0")
        }

        if is_undefined_like_rhs(arena, rhs) {
            return;
        }

        fn property_access_chain(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let left = property_access_chain(arena, access.expression)?;
                let right_node = arena.get(access.name_or_argument)?;
                let right = arena.get_identifier(right_node)?.escaped_text.clone();
                return Some(format!("{left}.{right}"));
            }
            None
        }

        fn root_identifier_index(arena: &NodeArena, idx: NodeIndex) -> Option<NodeIndex> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return Some(idx);
            }
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                return root_identifier_index(arena, access.expression);
            }
            None
        }

        fn resolved_const_expando_key(
            binder: &BinderState,
            arena: &NodeArena,
            sym_id: SymbolId,
            depth: u8,
        ) -> Option<String> {
            if depth > 8 {
                return None;
            }

            let symbol = binder.symbols.get(sym_id)?;
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else {
                symbol
                    .declarations
                    .iter()
                    .copied()
                    .find(|decl| !decl.is_none())?
            };
            if !arena.is_const_variable_declaration(decl_idx) {
                return None;
            }

            let decl_node = arena.get(decl_idx)?;
            let var_decl = arena.get_variable_declaration(decl_node)?;
            let init_idx = var_decl.initializer;
            if init_idx.is_none() {
                return None;
            }
            let init_node = arena.get(init_idx)?;

            match init_node.kind {
                k if k == SyntaxKind::StringLiteral as u16
                    || k == SyntaxKind::NumericLiteral as u16
                    || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    arena.get_literal(init_node).map(|lit| lit.text.clone())
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = arena.get_unary_expr(init_node)?;
                    let operand = arena.get(unary.operand)?;
                    if operand.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let lit = arena.get_literal(operand)?;
                    match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{}", lit.text)),
                        k if k == SyntaxKind::PlusToken as u16 => Some(lit.text.clone()),
                        _ => None,
                    }
                }
                k if k == SyntaxKind::Identifier as u16 => {
                    let name = arena.get_identifier(init_node)?.escaped_text.clone();
                    let next_sym = binder.file_locals.get(&name)?;
                    resolved_const_expando_key(binder, arena, next_sym, depth + 1)
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    symbol_call(arena, init_idx).then(|| format!("__unique_{}", sym_id.0))
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    let assertion = arena.get_type_assertion(init_node)?;
                    let inner = arena.get(assertion.expression)?;
                    match inner.kind {
                        k if k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                        {
                            arena.get_literal(inner).map(|lit| lit.text.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        fn expando_member_key(
            binder: &BinderState,
            arena: &NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            match node.kind {
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                    let access = arena.get_access_expr(node)?;
                    let name_node = arena.get(access.name_or_argument)?;
                    arena
                        .get_identifier(name_node)
                        .map(|ident| ident.escaped_text.clone())
                }
                syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                    let access = arena.get_access_expr(node)?;
                    let key_node = arena.get(access.name_or_argument)?;
                    match key_node.kind {
                        k if k == SyntaxKind::Identifier as u16 => {
                            let ident = arena.get_identifier(key_node)?;
                            binder
                                .file_locals
                                .get(&ident.escaped_text)
                                .and_then(|sym_id| {
                                    resolved_const_expando_key(binder, arena, sym_id, 0)
                                })
                                .or_else(|| Some(ident.escaped_text.clone()))
                        }
                        k if k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                        {
                            arena.get_literal(key_node).map(|lit| lit.text.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }

        let Some(lhs_node) = arena.get(lhs) else {
            return;
        };
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return;
        }
        let Some(access) = arena.get_access_expr(lhs_node) else {
            return;
        };
        let Some(prop_name) = expando_member_key(self, arena, lhs) else {
            return;
        };

        let Some(obj_key) = property_access_chain(arena, access.expression) else {
            return;
        };
        let root_name = obj_key.split('.').next().unwrap_or_default();
        if root_name.is_empty() {
            return;
        }

        // CommonJS export chains like `module.exports.foo = ...` and
        // `module.exports.foo.bar = ...` don't resolve through `file_locals`
        // because `module` is not a user-declared symbol. Track them directly
        // so the checker can reuse one expando summary path for property reads
        // and forward-reference TS2565 checks.
        if obj_key == "module.exports"
            || obj_key.starts_with("module.exports.")
            || obj_key == "exports"
            || obj_key.starts_with("exports.")
        {
            self.expando_properties
                .entry(obj_key)
                .or_default()
                .insert(prop_name);
            return;
        }

        // Resolve the root identifier through the enclosing scope chain so nested
        // function/value roots share the same expando summary path as top-level ones.
        let Some(root_ident) = root_identifier_index(arena, access.expression) else {
            return;
        };
        let Some(sym_id) = self.resolve_identifier(arena, root_ident) else {
            return;
        };
        let Some(symbol) = self.symbols.get(sym_id) else {
            return;
        };

        // Track for functions and namespace-like roots.
        // Classes are excluded: tsc does not allow expando assignments on class
        // constructor types (`class C {} C.x = 1;` → TS2339).
        if (symbol.flags
            & (symbol_flags::FUNCTION
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE))
            != 0
            && (symbol.flags & symbol_flags::CLASS) == 0
        {
            self.expando_properties
                .entry(obj_key.clone())
                .or_default()
                .insert(prop_name);
            return;
        }

        // Also track for variables initialized with function/class/object-literal expressions
        // (e.g. `var X = function(){}; X.prop = 1` or `var X = {}; X.prop = 1`)
        // For typed variables, only track function/arrow inits (expando function pattern).
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
            if var_decl.initializer.is_none() {
                return;
            }
            let Some(init_node) = arena.get(var_decl.initializer) else {
                return;
            };
            let has_type_annotation = var_decl.type_annotation.is_some();
            let is_function_like = init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || init_node.kind == syntax_kind_ext::ARROW_FUNCTION;
            let is_expando_init = is_function_like
                || (!has_type_annotation
                    && (init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                        || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION));
            if is_expando_init {
                self.expando_properties
                    .entry(obj_key)
                    .or_default()
                    .insert(prop_name);
            }
        }
    }

    /// Check if the current scope is the global (file-level) scope.
    /// Record a semantic definition entry for a top-level declaration.
    ///
    /// This captures stable identity information at bind time so the checker
    /// can pre-create solver `DefIds` during construction rather than inventing
    /// them on demand in hot paths.
    ///
    /// Only records entries for declarations at the source file scope (ScopeId(0))
    /// to avoid noise from nested declarations that are less likely to be
    /// cross-file semantic references.
    /// Collect type parameter names from a type parameter `NodeList`.
    ///
    /// Returns an empty `Vec` if `type_params` is `None` or contains no
    /// extractable names. Each entry is the escaped text of the type
    /// parameter identifier (e.g., `["T", "U"]` for `<T, U>`).
    pub(crate) fn collect_type_param_names(
        arena: &NodeArena,
        type_params: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(params) = type_params else {
            return Vec::new();
        };
        params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let node = arena.get(param_idx)?;
                let tp = arena.get_type_parameter(node)?;
                let name = Self::get_identifier_name(arena, tp.name)?;
                Some(name.to_string())
            })
            .collect()
    }

    pub(crate) fn record_semantic_def(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        type_param_count: u16,
        type_param_names: Vec<String>,
        is_exported: bool,
    ) {
        self.record_semantic_def_ext(
            sym_id,
            kind,
            name,
            declaration,
            type_param_count,
            type_param_names,
            is_exported,
            Vec::new(),
            false,
            false,
            Vec::new(),
        );
    }

    /// Extended version of `record_semantic_def` that also captures enriched
    /// identity data: enum member names, const-enum flag, and abstract-class flag.
    ///
    /// This captures stable identity information at bind time so the checker
    /// can pre-create solver `DefIds` during construction rather than inventing
    /// them on demand in hot paths.
    ///
    /// Only records entries for declarations at the source file scope (ScopeId(0))
    /// to avoid noise from nested declarations that are less likely to be
    /// cross-file semantic references.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_semantic_def_ext(
        &mut self,
        sym_id: SymbolId,
        kind: crate::state::SemanticDefKind,
        name: &str,
        declaration: NodeIndex,
        type_param_count: u16,
        type_param_names: Vec<String>,
        is_exported: bool,
        enum_member_names: Vec<String>,
        is_const: bool,
        is_abstract: bool,
        heritage_names: Vec<String>,
    ) {
        // Only capture top-level declarations (source file scope or module scope)
        // and declarations inside `declare global { }` blocks.
        // Nested declarations (inside function bodies, class bodies, etc.) are not
        // recorded because they don't participate in cross-file identity.
        let is_top_level = self.current_scope_id == crate::ScopeId(0)
            || self
                .scopes
                .get(self.current_scope_id.0 as usize)
                .is_some_and(|scope| {
                    matches!(
                        scope.kind,
                        crate::ContainerKind::SourceFile | crate::ContainerKind::Module
                    )
                });
        // Declarations inside `declare global { }` blocks are semantically
        // top-level even if their scope chain doesn't directly match
        // SourceFile/Module (e.g., when the global block is nested inside
        // another module declaration). Capture them so the pre-population
        // pipeline creates stable DefIds for global augmentations.
        if !is_top_level && !self.in_global_augmentation {
            return;
        }
        // Declaration merging: keep the first declaration's core identity stable
        // (kind, name, span, file_id) but accumulate heritage_names and
        // type_param_count from later declarations.  This ensures the
        // pre-populated DefinitionInfo has complete heritage information
        // (e.g., `interface A extends B {}` + `interface A extends C {}`
        // yields heritage_names = ["B", "C"]).
        if let Some(existing) = self.semantic_defs.get_mut(&sym_id) {
            // Accumulate new heritage_names that aren't already present.
            for h in &heritage_names {
                if !existing.heritage_names.contains(h) {
                    existing.heritage_names.push(h.clone());
                }
            }
            // If the first declaration had no type params but this one does
            // (e.g., augmentation adds generics), update the arity and names.
            if existing.type_param_count == 0 && type_param_count > 0 {
                existing.type_param_count = type_param_count;
                existing.type_param_names = type_param_names;
            }
            // If the later declaration is exported, mark as exported.
            if is_exported {
                existing.is_exported = true;
            }
            // Accumulate enum members from later enum declarations.
            if !enum_member_names.is_empty() {
                for m in &enum_member_names {
                    if !existing.enum_member_names.contains(m) {
                        existing.enum_member_names.push(m.clone());
                    }
                }
            }
            // Promote global augmentation flag if any declaration is from declare global.
            if self.in_global_augmentation {
                existing.is_global_augmentation = true;
            }
            return;
        }
        // Determine containing namespace symbol, if any.
        // A declaration is namespace-parented when its scope is Module but not
        // the source-file root (ScopeId(0)).
        let parent_namespace = if self.current_scope_id != crate::ScopeId(0) {
            self.scopes
                .get(self.current_scope_id.0 as usize)
                .and_then(|scope| {
                    if scope.kind == crate::ContainerKind::Module {
                        // Look up the namespace symbol from the scope's container node.
                        self.node_symbols.get(&scope.container_node.0).copied()
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        self.semantic_defs.insert(
            sym_id,
            crate::state::SemanticDefEntry {
                kind,
                name: name.to_string(),
                file_id: self
                    .symbols
                    .get(sym_id)
                    .map_or(u32::MAX, |s| s.decl_file_idx),
                span_start: declaration.0,
                type_param_count,
                type_param_names,
                is_exported,
                enum_member_names,
                is_const,
                is_abstract,
                heritage_names,
                parent_namespace,
                is_global_augmentation: self.in_global_augmentation,
            },
        );
    }

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
