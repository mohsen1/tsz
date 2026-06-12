//! Binder declaration binding.
//!
//! Validation, diagnostics reporting, and resolution statistics live in
//! `validation.rs`.

use crate::state::FileFeatures;
use crate::{ContainerKind, FlowNodeId, SymbolId, SymbolTable, symbol_flags};
use std::sync::Arc;
use tsz_common::interner::AstAtom;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};

use crate::state::BinderState;

/// Named parameters for `record_semantic_def_ext` and `record_semantic_def_with_declare`.
#[derive(Default)]
pub(crate) struct SemanticDefDetails {
    pub type_param_count: u16,
    pub type_param_names: Vec<String>,
    pub is_exported: bool,
    pub enum_member_names: Vec<String>,
    pub is_const: bool,
    pub is_abstract: bool,
    pub is_declare: bool,
    pub extends_names: Vec<String>,
    pub implements_names: Vec<String>,
}

impl BinderState {
    fn identifier_atom(arena: &NodeArena, index: NodeIndex) -> Option<AstAtom> {
        arena
            .get_identifier_at(index)
            .and_then(|ident| (ident.atom != AstAtom::NONE).then_some(ident.atom))
    }

    /// Append a `(name, declaration)` entry to the `module_augmentations`
    /// table for the given target module specifier.
    pub(crate) fn record_module_augmentation_entry(
        &mut self,
        module_spec: &str,
        name: &str,
        declaration: NodeIndex,
    ) {
        Arc::make_mut(&mut self.module_augmentations)
            .entry(module_spec.to_string())
            .or_default()
            .push(crate::state::ModuleAugmentation::new(
                name.to_string(),
                declaration,
            ));
    }

    /// Allocate or extend the augmentation-local symbol for a declaration inside
    /// `declare module "<module_spec>" { ... }`.
    ///
    /// Augmentation declarations must never merge into a non-augmentation symbol
    /// of the same name at file scope (issue #6164). Within one file, repeated
    /// declarations of the same name across one or more `declare module
    /// "<same-target>"` blocks merge with each other.
    pub(crate) fn declare_module_augmentation_symbol(
        &mut self,
        arena: &NodeArena,
        module_spec: &str,
        name: &str,
        flags: u32,
        declaration: NodeIndex,
        is_exported: bool,
    ) -> SymbolId {
        self.record_module_augmentation_entry(module_spec, name, declaration);

        let span = Self::declaration_span(arena, declaration);
        let key = (module_spec.to_string(), name.to_string());
        let sym_id = if let Some(&existing) = self.module_augmentation_symbols.get(&key) {
            if let Some(sym) = self.symbols.get_mut(existing) {
                sym.flags |= flags;
                sym.add_declaration(declaration, span);
                if is_exported {
                    sym.is_exported = true;
                }
            }
            existing
        } else {
            let new_sym_id = self.symbols.alloc(flags, key.1.clone());
            if let Some(sym) = self.symbols.get_mut(new_sym_id) {
                sym.add_declaration(declaration, span);
                sym.is_exported = is_exported;
            }
            Arc::make_mut(&mut self.augmentation_target_modules).insert(new_sym_id, key.0.clone());
            self.module_augmentation_symbols.insert(key, new_sym_id);
            new_sym_id
        };
        Arc::make_mut(&mut self.node_symbols).insert(declaration.0, sym_id);
        sym_id
    }

    /// Register a value-producing declaration that appears inside a
    /// `declare global { ... }` block as a global augmentation.
    ///
    /// Variables, functions, classes, and enums declared inside `declare global`
    /// in an external module contribute a value binding to the global scope. They
    /// must be hoisted to `file_locals` (the gateway for cross-file visibility)
    /// and recorded in `global_augmentations` so cross-file resolution can find
    /// them; otherwise a bare reference reports a false `TS2304`.
    fn record_global_value_augmentation(
        &mut self,
        name: &str,
        sym_id: SymbolId,
        decl: NodeIndex,
        flags: u32,
    ) {
        self.file_locals.set(name.to_string(), sym_id);
        Arc::make_mut(&mut self.global_augmentations)
            .entry(name.to_string())
            .or_default()
            .push(crate::state::GlobalAugmentation::new(decl, flags));
    }

