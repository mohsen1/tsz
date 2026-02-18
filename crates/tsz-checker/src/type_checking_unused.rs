//! Unused declaration checking (TS6133, TS6192) and overload resolution helpers.
//!
//! Split from `type_checking_global.rs` to keep file sizes manageable.
//! Contains:
//! - Unused declaration checking (variables, functions, classes, imports)
//! - Import/variable/binding parent node traversal helpers
//! - Constructor/method/function implementation finders

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check for unused declarations (TS6133, TS6192).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    /// Also reports import declarations where ALL imports are unused (TS6192).
    pub(crate) fn check_unused_declarations(&mut self) {
        use crate::diagnostics::Diagnostic;
        use std::collections::{HashMap, HashSet};
        use tsz_binder::ContainerKind;
        use tsz_binder::symbol_flags;

        let check_locals = self.ctx.no_unused_locals();
        let check_params = self.ctx.no_unused_parameters();
        let is_module = self.ctx.binder.is_external_module();

        // Skip .d.ts files entirely (ambient declarations)
        if self.ctx.file_name.ends_with(".d.ts") {
            return;
        }

        // Collect symbols from scopes.
        // For script files (non-module), skip the root SourceFile scope since
        // top-level declarations are globals and not checked by noUnusedLocals.
        // For module files, check all scopes including root.
        let mut symbols_to_check: Vec<(tsz_binder::SymbolId, String)> = Vec::new();

        for scope in &self.ctx.binder.scopes {
            // Skip root scope in script files
            if !is_module && scope.kind == ContainerKind::SourceFile {
                continue;
            }
            for (name, &sym_id) in scope.table.iter() {
                // Skip lib-originating symbols (e.g. from lib.d.ts)
                if self.ctx.binder.lib_symbol_ids.contains(&sym_id) {
                    continue;
                }
                symbols_to_check.push((sym_id, name.clone()));
            }
        }

        let file_name = self.ctx.file_name.clone();

        // Track import declarations for TS6192.
        // Map from import declaration NodeIndex to (total_count, unused_count).
        let mut import_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track variable declarations for TS6199.
        // Map from variable declaration NodeIndex to (total_count, unused_count).
        let mut variable_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track destructuring patterns for TS6198.
        // Map from binding pattern NodeIndex to (total_elements, unused_elements).
        let destructuring_patterns: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // First pass: identify ALL import symbols and track them by import declaration.
        // This includes both used and unused imports.
        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track ALIAS symbols (imports)
            if (flags & symbol_flags::ALIAS) == 0 {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Find the parent IMPORT_DECLARATION node
            if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx) {
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                let entry = import_declarations.entry(import_decl_idx).or_insert((0, 0));
                entry.0 += 1; // total count
                if !is_used {
                    entry.1 += 1; // unused count
                }
            }
        }

        // Second pass: track variable declarations (for TS6199)
        // We need to track VARIABLE_DECLARATION nodes (not individual variables)
        // to distinguish `var x, y;` (2 decls) from `const {a, b} = obj;` (1 decl with multiple bindings)
        let mut var_decl_list_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_var_decls: HashSet<NodeIndex> = HashSet::new();
        let mut pattern_children: HashMap<NodeIndex, HashSet<NodeIndex>> = HashMap::new();
        let mut unused_pattern_elements: HashSet<NodeIndex> = HashSet::new();

        for (_sym_id, _name) in &symbols_to_check {
            let sym_id = *_sym_id;
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };
            let flags = symbol.flags;

            // Only track variables (not imports, not parameters)
            let is_var = (flags
                & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                != 0;
            if !is_var {
                continue;
            }

            // Get the declaration node
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            // Skip if this is a parameter
            if self.is_parameter_declaration(decl_idx) {
                continue;
            }

            // Find the parent VARIABLE_DECLARATION and VARIABLE_DECLARATION_LIST
            if let Some(var_decl_node_idx) = self.find_parent_variable_decl_node(decl_idx)
                && let Some(var_decl_list_idx) =
                    self.find_parent_variable_declaration(var_decl_node_idx)
            {
                // Track this VARIABLE_DECLARATION node under its parent list
                var_decl_list_children
                    .entry(var_decl_list_idx)
                    .or_default()
                    .insert(var_decl_node_idx);

                // Check if this variable is unused
                let is_used = self.ctx.referenced_symbols.borrow().contains(&sym_id);
                if !is_used {
                    unused_var_decls.insert(var_decl_node_idx);
                }

                if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                    pattern_children
                        .entry(pattern_idx)
                        .or_default()
                        .insert(decl_idx);
                    if !is_used {
                        unused_pattern_elements.insert(decl_idx);
                    }
                }
            }
        }

        // Now count VARIABLE_DECLARATION nodes (not variables) in each list
        for (var_decl_list_idx, decl_nodes) in &var_decl_list_children {
            let total_count = decl_nodes.len();
            let unused_count = decl_nodes
                .iter()
                .filter(|n| unused_var_decls.contains(n))
                .count();
            variable_declarations.insert(*var_decl_list_idx, (total_count, unused_count));
        }

        for (sym_id, name) in symbols_to_check {
            // Skip if already referenced
            if self.ctx.referenced_symbols.borrow().contains(&sym_id) {
                continue;
            }

            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            let flags = symbol.flags;

            // Skip exported symbols — they may be used externally
            if symbol.is_exported || (flags & symbol_flags::EXPORT_VALUE) != 0 {
                continue;
            }

            // Skip special/internal names
            if name == "default" || name == "__export" || name == "arguments" || name == "React"
            // JSX factory — always considered used when JSX is enabled
            {
                continue;
            }

            // Skip type parameters — they are handled separately (not in binder scope)
            if (flags & symbol_flags::TYPE_PARAMETER) != 0 {
                continue;
            }

            // Skip non-private members (constructors, signatures, enum members, prototype)
            // Private members ARE checked under noUnusedLocals (TS6133)
            let is_member = (flags
                & (symbol_flags::PROPERTY
                    | symbol_flags::METHOD
                    | symbol_flags::GET_ACCESSOR
                    | symbol_flags::SET_ACCESSOR))
                != 0;
            if is_member {
                // Only private members get unused checking — use PRIVATE flag set by binder
                let is_private = (flags & symbol_flags::PRIVATE) != 0;
                if !is_private {
                    continue; // Public/protected members may be used externally
                }
                // Setter-only private members are "used" by write accesses.
                // TSC never flags them as unused since writes count as usage.
                let is_setter_only = (flags & symbol_flags::SET_ACCESSOR) != 0
                    && (flags & symbol_flags::GET_ACCESSOR) == 0;
                if is_setter_only {
                    continue;
                }
                // Fall through to check private members
            }

            // Always skip constructors, signatures, enum members, prototype
            if (flags
                & (symbol_flags::CONSTRUCTOR
                    | symbol_flags::SIGNATURE
                    | symbol_flags::ENUM_MEMBER
                    | symbol_flags::PROTOTYPE))
                != 0
            {
                continue;
            }

            // Get the declaration node for position info
            let decl_idx = if !symbol.value_declaration.is_none() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Ambient declarations (`declare ...` or declarations nested in ambient
            // contexts) are not checked by noUnusedLocals.
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }

            // Skip catch clause variables — TSC exempts them from unused checking
            if self.is_catch_clause_variable(decl_idx) {
                continue;
            }

            // Skip using/await using declarations — they always have dispose side effects
            if self.is_using_declaration(decl_idx) {
                continue;
            }

            // Skip named function expression names — TSC never flags these as unused.
            // `var x = function somefn() {}` binds `somefn` in its own scope.
            if decl_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                && (flags & symbol_flags::FUNCTION) != 0
            {
                continue;
            }

            // Determine what kind of symbol this is and whether we should check it
            if check_locals {
                // Check local variables, functions, classes, interfaces, type aliases, imports
                let is_checkable_local = (flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                    || (flags & symbol_flags::FUNCTION) != 0
                    || (flags & symbol_flags::CLASS) != 0
                    || (flags & symbol_flags::INTERFACE) != 0
                    || (flags & symbol_flags::TYPE_ALIAS) != 0
                    || (flags & symbol_flags::ALIAS) != 0  // imports
                    || (flags & symbol_flags::REGULAR_ENUM) != 0
                    || (flags & symbol_flags::CONST_ENUM) != 0;

                // var declarations that aren't parameters
                let is_var = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && !self.is_parameter_declaration(decl_idx);

                // Private class members (property, method, accessor)
                let is_private_member = is_member;

                // Non-exported namespaces/modules
                let is_unused_namespace =
                    (flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)) != 0;

                if is_checkable_local || is_var || is_private_member || is_unused_namespace {
                    // For imports, check if this is part of an import declaration where ALL imports are unused.
                    // If so, skip emitting TS6133 here because TS6192 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE imports (single unused imports get TS6133).
                    let is_import = (flags & symbol_flags::ALIAS) != 0;
                    let skip_import_ts6133 = if is_import {
                        if let Some(import_decl_idx) = self.find_parent_import_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                import_declarations.get(&import_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple imports and ALL are unused (TS6192 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For variables, check if this is part of a variable declaration where ALL variables are unused.
                    // If so, skip emitting TS6133 here because TS6199 will be emitted for the entire declaration.
                    // Only skip when there are MULTIPLE variables (single unused variables get TS6133).
                    let is_variable = (flags
                        & (symbol_flags::BLOCK_SCOPED_VARIABLE
                            | symbol_flags::FUNCTION_SCOPED_VARIABLE))
                        != 0
                        && !self.is_parameter_declaration(decl_idx);
                    let skip_variable_ts6133 = if is_variable {
                        if let Some(var_decl_idx) = self.find_parent_variable_declaration(decl_idx)
                        {
                            if let Some(&(total_count, unused_count)) =
                                variable_declarations.get(&var_decl_idx)
                            {
                                // Skip TS6133 only if there are multiple variables and ALL are unused (TS6199 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // For destructuring patterns, check if this is part of a binding pattern where ALL elements are unused.
                    // If so, skip emitting TS6133 here because TS6198 will be emitted for the entire pattern.
                    // Only skip when there are MULTIPLE elements (single unused elements get TS6133).
                    let skip_destructuring_ts6133 = if is_variable {
                        if let Some(pattern_idx) = self.find_parent_binding_pattern(decl_idx) {
                            if let Some(&(total_count, unused_count)) =
                                destructuring_patterns.get(&pattern_idx)
                            {
                                // Skip TS6133 only if there are multiple elements and ALL are unused (TS6198 will cover it)
                                total_count > 1 && unused_count == total_count
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !skip_import_ts6133 && !skip_variable_ts6133 && !skip_destructuring_ts6133 {
                        // Check if write-only (assigned but never read)
                        // Destructured variables should NOT get TS6198 - they get TS6133
                        let is_destructured = self.find_parent_binding_pattern(decl_idx).is_some();
                        let is_write_only =
                            !is_destructured && self.ctx.written_symbols.borrow().contains(&sym_id);

                        // TS6196 for classes, interfaces, type aliases, enums ("never used")
                        // TS6198 for write-only variables ("assigned but never used")
                        // TS6133 for variables, functions, imports, class properties ("value never read")
                        // Note: TS6138 ("Property 'x' is declared but its value is never read")
                        // is only for constructor parameter properties, handled in the parameter section below.
                        let is_type_only = (flags & symbol_flags::CLASS) != 0
                            || (flags & symbol_flags::INTERFACE) != 0
                            || (flags & symbol_flags::TYPE_ALIAS) != 0
                            || (flags & symbol_flags::REGULAR_ENUM) != 0
                            || (flags & symbol_flags::CONST_ENUM) != 0;
                        let (msg, code) = if is_type_only {
                            (format!("'{name}' is declared but never used."), 6196)
                        } else if is_write_only {
                            (
                                format!("'{name}' is assigned a value but never used."),
                                6198,
                            )
                        } else {
                            (
                                format!("'{name}' is declared but its value is never read."),
                                6133,
                            )
                        };
                        let start = decl_node.pos;
                        let length = decl_node.end.saturating_sub(decl_node.pos);
                        self.ctx.push_diagnostic(Diagnostic {
                            file: file_name.clone(),
                            start,
                            length,
                            message_text: msg,
                            category: crate::diagnostics::DiagnosticCategory::Error,
                            code,
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            if check_params {
                // Check function parameters (but not catch clause or overload signature params)
                let is_param = (flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                    && self.is_parameter_declaration(decl_idx)
                    && !self.is_overload_signature_parameter(decl_idx);

                // Skip `this` parameter — it's a TypeScript type annotation, not an actual parameter
                if is_param && name == "this" {
                    continue;
                }

                // Skip parameters starting with _ (TSC convention for intentionally unused)
                if is_param && name.starts_with('_') {
                    continue;
                }

                if is_param {
                    let msg = format!("'{name}' is declared but its value is never read.");
                    let start = decl_node.pos;
                    let length = decl_node.end.saturating_sub(decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6133,
                        related_information: Vec::new(),
                    });
                }
            }
        }

        // Emit TS6192 for import declarations where ALL imports are unused.
        // Only emit this when there are MULTIPLE imports (total_count > 1).
        // For single unused imports, TS6133 is emitted above.
        if check_locals {
            for (import_decl_idx, (total_count, unused_count)) in import_declarations {
                // Only emit if there are multiple imports and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(import_decl_node) = self.ctx.arena.get(import_decl_idx)
                {
                    let msg = "All imports in import declaration are unused.".to_string();
                    let start = import_decl_node.pos;
                    let length = import_decl_node.end.saturating_sub(import_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6192,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6199 for variable declarations where ALL variables are unused.
            // Only emit this when there are MULTIPLE variables (total_count > 1).
            // For single unused variables, TS6133 is emitted above.
            for (var_decl_idx, (total_count, unused_count)) in variable_declarations {
                // Only emit if there are multiple variables and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(var_decl_node) = self.ctx.arena.get(var_decl_idx)
                {
                    let msg = "All variables are unused.".to_string();
                    let start = var_decl_node.pos;
                    let length = var_decl_node.end.saturating_sub(var_decl_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6199,
                        related_information: Vec::new(),
                    });
                }
            }

            // Emit TS6198 for destructuring patterns where ALL elements are unused.
            // Only emit this when there are MULTIPLE elements (total_count > 1).
            // For single unused elements, TS6133 is emitted above.
            for (pattern_idx, (total_count, unused_count)) in destructuring_patterns {
                // Only emit if there are multiple elements and ALL are unused
                if total_count > 1
                    && unused_count == total_count
                    && let Some(pattern_node) = self.ctx.arena.get(pattern_idx)
                {
                    let msg = "All destructured elements are unused.".to_string();
                    let start = pattern_node.pos;
                    let length = pattern_node.end.saturating_sub(pattern_node.pos);
                    self.ctx.push_diagnostic(Diagnostic {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: msg,
                        category: crate::diagnostics::DiagnosticCategory::Error,
                        code: 6198,
                        related_information: Vec::new(),
                    });
                }
            }
        }
    }

    /// Find the parent `IMPORT_DECLARATION` node for an import symbol's declaration.
    fn find_parent_import_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find IMPORT_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::IMPORT_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `VARIABLE_DECLARATION` node for a variable symbol's declaration.
    /// This returns the `VARIABLE_DECLARATION` node itself, not the `VARIABLE_DECLARATION_LIST`.
    fn find_parent_variable_decl_node(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `VARIABLE_DECLARATION_LIST` node for a variable symbol's declaration.
    /// This allows us to track all variables declared in a single statement (e.g., `var x, y;`).
    fn find_parent_variable_declaration(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find VARIABLE_DECLARATION_LIST
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Find the parent `BINDING_PATTERN` (`OBJECT_BINDING_PATTERN` or `ARRAY_BINDING_PATTERN`)
    /// for a binding element declaration. This is used to track TS6198 (all destructured
    /// elements are unused).
    fn find_parent_binding_pattern(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the parent chain to find OBJECT_BINDING_PATTERN or ARRAY_BINDING_PATTERN
        for _ in 0..10 {
            // Limit iterations to prevent infinite loops
            if idx.is_none() {
                return None;
            }

            if let Some(node) = self.ctx.arena.get(idx)
                && (node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
            {
                return Some(idx);
            }

            // Move to parent
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }

        None
    }

    /// Check if a declaration node is a parameter declaration.
    fn is_parameter_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::PARAMETER
    }

    /// Check if a declaration is a `using` or `await using` variable.
    /// These always have dispose side effects, so TSC never flags them as unused.
    fn is_using_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        {
            let flags = parent.flags as u32;
            if (flags & node_flags::USING) != 0 {
                return true;
            }
        }
        false
    }

    /// Check if a declaration is a catch clause variable.
    fn is_catch_clause_variable(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        let parent_idx = self
            .ctx
            .arena
            .get_extended(idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        if parent_idx.is_none() {
            return false;
        }
        if let Some(parent) = self.ctx.arena.get(parent_idx)
            && parent.kind == syntax_kind_ext::CATCH_CLAUSE
        {
            return true;
        }
        false
    }

    /// Check if a parameter is in an overload signature (function/method without body).
    /// TSC does not flag parameters in overload signatures as unused.
    fn is_overload_signature_parameter(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        // Walk up from parameter to find containing function/method/constructor
        // Structure: Parameter → SyntaxList/ParameterList → FunctionDecl/MethodDecl/Constructor
        let mut current = idx;
        for _ in 0..5 {
            let parent_idx = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent_idx.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent_idx) {
                match parent_node.kind {
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::FUNCTION_EXPRESSION => {
                        if let Some(func) = self.ctx.arena.get_function(parent_node) {
                            return func.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) = self.ctx.arena.get_method_decl(parent_node) {
                            return method.body.is_none();
                        }
                        return false;
                    }
                    syntax_kind_ext::CONSTRUCTOR => {
                        if let Some(ctor) = self.ctx.arena.get_constructor(parent_node) {
                            return ctor.body.is_none();
                        }
                        return false;
                    }
                    _ => {
                        current = parent_idx;
                    }
                }
            } else {
                return false;
            }
        }
        false
    }

    // 23. Import and Private Brand Utilities (moved to symbol_resolver.rs)

    // 25. AST Traversal Utilities (11 functions)

    /// Find the enclosing function-like node for a given node.
    ///
    /// Traverses up the AST to find the first parent that is a function-like
    /// construct (function declaration, function expression, arrow function, method, constructor).
    /// Find if there's a constructor implementation after position `start` in members list.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    ///
    /// Returns true if a constructor with a body is found, false otherwise.
    pub(crate) fn find_constructor_impl(&self, members: &[NodeIndex], start: usize) -> bool {
        for member_idx in members.iter().skip(start).copied() {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(node)
                    && !ctor.body.is_none()
                {
                    return true;
                }
                // Another constructor overload - keep looking
            } else {
                // Non-constructor member - no implementation found
                return false;
            }
        }
        false
    }

    /// Check if there's a method implementation with the given name after position `start`.
    ///
    /// ## Parameters
    /// - `members`: Slice of member node indices
    /// - `start`: Position to start searching from
    /// - `_name`: The method name to search for
    ///
    /// Returns (found: bool, name: Option<String>).
    pub(crate) fn find_method_impl(
        &self,
        members: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<usize>) {
        for (offset, member_idx) in members.iter().skip(start).copied().enumerate() {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    let member_name = self.get_method_name_from_node(member_idx);
                    if member_name.as_deref() != Some(name) {
                        if method.body.is_some() {
                            // Different name but has body - wrong-named implementation (TS2389)
                            return (true, member_name, Some(start + offset));
                        }
                        // Different name, no body - no implementation found
                        return (false, None, None);
                    }
                    if !method.body.is_none() {
                        // Found the implementation with matching name
                        return (true, member_name, Some(start + offset));
                    }
                    // Same name but no body - another overload signature, keep looking
                }
            } else {
                // Non-method member encountered - no implementation found
                return (false, None, None);
            }
        }
        (false, None, None)
    }

    /// Find a function implementation with the given name after position `start`.
    ///
    /// Recursively searches through statements to find a matching function implementation.
    /// Handles overload signatures by continuing to search through same-name overloads.
    ///
    /// ## Parameters
    /// - `statements`: Slice of statement node indices
    /// - `start`: Position to start searching from
    /// - `name`: The function name to search for
    ///
    /// Returns (found: bool, name: Option<String>, node: Option<NodeIndex>).
    pub(crate) fn find_function_impl(
        &self,
        statements: &[NodeIndex],
        start: usize,
        name: &str,
    ) -> (bool, Option<String>, Option<NodeIndex>) {
        if start >= statements.len() {
            return (false, None, None);
        }

        let stmt_idx = statements[start];
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return (false, None, None);
        };

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
        {
            // Check if this is an implementation (has body)
            if !func.body.is_none() {
                // This is an implementation - check if name matches
                let impl_name = self.get_function_name_from_node(stmt_idx);
                return (true, impl_name, Some(stmt_idx));
            }

            // Another overload signature without body - need to look further
            // but we should check if this is the same function name
            let overload_name = self.get_function_name_from_node(stmt_idx);
            if overload_name.as_ref() == Some(&name.to_string()) {
                // Same function, continue looking for implementation
                return self.find_function_impl(statements, start + 1, name);
            }
        }

        (false, None, None)
    }
}
