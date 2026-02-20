//! Module and namespace declaration binding.
//!
//! This module handles binding of module/namespace declarations, including
//! ambient modules, module augmentation, export population, and symbol visibility.

use crate::state::BinderState;
use crate::{ContainerKind, Symbol, SymbolId, SymbolTable, symbol_flags};
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl BinderState {
    /// Check if `idx` is nested inside an ambient module (one with `declare` or
    /// a string-literal name). Walks up through `MODULE_BLOCK` / `MODULE_DECLARATION`
    /// ancestors until it finds one that is ambient or reaches the source file.
    fn is_inside_ambient_module(arena: &NodeArena, idx: NodeIndex) -> bool {
        let mut current = idx;
        // Walk up through the AST looking for an ambient ancestor
        for _ in 0..32 {
            // limit depth to prevent infinite loop
            let Some(ext) = arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent_node) = arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(parent_module) = arena.get_module(parent_node)
            {
                // Check if this ancestor has `declare` modifier
                if Self::has_declare_modifier(arena, parent_module.modifiers.as_ref()) {
                    return true;
                }
                // Check if this ancestor has a string-literal name
                if let Some(name_node) = arena.get(parent_module.name)
                    && (name_node.kind == SyntaxKind::StringLiteral as u16
                        || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                {
                    return true;
                }
            }
            // Keep walking up
            current = parent_idx;
        }
        false
    }

    pub(crate) fn bind_module_declaration(
        &mut self,
        arena: &NodeArena,
        node: &Node,
        idx: NodeIndex,
    ) {
        if let Some(module) = arena.get_module(node) {
            if self.in_module_augmentation
                && let Some(ref module_spec) = self.current_augmented_module
                && let Some(name) = Self::get_identifier_name(arena, module.name)
            {
                self.module_augmentations
                    .entry(module_spec.clone())
                    .or_default()
                    .push(crate::state::ModuleAugmentation::new(name.to_string(), idx));
            }

            let is_global_augmentation = u32::from(node.flags) & node_flags::GLOBAL_AUGMENTATION
                != 0
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
                if module.body.is_some() {
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

            let mut declared_module_specifier: Option<String> = None;
            let mut is_augmentation = false;
            if let Some(name_node) = arena.get(module.name)
                && (name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
            {
                // Ambient module declaration with string literal name
                if let Some(lit) = arena.get_literal(name_node)
                    && !lit.text.is_empty()
                {
                    let module_specifier = lit.text.clone();
                    declared_module_specifier = Some(module_specifier.clone());

                    // Rule #44: Detect module augmentation
                    // A `declare module "x"` in an external module (file with imports/exports)
                    // is a module augmentation if it references an existing or external module.
                    is_augmentation = self.is_external_module
                        && self.is_potential_module_augmentation(&module_specifier);

                    if is_augmentation {
                        // Track as module augmentation - bind body with augmentation context
                        if module.body.is_none() {
                            // Shorthand ambient module: `declare module "*.json";` (no body)
                            // Even when classified as augmentation, a bodyless declaration
                            // is a shorthand that makes matching imports resolve to `any`.
                            self.shorthand_ambient_modules.insert(module_specifier);
                        } else {
                            self.node_scope_ids
                                .insert(module.body.0, self.current_scope_id);
                            let was_in_augmentation = self.in_module_augmentation;
                            let prev_module = self.current_augmented_module.take();
                            self.in_module_augmentation = true;
                            self.current_augmented_module = Some(module_specifier);
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

            let name = Self::get_identifier_name(arena, module.name)
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
                let mut is_exported = Self::has_export_modifier(arena, module.modifiers.as_ref());
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

                if self.in_global_augmentation {
                    self.global_augmentations
                        .entry(name.clone())
                        .or_default()
                        .push(crate::state::GlobalAugmentation::new(idx));
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
            if module.body.is_none() {
                // Shorthand ambient module declaration: `declare module "foo"` without body
                // Track this so imports from these modules are typed as `any`
                if let Some(name_node) = arena.get(module.name)
                    && let Some(lit) = arena.get_literal(name_node)
                    && !lit.text.is_empty()
                {
                    self.shorthand_ambient_modules.insert(lit.text.clone());
                }
            } else {
                self.node_scope_ids
                    .insert(module.body.0, self.current_scope_id);
            }

            self.bind_node(arena, module.body);

            // Populate exports for the module symbol
            if module_symbol_id.is_some() && module.body.is_some() {
                let mut is_ambient_module = !is_augmentation
                    && (declared_module_specifier.is_some()
                        || Self::has_declare_modifier(arena, module.modifiers.as_ref()));

                // Nested namespaces inside ambient contexts should treat declarations
                // as ambient-exported for symbol visibility. This covers:
                // - `declare module "x" { namespace N { ... } }` (external modules)
                // - `declare namespace A { namespace B { namespace C { ... } } }`
                // Walk up through all ancestors to find any ambient module.
                if !is_ambient_module && Self::is_inside_ambient_module(arena, idx) {
                    is_ambient_module = true;
                }
                self.populate_module_exports(
                    arena,
                    module.body,
                    module_symbol_id,
                    is_ambient_module,
                );
                if let Some(module_specifier) = declared_module_specifier.as_ref()
                    && let Some(symbol) = self.symbols.get(module_symbol_id)
                    && let Some(exports) = symbol.exports.as_ref()
                    && !exports.is_empty()
                {
                    self.module_exports
                        .insert(module_specifier.clone(), exports.as_ref().clone());
                }
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
        is_ambient_module: bool,
    ) {
        // Get the module block statements
        let statements = if let Some(module_block) = arena.get_module_block_at(body_idx) {
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
                let mut is_exported = match stmt_node.kind {
                    syntax_kind_ext::VARIABLE_STATEMENT => arena
                        .get_variable(stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::FUNCTION_DECLARATION => arena
                        .get_function(stmt_node)
                        .and_then(|f| f.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::CLASS_DECLARATION => arena
                        .get_class(stmt_node)
                        .and_then(|c| c.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::INTERFACE_DECLARATION => arena
                        .get_interface(stmt_node)
                        .and_then(|i| i.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => arena
                        .get_type_alias(stmt_node)
                        .and_then(|t| t.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::ENUM_DECLARATION => arena
                        .get_enum(stmt_node)
                        .and_then(|e| e.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::MODULE_DECLARATION => arena
                        .get_module(stmt_node)
                        .and_then(|m| m.modifiers.as_ref())
                        .is_some_and(|mods| Self::has_export_modifier_any(arena, mods)),
                    syntax_kind_ext::EXPORT_DECLARATION | syntax_kind_ext::EXPORT_ASSIGNMENT => {
                        true
                    }
                    _ => false,
                };
                if is_ambient_module {
                    is_exported = true;
                }

                if is_exported {
                    // Collect the exported names and direct symbol mappings first
                    let mut exported_names = Vec::new();
                    let mut exported_symbols: Vec<(String, SymbolId)> = Vec::new();
                    let mut collect_var_exports =
                        |var_stmt: &tsz_parser::parser::node::VariableData| {
                            for &list_idx in &var_stmt.declarations.nodes {
                                if let Some(list_node) = arena.get(list_idx)
                                    && let Some(decl_list) = arena.get_variable(list_node)
                                {
                                    for &decl_idx in &decl_list.declarations.nodes {
                                        if let Some(decl_node) = arena.get(decl_idx)
                                            && let Some(decl) =
                                                arena.get_variable_declaration(decl_node)
                                            && let Some(name_node) = arena.get(decl.name)
                                            && let Some(ident) = arena.get_identifier(name_node)
                                        {
                                            exported_names.push(ident.escaped_text.clone());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&decl.name.0)
                                            {
                                                exported_symbols
                                                    .push((ident.escaped_text.clone(), sym_id));
                                            }
                                        }
                                    }
                                } else if let Some(decl_node) = arena.get(list_idx)
                                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                                    && let Some(name_node) = arena.get(decl.name)
                                    && let Some(ident) = arena.get_identifier(name_node)
                                {
                                    exported_names.push(ident.escaped_text.clone());
                                    if let Some(&sym_id) = self.node_symbols.get(&decl.name.0) {
                                        exported_symbols.push((ident.escaped_text.clone(), sym_id));
                                    }
                                }
                            }
                        };

                    match stmt_node.kind {
                        syntax_kind_ext::VARIABLE_STATEMENT => {
                            if let Some(var_stmt) = arena.get_variable(stmt_node) {
                                collect_var_exports(var_stmt);
                            }
                        }
                        syntax_kind_ext::FUNCTION_DECLARATION => {
                            if let Some(func) = arena.get_function(stmt_node)
                                && let Some(name) = Self::get_identifier_name(arena, func.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::CLASS_DECLARATION => {
                            if let Some(class) = arena.get_class(stmt_node)
                                && let Some(name) = Self::get_identifier_name(arena, class.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::ENUM_DECLARATION => {
                            if let Some(enm) = arena.get_enum(stmt_node)
                                && let Some(name) = Self::get_identifier_name(arena, enm.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::INTERFACE_DECLARATION => {
                            if let Some(iface) = arena.get_interface(stmt_node)
                                && let Some(name) = Self::get_identifier_name(arena, iface.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                            if let Some(alias) = arena.get_type_alias(stmt_node)
                                && let Some(name) = Self::get_identifier_name(arena, alias.name)
                            {
                                exported_names.push(name.to_string());
                            }
                        }
                        syntax_kind_ext::MODULE_DECLARATION => {
                            if let Some(module) = arena.get_module(stmt_node) {
                                let name = Self::get_identifier_name(arena, module.name)
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
                        syntax_kind_ext::EXPORT_ASSIGNMENT => {
                            if let Some(assign) = arena.get_export_assignment(stmt_node)
                                && let Some(target_name) =
                                    Self::get_identifier_name(arena, assign.expression)
                                && let Some(sym_id) = self
                                    .current_scope
                                    .get(target_name)
                                    .or_else(|| self.file_locals.get(target_name))
                            {
                                exported_symbols.push(("export=".to_string(), sym_id));

                                // Also expose members of the export-assignment target for
                                // named import compatibility (e.g. `export = alias; import { f }`).
                                let mut target_sym_id = sym_id;

                                if let Some(target_sym) = self.symbols.get(sym_id)
                                    && (target_sym.flags & symbol_flags::ALIAS) != 0
                                {
                                    let decl_idx = if target_sym.value_declaration.is_none() {
                                        target_sym
                                            .declarations
                                            .first()
                                            .copied()
                                            .unwrap_or(NodeIndex::NONE)
                                    } else {
                                        target_sym.value_declaration
                                    };

                                    if decl_idx.is_some()
                                        && let Some(decl_node) = arena.get(decl_idx)
                                        && decl_node.kind
                                            == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                        && let Some(import_decl) = arena.get_import_decl(decl_node)
                                    {
                                        let module_ref = import_decl.module_specifier;
                                        if let Some(module_ref_node) = arena.get(module_ref)
                                            && module_ref_node.kind
                                                != SyntaxKind::StringLiteral as u16
                                            && let Some(ref_name) =
                                                Self::get_identifier_name(arena, module_ref)
                                            && let Some(resolved) = self
                                                .current_scope
                                                .get(ref_name)
                                                .or_else(|| self.file_locals.get(ref_name))
                                        {
                                            target_sym_id = resolved;
                                        }
                                    }
                                }

                                if let Some(target_symbol) = self.symbols.get(target_sym_id) {
                                    let collect_members_from_symbol =
                                        |symbol: &Symbol,
                                         exported_symbols: &mut Vec<(String, SymbolId)>| {
                                            if let Some(exports) = symbol.exports.as_ref() {
                                                for (export_name, &export_sym_id) in exports.iter() {
                                                    if export_name != "export=" {
                                                        exported_symbols.push((
                                                            export_name.clone(),
                                                            export_sym_id,
                                                        ));
                                                    }
                                                }
                                            }
                                            if let Some(members) = symbol.members.as_ref() {
                                                for (member_name, &member_sym_id) in members.iter() {
                                                    exported_symbols
                                                        .push((member_name.clone(), member_sym_id));
                                                }
                                            }
                                        };

                                    collect_members_from_symbol(
                                        target_symbol,
                                        &mut exported_symbols,
                                    );

                                    // Some declaration patterns keep value and namespace halves in
                                    // sibling symbols with the same name (e.g. function + namespace).
                                    // Include namespace-shaped siblings so `export = X` exposes all
                                    // merged members for named import compatibility.
                                    for candidate_id in
                                        self.symbols.find_all_by_name(&target_symbol.escaped_name)
                                    {
                                        if candidate_id == target_sym_id {
                                            continue;
                                        }
                                        let Some(candidate_symbol) = self.symbols.get(candidate_id)
                                        else {
                                            continue;
                                        };
                                        if (candidate_symbol.flags
                                            & (symbol_flags::MODULE
                                                | symbol_flags::NAMESPACE_MODULE
                                                | symbol_flags::VALUE_MODULE))
                                            == 0
                                        {
                                            continue;
                                        }
                                        collect_members_from_symbol(
                                            candidate_symbol,
                                            &mut exported_symbols,
                                        );
                                    }
                                }
                            }
                        }
                        syntax_kind_ext::EXPORT_DECLARATION => {
                            if let Some(export_decl) = arena.get_export_decl(stmt_node)
                                && export_decl.export_clause.is_some()
                                && let Some(clause_node) = arena.get(export_decl.export_clause)
                            {
                                match clause_node.kind {
                                    syntax_kind_ext::VARIABLE_STATEMENT => {
                                        if let Some(var_stmt) = arena.get_variable(clause_node) {
                                            collect_var_exports(var_stmt);
                                        }
                                    }
                                    syntax_kind_ext::FUNCTION_DECLARATION => {
                                        if let Some(func) = arena.get_function(clause_node)
                                            && let Some(name) =
                                                Self::get_identifier_name(arena, func.name)
                                        {
                                            exported_names.push(name.to_string());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&export_decl.export_clause.0)
                                            {
                                                exported_symbols.push((name.to_string(), sym_id));
                                            }
                                        }
                                    }
                                    syntax_kind_ext::CLASS_DECLARATION => {
                                        if let Some(class) = arena.get_class(clause_node)
                                            && let Some(name) =
                                                Self::get_identifier_name(arena, class.name)
                                        {
                                            exported_names.push(name.to_string());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&export_decl.export_clause.0)
                                            {
                                                exported_symbols.push((name.to_string(), sym_id));
                                            }
                                        }
                                    }
                                    syntax_kind_ext::ENUM_DECLARATION => {
                                        if let Some(enm) = arena.get_enum(clause_node)
                                            && let Some(name) =
                                                Self::get_identifier_name(arena, enm.name)
                                        {
                                            exported_names.push(name.to_string());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&export_decl.export_clause.0)
                                            {
                                                exported_symbols.push((name.to_string(), sym_id));
                                            }
                                        }
                                    }
                                    syntax_kind_ext::INTERFACE_DECLARATION => {
                                        if let Some(iface) = arena.get_interface(clause_node)
                                            && let Some(name) =
                                                Self::get_identifier_name(arena, iface.name)
                                        {
                                            exported_names.push(name.to_string());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&export_decl.export_clause.0)
                                            {
                                                exported_symbols.push((name.to_string(), sym_id));
                                            }
                                        }
                                    }
                                    syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                                        if let Some(alias) = arena.get_type_alias(clause_node)
                                            && let Some(name) =
                                                Self::get_identifier_name(arena, alias.name)
                                        {
                                            exported_names.push(name.to_string());
                                            if let Some(&sym_id) =
                                                self.node_symbols.get(&export_decl.export_clause.0)
                                            {
                                                exported_symbols.push((name.to_string(), sym_id));
                                            }
                                        }
                                    }
                                    syntax_kind_ext::MODULE_DECLARATION => {
                                        if let Some(module) = arena.get_module(clause_node) {
                                            let name =
                                                Self::get_identifier_name(arena, module.name)
                                                    .map(str::to_string)
                                                    .or_else(|| {
                                                        arena
                                                            .get(module.name)
                                                            .and_then(|name_node| {
                                                                arena.get_literal(name_node)
                                                            })
                                                            .map(|lit| lit.text.clone())
                                                    });
                                            if let Some(name) = name {
                                                exported_names.push(name.clone());
                                                if let Some(&sym_id) = self
                                                    .node_symbols
                                                    .get(&export_decl.export_clause.0)
                                                {
                                                    exported_symbols.push((name.clone(), sym_id));
                                                }
                                            }
                                        }
                                    }
                                    syntax_kind_ext::NAMED_EXPORTS => {
                                        if let Some(named_exports) =
                                            arena.get_named_imports(clause_node)
                                        {
                                            for &specifier_idx in &named_exports.elements.nodes {
                                                if let Some(spec_node) = arena.get(specifier_idx)
                                                    && let Some(spec) =
                                                        arena.get_specifier(spec_node)
                                                {
                                                    let name_idx = if spec.name.is_none() {
                                                        spec.property_name
                                                    } else {
                                                        spec.name
                                                    };
                                                    if let Some(name_node) = arena.get(name_idx)
                                                        && let Some(ident) =
                                                            arena.get_identifier(name_node)
                                                    {
                                                        exported_names
                                                            .push(ident.escaped_text.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }

                    if is_ambient_module {
                        let in_scope: Vec<String> = exported_names
                            .iter()
                            .filter(|name| {
                                self.current_scope.has(name.as_str())
                                    || self.file_locals.has(name.as_str())
                            })
                            .cloned()
                            .collect();
                        let module_name = self
                            .symbols
                            .get(module_symbol_id)
                            .map_or("<unknown>", |sym| sym.escaped_name.as_str());
                        tracing::debug!(
                            module_name,
                            exported_names = ?exported_names,
                            in_scope = ?in_scope,
                            "Ambient module export candidates"
                        );
                    }

                    // Now add them to exports
                    for (name, sym_id) in &exported_symbols {
                        if let Some(module_sym) = self.symbols.get_mut(module_symbol_id) {
                            let exports = module_sym
                                .exports
                                .get_or_insert_with(|| Box::new(SymbolTable::new()));
                            exports.set(name.clone(), *sym_id);
                        }
                        if let Some(child_sym) = self.symbols.get_mut(*sym_id) {
                            child_sym.is_exported = true;
                        }
                    }
                    for name in &exported_names {
                        if exported_symbols.iter().any(|(n, _)| n == name) {
                            continue;
                        }
                        if let Some(sym_id) = self
                            .current_scope
                            .get(name)
                            .or_else(|| self.file_locals.get(name))
                        {
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

    /// Check if any modifier in a `NodeList` is the export keyword.
    pub(crate) fn has_export_modifier_any(arena: &NodeArena, modifiers: &NodeList) -> bool {
        for &mod_idx in &modifiers.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == SyntaxKind::ExportKeyword as u16
            {
                return true;
            }
        }
        false
    }
}
