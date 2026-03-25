//! Unused declaration checking (TS6133, TS6138, TS6192) and overload resolution helpers.
//! - Constructor/method/function implementation finders
//! - TS6138: "Property 'x' is declared but its value is never read" for parameter properties

use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check for unused declarations (TS6133, TS6192).
    /// Reports variables, functions, classes, and other declarations that are never referenced.
    /// Also reports import declarations where ALL imports are unused (TS6192).
    pub(crate) fn check_unused_declarations(&mut self) {
        use std::collections::{HashMap, HashSet};
        use tsz_binder::ContainerKind;
        use tsz_binder::symbol_flags;

        let check_locals = self.ctx.no_unused_locals();
        let check_params = self.ctx.no_unused_parameters();
        let is_module = self.ctx.binder.is_external_module();

        // Skip .d.ts files entirely (ambient declarations)
        if self.ctx.is_declaration_file() {
            return;
        }

        // Collect symbols from scopes.
        // For script files (non-module), skip the root SourceFile scope since
        // top-level declarations are globals and not checked by noUnusedLocals.
        // For module files, check all scopes including root.
        //
        // The binder may create duplicate symbols for the same declaration in
        // both Function and Block scopes (e.g. `var` inside a function body).
        // Deduplicate by (name, declaration node) so we only check each
        // declaration once.  If ANY duplicate symbol is referenced, we treat
        // the canonical one as referenced too.
        let mut symbols_to_check: Vec<(tsz_binder::SymbolId, String)> = Vec::new();
        let mut seen_decls: HashSet<(String, NodeIndex)> = HashSet::new();

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
                // Deduplicate symbols that share the same name and declaration.
                // The binder can create separate SymbolIds for the same `var`
                // in both Function scope and Block scope.  Only keep the first
                // occurrence but propagate referenced status from duplicates.
                let decl_idx = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map(|s| {
                        if s.value_declaration.is_some() {
                            s.value_declaration
                        } else {
                            s.declarations.first().copied().unwrap_or(NodeIndex::NONE)
                        }
                    })
                    .unwrap_or(NodeIndex::NONE);
                let key = (name.clone(), decl_idx);
                if !seen_decls.insert(key) {
                    // Duplicate — propagate referenced status to the canonical symbol
                    if self.ctx.referenced_symbols.borrow().contains(&sym_id) {
                        // Find the canonical sym_id already in symbols_to_check
                        if let Some(&(canonical_sym_id, _)) =
                            symbols_to_check.iter().find(|(_, n)| *n == *name)
                        {
                            self.ctx
                                .referenced_symbols
                                .borrow_mut()
                                .insert(canonical_sym_id);
                        }
                    }
                    continue;
                }
                symbols_to_check.push((sym_id, name.clone()));
            }
        }

        // Track import declarations for TS6192.
        // Map from import declaration NodeIndex to (total_count, unused_count).
        let mut import_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track variable declarations for TS6199.
        // Map from variable declaration NodeIndex to (total_count, unused_count).
        let mut variable_declarations: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

        // Track destructuring patterns for TS6198.
        // Map from binding pattern NodeIndex to (total_elements, unused_elements).
        let mut destructuring_patterns: HashMap<NodeIndex, (usize, usize)> = HashMap::new();

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
            let decl_idx = if symbol.value_declaration.is_some() {
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
            let decl_idx = if symbol.value_declaration.is_some() {
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

        // Now count binding elements in each destructuring pattern.
        // TS6198 ("All destructured elements are unused") only applies to OBJECT binding
        // patterns (`{ a, b }`), not array patterns (`[a, b]`).
        for (pattern_idx, elements) in &pattern_children {
            // Only consider object binding patterns
            if let Some(node) = self.ctx.arena.get(*pattern_idx) {
                if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
                    continue;
                }
            } else {
                continue;
            }
            let total_count = elements.len();
            let unused_count = elements
                .iter()
                .filter(|n| unused_pattern_elements.contains(n))
                .count();
            destructuring_patterns.insert(*pattern_idx, (total_count, unused_count));
        }

        for (sym_id, name) in symbols_to_check {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                continue;
            };

            let flags = symbol.flags;
            let decl_idx = if symbol.value_declaration.is_some() {
                symbol.value_declaration
            } else if let Some(&first) = symbol.declarations.first() {
                first
            } else {
                continue;
            };

            let is_param_decl = self.is_parameter_declaration(decl_idx);
            let referenced = self.ctx.referenced_symbols.borrow().contains(&sym_id);
            // For parameter properties (constructor(private x: string)), the binder
            // creates a PROPERTY symbol in the class scope and a VARIABLE symbol in
            // the constructor scope, both sharing the same declaration node. The
            // deduplication logic (lines 69-82) propagates `referenced` status from
            // the VARIABLE to the PROPERTY, so `referenced` being true for the
            // PROPERTY symbol does NOT necessarily mean `this.x` was read — it might
            // just mean the parameter name was used in the constructor body.
            //
            // To correctly detect whether the property was actually read, we check
            // `referenced_as_property`, which is populated exclusively by
            // `check_property_accessibility` (property access, destructuring of this).
            let is_parameter_property = (flags & symbol_flags::PROPERTY) != 0
                && self
                    .ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::PARAMETER);

            // For parameter properties, use the property-specific reference set.
            if is_parameter_property {
                if self.ctx.referenced_as_property.borrow().contains(&sym_id) {
                    continue;
                }
            } else {
                #[allow(clippy::nonminimal_bool)]
                if referenced
                    && !self.is_self_reference_only_symbol_use(&name, decl_idx, flags)
                    && !(is_param_decl && self.is_parameter_only_type_referenced(&name, decl_idx))
                {
                    continue;
                }
            }

            // Skip exported symbols — they may be used externally
            if symbol.is_exported || (flags & symbol_flags::EXPORT_VALUE) != 0 {
                continue;
            }

            // Skip special/internal names
            if name == "default" || name == "__export" || name == "arguments" {
                continue;
            }

            // Skip React import only in classic JSX mode where React must be in scope.
            // In react-jsx/react-jsxdev modes, the automatic runtime handles the factory,
            // so an unused React import should be flagged.
            if name == "React" {
                use tsz_common::checker_options::JsxMode;
                let jsx_mode = self.ctx.compiler_options.jsx_mode;
                if jsx_mode == JsxMode::React || jsx_mode == JsxMode::Preserve {
                    continue;
                }
            }

            // Skip type parameters — they are handled separately (not in binder scope)
            if (flags & symbol_flags::TYPE_PARAMETER) != 0
                && !self.is_parameter_declaration(decl_idx)
            {
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
                // Only private members get unused checking — use PRIVATE flag set by binder.
                // ES private names (#foo) are also private but don't use the `private` keyword
                // modifier, so they won't have the PRIVATE flag. Detect them by name prefix.
                let is_private = (flags & symbol_flags::PRIVATE) != 0 || name.starts_with('#');
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

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Ambient declarations (`declare ...` or declarations nested in ambient
            // contexts) are not checked by noUnusedLocals.
            if self.should_skip_unused_for_ambient_declaration(decl_idx) {
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
                        // Skip underscore-prefixed binding elements in destructuring patterns.
                        // TSC suppresses TS6133 for `_`-prefixed names when they are part of a
                        // destructuring pattern (e.g., `const [_a, b] = arr` or `const { x: _x } = obj`).
                        // Regular declarations like `let _a = 1` are NOT suppressed.
                        if is_variable
                            && name.starts_with('_')
                            && self.find_parent_binding_pattern(decl_idx).is_some()
                        {
                            continue;
                        }

                        // Skip underscore-prefixed variables in for-in/for-of loops.
                        // TSC treats these as intentionally unused iteration variables
                        // (e.g., `for (const _ of items) {}`).
                        if is_variable
                            && name.starts_with('_')
                            && self.is_for_in_or_of_variable(decl_idx)
                        {
                            continue;
                        }

                        // Skip underscore-prefixed namespace imports and named import specifiers.
                        // TSC suppresses TS6133 for `import * as _foo from "mod"` and
                        // `import { _foo } from "mod"` when noUnusedLocals is enabled.
                        if is_import && name.starts_with('_') {
                            continue;
                        }

                        // Skip non-rest elements in object destructuring patterns that include
                        // a rest element. TSC considers these variables as "used" because they
                        // structurally exclude properties from the rest binding
                        // (e.g., `const {a, ...rest} = obj` — `a` is needed to exclude it from `rest`).
                        if is_variable && self.is_binding_element_alongside_rest(decl_idx) {
                            continue;
                        }

                        // Check if the symbol is referenced in JSDoc tags (e.g. `@link`, `@import`, `@type`).
                        // If so, consider it used and suppress the unused warning.
                        if self.is_symbol_used_in_jsdoc(&name) {
                            continue;
                        }

                        // TS6196 for classes, interfaces, type aliases, enums ("never used")
                        // TS6133 for variables, functions, imports, class properties ("value never read")
                        // TS6138 for constructor parameter properties ("Property 'x' is declared
                        //        but its value is never read") — detected by PROPERTY symbol
                        //        whose declaration node is a PARAMETER.
                        // Note: tsc uses TS6198 only for "all destructured elements unused" (a
                        // different path), never for individual write-only variables.
                        let is_type_only = (flags & symbol_flags::CLASS) != 0
                            || (flags & symbol_flags::INTERFACE) != 0
                            || (flags & symbol_flags::TYPE_ALIAS) != 0
                            || (flags & symbol_flags::REGULAR_ENUM) != 0
                            || (flags & symbol_flags::CONST_ENUM) != 0;

                        // Check if this is a parameter property (PROPERTY from a PARAMETER node)
                        let is_parameter_property = (flags & symbol_flags::PROPERTY) != 0
                            && decl_node.kind == syntax_kind_ext::PARAMETER;

                        let report_node = if is_parameter_property {
                            // For parameter properties, report at the parameter name
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx)
                        } else if let Some(spec_name_node) =
                            self.find_named_import_specifier_name_node(decl_idx, &name)
                        {
                            spec_name_node
                        } else if is_import {
                            // ES import declarations are anchored at declaration start, but
                            // import-equals diagnostics are anchored at the imported name.
                            if let Some(import_decl_idx) =
                                self.find_parent_import_declaration(decl_idx)
                            {
                                import_decl_idx
                            } else {
                                self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx)
                            }
                        } else {
                            self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx)
                        };
                        let (start, length) = if let Some(node) = self.ctx.arena.get(report_node) {
                            (node.pos, node.end.saturating_sub(node.pos))
                        } else {
                            (decl_node.pos, decl_node.end.saturating_sub(decl_node.pos))
                        };
                        if is_parameter_property {
                            self.error_property_declared_but_never_read(&name, start, length);
                        } else if is_type_only {
                            self.error_declared_but_never_used(&name, start, length);
                        } else {
                            self.error_declared_but_never_read(&name, start, length);
                        }
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
                    if self.is_symbol_used_in_jsdoc(&name) {
                        continue;
                    }

                    let report_node = self.get_declaration_name_node(decl_idx).unwrap_or(decl_idx);
                    let (start, length) = if let Some(node) = self.ctx.arena.get(report_node) {
                        (node.pos, node.end.saturating_sub(node.pos))
                    } else {
                        (decl_node.pos, decl_node.end.saturating_sub(decl_node.pos))
                    };
                    self.error_declared_but_never_read(&name, start, length);
                }
            }
        }

        // Emit TS6192 for import declarations where ALL imports are unused.
        // Only emit this when there are MULTIPLE imports (total_count > 1).
        // For single unused imports, TS6133 is emitted above.
        if check_locals {
            for (import_decl_idx, (total_count, unused_count)) in import_declarations {
                // Only emit if there are multiple imports and ALL are unused
                if total_count > 1 && unused_count == total_count {
                    self.error_at_node(
                        import_decl_idx,
                        "All imports in import declaration are unused.",
                        6192,
                    );
                }
            }

            // Emit TS6199 for variable declarations where ALL variables are unused.
            // Only emit this when there are MULTIPLE variables (total_count > 1).
            // For single unused variables, TS6133 is emitted above.
            for (var_decl_idx, (total_count, unused_count)) in variable_declarations {
                // Only emit if there are multiple variables and ALL are unused
                if total_count > 1 && unused_count == total_count {
                    self.error_at_node(var_decl_idx, "All variables are unused.", 6199);
                }
            }

            // Emit TS6198 for destructuring patterns where ALL elements are unused.
            // Only emit this when there are MULTIPLE elements (total_count > 1).
            // For single unused elements, TS6133 is emitted above.
            for (pattern_idx, (total_count, unused_count)) in destructuring_patterns {
                // Only emit if there are multiple elements and ALL are unused
                if total_count > 1 && unused_count == total_count {
                    self.error_at_node(pattern_idx, "All destructured elements are unused.", 6198);
                }
            }

            // Emit TS6138 for constructor parameter properties whose property is never read.
            // Parameter properties (e.g., `constructor(private x: string)`) create both a
            // parameter variable and a class property. The parameter may be referenced in the
            // constructor body, but if the property (this.x) is never read, TS6138 fires.
            // The binder creates PROPERTY symbols in the class scope for parameter properties.
            // Only private ones are checked (lines 268-274 skip non-private members).
            // Detection: if a PROPERTY symbol's declaration node is a PARAMETER, it's from
            // a parameter property and gets TS6138 instead of TS6133.
        }
    }

    /// Walk up the parent chain (up to 10 levels) to find an ancestor matching `predicate`.
    fn find_ancestor(
        &self,
        mut idx: NodeIndex,
        predicate: impl Fn(u16) -> bool,
    ) -> Option<NodeIndex> {
        for _ in 0..10 {
            if idx.is_none() {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(idx)
                && predicate(node.kind)
            {
                return Some(idx);
            }
            idx = self
                .ctx
                .arena
                .get_extended(idx)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
        }
        None
    }

    /// Find the parent `IMPORT_DECLARATION` node for an import symbol's declaration.
    fn find_parent_import_declaration(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        self.find_ancestor(idx, |kind| kind == syntax_kind_ext::IMPORT_DECLARATION)
    }

    /// Returns the local name node for a named import in a multi-specifier clause.
    fn find_named_import_specifier_name_node(
        &self,
        idx: NodeIndex,
        symbol_name: &str,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let import_decl_idx = self.find_parent_import_declaration(idx)?;
        let import_decl_node = self.ctx.arena.get(import_decl_idx)?;
        let import_decl = self.ctx.arena.get_import_decl(import_decl_node)?;
        let import_clause_node = self.ctx.arena.get(import_decl.import_clause)?;
        let import_clause = self.ctx.arena.get_import_clause(import_clause_node)?;
        if !import_clause.named_bindings.is_some() {
            return None;
        }
        let named_bindings_node = self.ctx.arena.get(import_clause.named_bindings)?;
        if named_bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
            return None;
        }

        let named = self.ctx.arena.get_named_imports(named_bindings_node)?;
        if named.elements.nodes.len() <= 1 {
            return None;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                continue;
            };
            if spec_node.kind != syntax_kind_ext::IMPORT_SPECIFIER {
                continue;
            }
            let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                continue;
            };
            let local_name_idx = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            if self.get_identifier_text_from_idx(local_name_idx).as_deref() == Some(symbol_name) {
                return Some(local_name_idx);
            }
        }

        // Fallback for cases where declaration directly points at a specifier node.
        let specifier_idx =
            self.find_ancestor(idx, |kind| kind == syntax_kind_ext::IMPORT_SPECIFIER)?;
        let specifier_node = self.ctx.arena.get(specifier_idx)?;
        if specifier_node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
            let spec = self.ctx.arena.get_specifier(specifier_node)?;
            return if spec.name.is_some() {
                Some(spec.name)
            } else {
                Some(spec.property_name)
            };
        }
        None
    }

    /// Find the parent `VARIABLE_DECLARATION` node for a variable symbol's declaration.
    fn find_parent_variable_decl_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        self.find_ancestor(idx, |kind| kind == syntax_kind_ext::VARIABLE_DECLARATION)
    }

    /// Find the parent `VARIABLE_DECLARATION_LIST` node for a variable symbol's declaration.
    fn find_parent_variable_declaration(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        self.find_ancestor(idx, |kind| {
            kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        })
    }

    /// Find the parent binding pattern for a binding element declaration.
    fn find_parent_binding_pattern(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        self.find_ancestor(idx, |kind| {
            kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        })
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

    /// Check if a variable declaration is in a for-in or for-of statement.
    /// TSC suppresses TS6133 for `_`-prefixed iteration variables.
    fn is_for_in_or_of_variable(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        // Walk up: BindingElement? → VariableDeclaration → VariableDeclarationList → ForInStatement/ForOfStatement
        let var_decl_idx = self.find_parent_variable_decl_node(idx);
        let var_decl_list_idx = var_decl_idx.and_then(|v| self.find_parent_variable_declaration(v));
        let Some(vdl_idx) = var_decl_list_idx else {
            return false;
        };
        let grandparent_idx = self
            .ctx
            .arena
            .get_extended(vdl_idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);
        if grandparent_idx.is_none() {
            return false;
        }
        if let Some(gp_node) = self.ctx.arena.get(grandparent_idx) {
            return gp_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                || gp_node.kind == syntax_kind_ext::FOR_OF_STATEMENT;
        }
        false
    }

    /// Check if a binding element is in an object destructuring alongside a rest element.
    /// TSC considers such elements as "used" because they structurally exclude properties
    /// from the rest binding (e.g., `const {a, ...rest} = obj`).
    fn is_binding_element_alongside_rest(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        // Find the parent object binding pattern
        let Some(pattern_idx) = self.find_parent_binding_pattern(idx) else {
            return false;
        };
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return false;
        };
        // Only applies to object binding patterns, not array binding patterns
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
        }
        // Get the binding pattern's elements and check if any has a dotDotDotToken (rest)
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return false;
        };
        // Check if the current element IS the rest element itself.
        // Only non-rest elements alongside a rest element should be considered "used".
        // The rest element itself (e.g., `...bar` in `const {a, ...bar} = foo`) should
        // still be checked for unused status.
        // Note: `idx` may be an Identifier node, so we check its parent (BindingElement).
        if let Some(ext) = self.ctx.arena.get_extended(idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(parent_be) = self.ctx.arena.get_binding_element(parent_node)
            && parent_be.dot_dot_dot_token
        {
            return false;
        }

        for &elem_idx in &pattern_data.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(be) = self.ctx.arena.get_binding_element(elem_node)
                && be.dot_dot_dot_token
            {
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

    /// Return true when the symbol is only referenced within its own
    /// declaration subtree, which tsc still treats as unused.
    fn is_self_reference_only_symbol_use(
        &self,
        name: &str,
        decl_idx: NodeIndex,
        flags: u32,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let can_ignore_self_refs = (flags
            & (symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::TYPE_ALIAS
                | symbol_flags::REGULAR_ENUM
                | symbol_flags::CONST_ENUM
                | symbol_flags::VALUE_MODULE
                | symbol_flags::NAMESPACE_MODULE
                | symbol_flags::METHOD
                | symbol_flags::PROPERTY
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR))
            != 0;
        if !can_ignore_self_refs {
            return false;
        }

        let decl_root = self
            .ctx
            .arena
            .get(decl_idx)
            .filter(|node| node.kind != SyntaxKind::Identifier as u16)
            .map(|_| decl_idx)
            .or_else(|| self.ctx.arena.node_info(decl_idx).map(|info| info.parent))
            .unwrap_or(decl_idx);
        let decl_name_idx = self
            .get_declaration_name_node(decl_root)
            .or_else(|| self.get_declaration_name_node(decl_idx))
            .unwrap_or(decl_idx);
        let Some(target_sym_id) = self
            .ctx
            .binder
            .get_node_symbol(decl_name_idx)
            .or_else(|| self.resolve_identifier_reference_without_tracking(decl_name_idx))
        else {
            return false;
        };
        let mut saw_same_name_reference = false;

        for i in 0..self.ctx.arena.len() {
            let idx = NodeIndex(i as u32);
            if idx == decl_name_idx {
                continue;
            }

            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(ident) = self.ctx.arena.get_identifier(node) else {
                continue;
            };
            if ident.escaped_text != name {
                continue;
            }

            let resolved_sym = self.resolve_identifier_reference_without_tracking(idx);
            if resolved_sym == Some(target_sym_id) {
                if !self.node_is_within_declaration(idx, decl_root)
                    && (self.is_declaration_name_position(idx)
                        || self.node_is_shadowed_by_same_name_type_parameter(idx, name))
                {
                    continue;
                }
                if !self.node_is_within_declaration(idx, decl_root) {
                    return false;
                }
                saw_same_name_reference = true;
                continue;
            }

            if self.node_is_within_declaration(idx, decl_root)
                && !self.is_declaration_name_of_other_symbol(idx, target_sym_id)
            {
                saw_same_name_reference = true;
            }
        }

        saw_same_name_reference
    }

    fn node_is_within_declaration(&self, idx: NodeIndex, decl_idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..50 {
            if current == decl_idx {
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            current = ext.parent;
        }
        false
    }

    fn node_is_in_type_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.is_type_node()
                || parent_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            {
                return true;
            }
            current = parent;
        }
        false
    }

    fn is_parameter_only_type_referenced(&self, name: &str, decl_idx: NodeIndex) -> bool {
        use tsz_scanner::SyntaxKind;

        let param_root = self
            .ctx
            .arena
            .get(decl_idx)
            .filter(|node| node.kind != SyntaxKind::Identifier as u16)
            .map(|_| decl_idx)
            .or_else(|| self.ctx.arena.node_info(decl_idx).map(|info| info.parent))
            .unwrap_or(decl_idx);
        let decl_name_idx = self
            .get_declaration_name_node(param_root)
            .or_else(|| self.get_declaration_name_node(decl_idx))
            .unwrap_or(decl_idx);
        let Some(target_sym_id) = self
            .ctx
            .binder
            .get_node_symbol(decl_name_idx)
            .or_else(|| self.resolve_identifier_reference_without_tracking(decl_name_idx))
        else {
            return false;
        };
        let mut saw_same_name_reference = false;

        for i in 0..self.ctx.arena.len() {
            let idx = NodeIndex(i as u32);
            if idx == decl_name_idx {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(ident) = self.ctx.arena.get_identifier(node) else {
                continue;
            };
            if ident.escaped_text != name {
                continue;
            }

            let in_benign_context = self.node_is_within_declaration(idx, param_root)
                || self.node_is_in_type_context(idx);
            let resolved_sym = self.resolve_identifier_reference_without_tracking(idx);
            if resolved_sym == Some(target_sym_id) {
                if !in_benign_context && self.is_declaration_name_position(idx) {
                    continue;
                }
                if in_benign_context {
                    saw_same_name_reference = true;
                    continue;
                }
                return false;
            }

            if in_benign_context && !self.is_declaration_name_of_other_symbol(idx, target_sym_id) {
                saw_same_name_reference = true;
                continue;
            }
        }

        saw_same_name_reference
    }

    fn resolve_identifier_reference_without_tracking(
        &self,
        idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        if self.node_is_in_type_context(idx) {
            match self.resolve_identifier_symbol_in_type_position_without_tracking(idx) {
                TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => {
                    Some(sym_id)
                }
                TypeSymbolResolution::NotFound => {
                    self.resolve_identifier_symbol_without_tracking(idx)
                }
            }
        } else {
            self.resolve_identifier_symbol_without_tracking(idx)
        }
    }

    fn is_declaration_name_of_other_symbol(
        &self,
        idx: NodeIndex,
        target_sym_id: tsz_binder::SymbolId,
    ) -> bool {
        self.ctx
            .binder
            .get_node_symbol(idx)
            .is_some_and(|sym_id| sym_id != target_sym_id)
    }

    fn is_declaration_name_position(&self, idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let parent = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        if self.get_declaration_name_node(parent) == Some(idx) {
            return true;
        }

        if parent_node.kind == syntax_kind_ext::TYPE_PARAMETER {
            return self
                .ctx
                .arena
                .get_type_parameter(parent_node)
                .is_some_and(|param| param.name == idx);
        }

        if parent_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
            || parent_node.kind == syntax_kind_ext::METHOD_SIGNATURE
        {
            return self
                .ctx
                .arena
                .get_signature(parent_node)
                .is_some_and(|sig| sig.name == idx);
        }

        false
    }

    fn should_skip_unused_for_ambient_declaration(&self, decl_idx: NodeIndex) -> bool {
        if self.ctx.is_declaration_file() {
            return true;
        }

        if !self.ctx.arena.is_in_ambient_context(decl_idx) {
            return false;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return true;
        };

        let has_explicit_declare = match node.kind {
            syntax_kind_ext::INTERFACE_DECLARATION => self
                .ctx
                .arena
                .get_interface(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .ctx
                .arena
                .get_type_alias(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            syntax_kind_ext::FUNCTION_DECLARATION => self
                .ctx
                .arena
                .get_function(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            syntax_kind_ext::CLASS_DECLARATION => self
                .ctx
                .arena
                .get_class(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            syntax_kind_ext::ENUM_DECLARATION => self
                .ctx
                .arena
                .get_enum(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            syntax_kind_ext::MODULE_DECLARATION => self
                .ctx
                .arena
                .get_module(node)
                .is_some_and(|decl| self.has_declare_modifier(&decl.modifiers)),
            _ => false,
        };
        if has_explicit_declare {
            return true;
        }

        let mut current = decl_idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            if self.ctx.arena.is_in_ambient_context(parent) {
                return true;
            }
            current = parent;
        }

        false
    }

    fn node_is_shadowed_by_same_name_type_parameter(&self, idx: NodeIndex, name: &str) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            let type_params = match parent_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION => self
                    .ctx
                    .arena
                    .get_function(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                syntax_kind_ext::CLASS_DECLARATION => self
                    .ctx
                    .arena
                    .get_class(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                syntax_kind_ext::METHOD_SIGNATURE
                | syntax_kind_ext::PROPERTY_SIGNATURE
                | syntax_kind_ext::CALL_SIGNATURE
                | syntax_kind_ext::CONSTRUCT_SIGNATURE => self
                    .ctx
                    .arena
                    .get_signature(parent_node)
                    .and_then(|decl| decl.type_parameters.clone()),
                _ => None,
            };

            if let Some(type_params) = type_params {
                for &param_idx in &type_params.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                        continue;
                    };
                    if param.name == idx {
                        continue;
                    }
                    let Some(param_name) = self.ctx.arena.get_identifier_at(param.name) else {
                        continue;
                    };
                    if param_name.escaped_text == name {
                        return true;
                    }
                }
            }

            current = parent;
        }

        false
    }

    // AST Traversal Utilities

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
                    && ctor.body.is_some()
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
                    if method.body.is_some() {
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
            if func.body.is_some() {
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

        // NOTE: A class declaration with the same name does NOT serve as a
        // function implementation. TSC reports TS2391 even when a class with the
        // same name follows the overload signatures (they merge, but the function
        // still needs its own body).
        (false, None, None)
    }

    /// Checks if a symbol name appears to be used in a JSDoc comment.
    /// This uses a fast string search over the file text to suppress false
    /// positive unused local errors for symbols referenced in JSDoc tags
    /// like `{@link X}`, `@import { X }`, `@type {X}`, etc.
    fn is_symbol_used_in_jsdoc(&self, name: &str) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let text: &str = &sf.text;

        let patterns = [
            format!("{{@link {name}}}"),
            format!("{{@link {name}."),
            format!("@import {{{name}}}"),
            format!("@import {{ {name} }}"),
            format!("@type {{{name}}}"),
            format!("@type {{{name}[]}}"),
            format!("@type {{ {name} }}"),
            format!("@param {{{name}}}"),
            format!("@param {{{name}[]}}"),
            format!("@param {{ {name} }}"),
            format!("@returns {{{name}}}"),
            format!("@returns {{{name}[]}}"),
            format!("@returns {{ {name} }}"),
            format!("@template {name}"),
        ];

        for p in &patterns {
            if text.contains(p) {
                return true;
            }
        }

        // Match JSDoc import with whitespace: `@import { Type } from ...`
        if text.contains(&format!("{{ {name} }}")) && text.contains("@import") {
            return true;
        }

        false
    }
}