    // Declaration binding methods

    pub(crate) fn bind_variable_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(decl) = arena.get_variable_declaration(node) {
            let mut decl_flags = u32::from(node.flags);
            if !node_flags::is_let_or_const(decl_flags)
                && let Some(ext) = arena.get_extended(idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                decl_flags |= u32::from(parent_node.flags);
            }
            let is_block_scoped = node_flags::is_let_or_const(decl_flags);
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
                    Arc::make_mut(&mut self.module_augmentations)
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                // Track variable declarations inside `declare global { }` blocks
                // as global augmentations, just like interfaces and namespaces.
                // This enables cross-file conflict detection with UMD exports.
                if self.in_global_augmentation {
                    Arc::make_mut(&mut self.global_augmentations)
                        .entry(name.to_string())
                        .or_default()
                        .push(crate::state::GlobalAugmentation::new(idx, flags));
                }

                let sym_id = self.declare_symbol_with_atom(
                    arena,
                    name,
                    Self::identifier_atom(arena, decl.name),
                    flags,
                    idx,
                    is_exported,
                );
                Arc::make_mut(&mut self.node_symbols).insert(decl.name.0, sym_id);
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
                    self.record_global_value_augmentation(name, sym_id, idx, flags);
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
                        let sym_id =
                            self.declare_symbol(arena, name, flags, ident_idx, is_exported);
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
                            self.record_global_value_augmentation(name, sym_id, ident_idx, flags);
                        }
                    }
                }
                // Walk binding element initializers so that nested functions
                // (e.g. arrow functions used as default values in destructuring)
                // get their scopes and parameter symbols created.
                self.bind_binding_element_initializers(arena, decl.name);
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
                    Arc::make_mut(&mut self.module_augmentations)
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                let sym_id = self.declare_symbol_with_atom(
                    arena,
                    name,
                    Self::identifier_atom(arena, func.name),
                    symbol_flags::FUNCTION,
                    idx,
                    is_exported,
                );
                if self.in_global_augmentation {
                    self.record_global_value_augmentation(
                        name,
                        sym_id,
                        idx,
                        symbol_flags::FUNCTION,
                    );
                }
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
            self.declare_arguments_symbol(arena);

            // Bind type parameters
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            self.with_fresh_flow(|binder| {
                binder.bind_function_body_parts(arena, &func.parameters, func.body);
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
                let sym_id = self.declare_symbol_with_atom(
                    arena,
                    name,
                    Self::identifier_atom(arena, param.name),
                    symbol_flags::FUNCTION_SCOPED_VARIABLE,
                    idx,
                    false,
                );
                Arc::make_mut(&mut self.node_symbols).insert(param.name.0, sym_id);
                tracing::debug!(param_name = %name, sym_id = sym_id.0, "Parameter bound");
            } else {
                let mut names = Vec::new();
                Self::collect_binding_identifiers(arena, param.name, &mut names);
                for ident_idx in names {
                    if let Some(name) = Self::get_identifier_name(arena, ident_idx) {
                        self.declare_symbol(
                            arena,
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
            self.declare_symbol_with_atom(
                arena,
                name,
                Self::identifier_atom(arena, param.name),
                flags,
                param_idx,
                false,
            );
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
                    let sym_id = self.declare_symbol_with_atom(
                        arena,
                        name,
                        Self::identifier_atom(arena, type_param.name),
                        symbol_flags::TYPE_PARAMETER,
                        param_idx,
                        false,
                    );
                    Arc::make_mut(&mut self.node_symbols).insert(type_param.name.0, sym_id);
                }
            }
        }
    }

    /// Bind the shared function-body template: parameters, hoisted
    /// declarations, then the body itself.
    ///
    /// Hoisting collects `var` and function declarations from the function
    /// body before binding it, so declarations are accessible throughout the
    /// function scope before their actual declaration point (JavaScript
    /// hoisting behavior). Function declarations in blocks are block-scoped
    /// in strict mode and external modules; in non-strict scripts they hoist
    /// (Annex B).
    ///
    /// Statement order is load-bearing for flow-graph construction; every
    /// call site previously inlined this exact sequence.
    fn bind_function_body_parts(
        &mut self,
        arena: &NodeArena,
        parameters: &NodeList,
        body: NodeIndex,
    ) {
        for &param_idx in &parameters.nodes {
            self.bind_parameter(arena, param_idx);
        }
        self.collect_hoisted_from_node(arena, body);
        self.process_hoisted_functions(arena);
        self.process_hoisted_vars(arena);
        self.bind_node(arena, body);
    }

    /// Bind an IIFE body inline in the outer flow context (no `FlowStart`
    /// node). This preserves narrowing from the outer scope and propagates
    /// assignments inside the IIFE to the outer scope's control flow.
    ///
    /// Return statements are redirected to a fresh branch label; after the
    /// body is bound, the fall-through flow merges into that label and the
    /// label is finalized the way tsc's `finishFlowLabel` does.
    ///
    /// Ordering invariant: the fall-through antecedent is added before the
    /// return label is popped from `return_targets`.
    fn bind_iife_body(&mut self, arena: &NodeArena, parameters: &NodeList, body: NodeIndex) {
        let return_label = self.create_branch_label();
        self.return_targets.push(return_label);

        self.bind_function_body_parts(arena, parameters, body);

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
                tracing::debug!(
                    param_count = func.parameters.nodes.len(),
                    "Binding arrow IIFE parameters"
                );
                self.bind_iife_body(arena, &func.parameters, func.body);
            } else {
                // Non-IIFE: isolated flow scope
                self.with_fresh_flow_inner(
                    |binder| {
                        tracing::debug!(
                            param_count = func.parameters.nodes.len(),
                            "Binding arrow function parameters"
                        );
                        binder.bind_function_body_parts(arena, &func.parameters, func.body);
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
            self.declare_arguments_symbol(arena);

            // Named function expressions bind their name in their own scope
            // (accessible only inside the function body, not in the parent scope)
            if let Some(name) = Self::get_identifier_name(arena, func.name) {
                self.declare_symbol(arena, name, symbol_flags::FUNCTION, idx, false);
            }

            // Bind type parameters
            self.bind_type_parameters(arena, func.type_parameters.as_ref());

            if is_iife {
                self.bind_iife_body(arena, &func.parameters, func.body);
            } else {
                // Non-IIFE: isolated flow scope with captured enclosing flow
                self.with_fresh_flow_inner(
                    |binder| {
                        binder.bind_function_body_parts(arena, &func.parameters, func.body);
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
        self.declare_arguments_symbol(arena);

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

                // Hoisting: Collect var and function declarations from the body
                // before binding. This ensures `var` declarations merge with
                // same-named parameters (JavaScript hoisting behavior), preventing
                // false TS7022 circularity when the initializer references the
                // parameter (e.g., `constructor(x?) { var x = (x || 0); }`).
                if body.is_some() {
                    binder.collect_hoisted_from_node(arena, body);
                    binder.process_hoisted_functions(arena);
                    binder.process_hoisted_vars(arena);
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

    pub(crate) fn declare_arguments_symbol(&mut self, arena: &NodeArena) {
        self.declare_symbol(
            arena,
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

                // Check modifiers once, reuse results
                let is_abstract = Self::has_abstract_modifier(arena, class.modifiers.as_ref());
                if is_abstract {
                    flags |= symbol_flags::ABSTRACT;
                }

                // Check if exported BEFORE allocating symbol
                let is_exported = Self::has_export_modifier(arena, class.modifiers.as_ref());

                if self.in_module_augmentation
                    && let Some(ref module_spec) = self.current_augmented_module
                {
                    Arc::make_mut(&mut self.module_augmentations)
                        .entry(module_spec.clone())
                        .or_default()
                        .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
                }

                let sym_id = self.declare_symbol(arena, name, flags, idx, is_exported);
                let tp_count = class
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, class.type_parameters.as_ref());
                let (extends_names, implements_names) = Self::collect_heritage_clause_names_split(
                    arena,
                    class.heritage_clauses.as_ref(),
                );
                let is_declare = Self::has_declare_modifier(arena, class.modifiers.as_ref());
                self.record_semantic_def_ext(
                    sym_id,
                    crate::state::SemanticDefKind::Class,
                    name,
                    idx,
                    SemanticDefDetails {
                        type_param_count: tp_count,
                        type_param_names: tp_names,
                        is_exported,
                        is_abstract,
                        is_declare,
                        extends_names,
                        implements_names,
                        ..Default::default()
                    },
                );

                // Track class declarations inside `declare global { }` blocks as
                // global augmentations, just like variables, functions, interfaces,
                // and namespaces. Without this, `declare global { class C { } }`
                // declared in an external module's `.d.ts` is invisible to
                // cross-file global resolution, so a bare reference like `new C()`
                // reports a false TS2304.
                if self.in_global_augmentation {
                    self.record_global_value_augmentation(name, sym_id, idx, flags);
                }
            }

            // Enter class scope for members, pre-sized for member count to avoid hash map resizing
            let member_capacity = class.members.nodes.len();
            self.enter_scope_with_capacity(ContainerKind::Class, idx, member_capacity);

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
            let member_capacity = class.members.nodes.len();
            self.enter_scope_with_capacity(ContainerKind::Class, idx, member_capacity);

            if let Some(name) = Self::get_identifier_name(arena, class.name) {
                let mut flags = symbol_flags::CLASS;
                if Self::has_abstract_modifier(arena, class.modifiers.as_ref()) {
                    flags |= symbol_flags::ABSTRACT;
                }
                let sym_id = self.declare_symbol(arena, name, flags, idx, false);
                Arc::make_mut(&mut self.node_symbols).insert(class.name.0, sym_id);
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
                    let span = arena.get(idx).map(|node| (node.pos, node.end));
                    sym.add_declaration(idx, span);
                    sym.set_value_declaration(idx, span);
                }
                Arc::make_mut(&mut self.node_symbols).insert(idx.0, sym_id);
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
        let Some(node) = arena.get(idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = arena.get_method_decl(node) else {
                    return;
                };
                self.bind_modifiers(arena, method.modifiers.as_ref());
                if let Some(name_node) = arena.get(method.name)
                    && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    self.bind_node(arena, method.name);
                }
                if let Some(name) = Self::get_property_name(arena, method.name) {
                    // Single-pass modifier extraction avoids 3 separate list walks
                    let flags = symbol_flags::METHOD
                        | Self::extract_member_modifier_flags(arena, method.modifiers.as_ref());
                    let sym_id = self.declare_symbol(arena, &name, flags, idx, false);
                    Arc::make_mut(&mut self.node_symbols).insert(method.name.0, sym_id);
                }
                self.bind_callable_body_with_type_params(
                    arena,
                    &method.parameters,
                    method.body,
                    idx,
                    method.type_parameters.as_ref(),
                );
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = arena.get_property_decl(node) else {
                    return;
                };
                self.bind_modifiers(arena, prop.modifiers.as_ref());
                if let Some(name_node) = arena.get(prop.name)
                    && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    self.bind_node(arena, prop.name);
                }
                if let Some(name) = Self::get_property_name(arena, prop.name) {
                    // Single-pass modifier extraction avoids 3 separate list walks
                    let flags = symbol_flags::PROPERTY
                        | Self::extract_member_modifier_flags(arena, prop.modifiers.as_ref());
                    let sym_id = self.declare_symbol(arena, &name, flags, idx, false);
                    Arc::make_mut(&mut self.node_symbols).insert(prop.name.0, sym_id);
                }

                if prop.initializer.is_some() {
                    self.bind_node(arena, prop.initializer);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let Some(accessor) = arena.get_accessor(node) else {
                    return;
                };
                self.bind_modifiers(arena, accessor.modifiers.as_ref());
                if let Some(name_node) = arena.get(accessor.name)
                    && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    self.bind_node(arena, accessor.name);
                }
                if let Some(name) = Self::get_property_name(arena, accessor.name) {
                    // Single-pass modifier extraction avoids 3 separate list walks
                    let base_flags = if node.kind == syntax_kind_ext::GET_ACCESSOR {
                        symbol_flags::GET_ACCESSOR
                    } else {
                        symbol_flags::SET_ACCESSOR
                    };
                    let flags = base_flags
                        | Self::extract_member_modifier_flags(arena, accessor.modifiers.as_ref());
                    let sym_id = self.declare_symbol(arena, &name, flags, idx, false);
                    Arc::make_mut(&mut self.node_symbols).insert(accessor.name.0, sym_id);
                }
                self.bind_callable_body(arena, &accessor.parameters, accessor.body, idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.declare_symbol(arena, "constructor", symbol_flags::CONSTRUCTOR, idx, false);
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
                Arc::make_mut(&mut self.global_augmentations)
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(
                        idx,
                        symbol_flags::INTERFACE,
                    ));
            }

            // In script files (non-module files), top-level interface declarations that
            // collide with a same-named lib symbol augment the lib's global interface.
            // TypeScript allows `interface Array<T> { ... }` (or `interface Node { ... }`)
            // in scripts without `declare global`. The hardcoded `is_built_in_global_type`
            // allow-list is kept as a fast path; the additional `lib_symbol_ids` check
            // covers DOM/WebWorker/etc. globals that aren't in the static list.
            if !self.in_global_augmentation
                && self.is_global_scope()
                && !self.is_external_module
                && (Self::is_built_in_global_type(name) || self.name_collides_with_lib_symbol(name))
            {
                Arc::make_mut(&mut self.global_augmentations)
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(
                        idx,
                        symbol_flags::INTERFACE,
                    ));
            }

            // Rule #44: augmentation interfaces always bind to a separate `SymbolId`
            // that is independent of any non-augmentation file-scope symbol of the
            // same name (issue #6164). Within one file, repeated declarations of the
            // same name across one or more `declare module "<same-target>" { ... }`
            // blocks merge with each other through
            // `declare_module_augmentation_symbol`.
            if self.in_module_augmentation
                && let Some(module_spec) = self.current_augmented_module.clone()
            {
                // If the name already exists as an import alias in the current scope,
                // only record the augmentation entry — declaring our own symbol would
                // contaminate the import alias's type. The augmentation is still
                // applied to the import alias at type-resolution time through the
                // alias's apply-augmentations path.
                let name_conflicts_with_import = self
                    .current_scope
                    .get(name)
                    .and_then(|sym_id| self.symbols.get(sym_id))
                    .is_some_and(|sym| sym.import_module.is_some());
                if name_conflicts_with_import {
                    self.record_module_augmentation_entry(&module_spec, name, idx);
                    return;
                }

                let aug_sym_id = self.declare_module_augmentation_symbol(
                    arena,
                    &module_spec,
                    name,
                    symbol_flags::INTERFACE,
                    idx,
                    is_exported,
                );
                let tp_count = iface
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, iface.type_parameters.as_ref());
                let (extends_names, implements_names) = Self::collect_heritage_clause_names_split(
                    arena,
                    iface.heritage_clauses.as_ref(),
                );
                let is_declare = Self::has_declare_modifier(arena, iface.modifiers.as_ref());
                self.record_semantic_def_ext(
                    aug_sym_id,
                    crate::state::SemanticDefKind::Interface,
                    name,
                    idx,
                    SemanticDefDetails {
                        type_param_count: tp_count,
                        type_param_names: tp_names,
                        is_exported,
                        is_declare,
                        extends_names,
                        implements_names,
                        ..Default::default()
                    },
                );
                return;
            }

            let sym_id = self.declare_symbol_with_atom(
                arena,
                name,
                Self::identifier_atom(arena, iface.name),
                symbol_flags::INTERFACE,
                idx,
                is_exported,
            );
            let tp_count = iface
                .type_parameters
                .as_ref()
                .map_or(0, |tp| tp.nodes.len() as u16);
            let tp_names = Self::collect_type_param_names(arena, iface.type_parameters.as_ref());
            let (extends_names, implements_names) =
                Self::collect_heritage_clause_names_split(arena, iface.heritage_clauses.as_ref());
            let is_declare = Self::has_declare_modifier(arena, iface.modifiers.as_ref());
            self.record_semantic_def_ext(
                sym_id,
                crate::state::SemanticDefKind::Interface,
                name,
                idx,
                SemanticDefDetails {
                    type_param_count: tp_count,
                    type_param_names: tp_names,
                    is_exported,
                    is_declare,
                    extends_names,
                    implements_names,
                    ..Default::default()
                },
            );

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
            // that should merge with lib.d.ts symbols at type resolution time.
            //
            // Skip when the type alias is nested inside a named non-global namespace
            // (e.g. `declare global { namespace JSX { type Element = any; } }`).
            // Those are namespace members (`JSX.Element`), not augmentations of a
            // top-level global type — recording them by their bare name corrupts
            // lib types that share the name. For example, lib.dom.d.ts's
            // `interface Element` constraint check on
            // `NodeListOf<HTMLElementTagNameMap[K]>` would otherwise emit
            // spurious TS2344. Type aliases inside a namespace can never
            // participate in global interface merging anyway.
            if self.in_global_augmentation && !Self::is_inside_namespace(arena, idx) {
                Arc::make_mut(&mut self.global_augmentations)
                    .entry(name.to_string())
                    .or_default()
                    .push(crate::state::GlobalAugmentation::new(
                        idx,
                        symbol_flags::TYPE_ALIAS,
                    ));
            }

            // Rule #44: augmentation type aliases bind to a separate `SymbolId`
            // independent of any non-augmentation file-scope symbol of the same name
            // (issue #6164). Same-target augmentations within a file merge with each
            // other through `declare_module_augmentation_symbol`.
            if self.in_module_augmentation
                && let Some(module_spec) = self.current_augmented_module.clone()
            {
                let aug_sym_id = self.declare_module_augmentation_symbol(
                    arena,
                    &module_spec,
                    name,
                    symbol_flags::TYPE_ALIAS,
                    idx,
                    is_exported,
                );
                let tp_count = alias
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, alias.type_parameters.as_ref());
                let is_declare = Self::has_declare_modifier(arena, alias.modifiers.as_ref());
                self.record_semantic_def_with_declare(
                    aug_sym_id,
                    crate::state::SemanticDefKind::TypeAlias,
                    name,
                    idx,
                    SemanticDefDetails {
                        type_param_count: tp_count,
                        type_param_names: tp_names,
                        is_exported,
                        is_declare,
                        ..Default::default()
                    },
                );

                self.enter_scope(ContainerKind::Block, idx);
                self.bind_type_parameters(arena, alias.type_parameters.as_ref());
                self.exit_scope(arena);
                return;
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
                    sym.add_declaration(idx, arena.get(idx).map(|node| (node.pos, node.end)));
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
                Arc::make_mut(&mut self.node_symbols).insert(idx.0, sym_id);
                self.declare_in_persistent_scope(name.to_string(), sym_id);
                // Record partnership: TYPE_ALIAS → ALIAS
                Arc::make_mut(&mut self.alias_partners).insert(sym_id, alias_id);
                let tp_count = alias
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, alias.type_parameters.as_ref());
                let is_declare = Self::has_declare_modifier(arena, alias.modifiers.as_ref());
                self.record_semantic_def_with_declare(
                    sym_id,
                    crate::state::SemanticDefKind::TypeAlias,
                    name,
                    idx,
                    SemanticDefDetails {
                        type_param_count: tp_count,
                        type_param_names: tp_names,
                        is_exported,
                        is_declare,
                        ..Default::default()
                    },
                );
            } else {
                let sym_id = self.declare_symbol_with_atom(
                    arena,
                    name,
                    Self::identifier_atom(arena, alias.name),
                    symbol_flags::TYPE_ALIAS,
                    idx,
                    is_exported,
                );
                let tp_count = alias
                    .type_parameters
                    .as_ref()
                    .map_or(0, |tp| tp.nodes.len() as u16);
                let tp_names =
                    Self::collect_type_param_names(arena, alias.type_parameters.as_ref());
                let is_declare = Self::has_declare_modifier(arena, alias.modifiers.as_ref());
                self.record_semantic_def_with_declare(
                    sym_id,
                    crate::state::SemanticDefKind::TypeAlias,
                    name,
                    idx,
                    SemanticDefDetails {
                        type_param_count: tp_count,
                        type_param_names: tp_names,
                        is_exported,
                        is_declare,
                        ..Default::default()
                    },
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
                Arc::make_mut(&mut self.module_augmentations)
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

            let enum_sym_id = self.declare_symbol_with_atom(
                arena,
                name,
                Self::identifier_atom(arena, enum_decl.name),
                enum_flags,
                idx,
                is_exported,
            );

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

            let is_declare = Self::has_declare_modifier(arena, enum_decl.modifiers.as_ref());
            self.record_semantic_def_ext(
                enum_sym_id,
                crate::state::SemanticDefKind::Enum,
                name,
                idx,
                SemanticDefDetails {
                    enum_member_names,
                    is_const,
                    is_exported,
                    is_declare,
                    ..Default::default()
                },
            );

            // Track enum declarations inside `declare global { }` blocks as global
            // augmentations, just like variables, functions, interfaces, and
            // namespaces. Without this, `declare global { const enum F { ... } }`
            // (and regular `enum`) declared in an external module's `.d.ts` is
            // invisible to cross-file global resolution, so a bare reference like
            // `F.A` reports a false TS2304 instead of resolving to the global enum.
            if self.in_global_augmentation {
                self.record_global_value_augmentation(name, enum_sym_id, idx, enum_flags);
            }

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
                        let span = arena.get(member_idx).map(|node| (node.pos, node.end));
                        sym.set_value_declaration(member_idx, span);
                        sym.add_declaration(member_idx, span);
                        sym.parent = enum_sym_id; // Set parent to the enum symbol
                    }
                    self.current_scope.set(member_name.to_string(), sym_id);
                    Arc::make_mut(&mut self.node_symbols).insert(member_idx.0, sym_id);
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

                        Arc::make_mut(&mut self.switch_clause_to_switch).insert(clause_idx.0, idx);

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

                        if Self::clause_allows_fallthrough(arena, clause) {
                            fallthrough_flow = self.current_flow;
                        } else {
                            fallthrough_flow = FlowNodeId::NONE;
                        }
                    }
                }

                // Add end_label antecedent once after all clauses (not per-clause).
                // Mirrors TypeScript binder: `addAntecedent(postSwitchLabel, currentFlow)`
                // is called after the CaseBlock loop. Break statements contribute via
                // `currentBreakTarget`; this handles fallthrough from the final clause.
                self.add_antecedent(end_label, self.current_flow);

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
}

impl Default for BinderState {
    fn default() -> Self {
        Self::new()
    }
}
