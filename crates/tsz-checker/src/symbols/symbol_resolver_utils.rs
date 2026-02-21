//! Lib symbol lookup, global value resolution, heritage symbol resolution,
//! test option parsing, and access class resolution.

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tracing::trace;
use tsz_binder::symbol_flags::CLASS;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Find a VALUE symbol for a name across all lib binders.
    ///
    /// This handles declaration merging across lib files: `interface Promise<T>` may be
    /// in one lib file (TYPE-only) while `declare var Promise: PromiseConstructor` is
    /// in another (VALUE). When the initial resolution finds only the TYPE symbol,
    /// this method searches all lib binders for the VALUE declaration.
    ///
    /// Returns the `SymbolId` of the VALUE symbol if found.
    pub(crate) fn find_value_symbol_in_libs(&self, name: &str) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        trace!(
            name = name,
            "find_value_symbol_in_libs: searching for VALUE symbol"
        );
        // Check file_locals first (may have merged value from lib)
        if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name) {
            trace!(
                name = name,
                val_sym_id = ?val_sym_id,
                "find_value_symbol_in_libs: found in file_locals"
            );
            if let Some(val_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(val_sym_id, &lib_binders)
            {
                trace!(
                    name = name,
                    val_sym_id = ?val_sym_id,
                    has_value = (val_symbol.flags & symbol_flags::VALUE) != 0,
                    is_type_only = val_symbol.is_type_only,
                    flags = val_symbol.flags,
                    "find_value_symbol_in_libs: symbol details"
                );
                if (val_symbol.flags & symbol_flags::VALUE) != 0 && !val_symbol.is_type_only {
                    trace!(
                        name = name,
                        returned_sym_id = ?val_sym_id,
                        "find_value_symbol_in_libs: returning from file_locals"
                    );
                    return Some(val_sym_id);
                }
            }
        }
        // Search lib binders directly
        for (lib_idx, lib_binder) in lib_binders.iter().enumerate() {
            if let Some(val_sym_id) = lib_binder.file_locals.get(name) {
                trace!(
                    name = name,
                    lib_idx = lib_idx,
                    val_sym_id = ?val_sym_id,
                    "find_value_symbol_in_libs: found in lib_binder"
                );
                if let Some(val_symbol) = lib_binder.get_symbol(val_sym_id) {
                    trace!(
                        name = name,
                        lib_idx = lib_idx,
                        val_sym_id = ?val_sym_id,
                        has_value = (val_symbol.flags & symbol_flags::VALUE) != 0,
                        is_type_only = val_symbol.is_type_only,
                        flags = val_symbol.flags,
                        "find_value_symbol_in_libs: lib symbol details"
                    );
                    if (val_symbol.flags & symbol_flags::VALUE) != 0 && !val_symbol.is_type_only {
                        trace!(
                            name = name,
                            lib_idx = lib_idx,
                            returned_sym_id = ?val_sym_id,
                            "find_value_symbol_in_libs: returning from lib_binder"
                        );
                        return Some(val_sym_id);
                    }
                }
            }
        }
        trace!(
            name = name,
            "find_value_symbol_in_libs: no VALUE symbol found"
        );
        None
    }

    /// Find a VALUE declaration node for a name across current + lib binders.
    ///
    /// Returning the declaration node avoids relying on cross-binder `SymbolId`
    /// identity, which can collide and lead to incorrect value/type selection.
    pub(crate) fn find_value_declaration_in_libs(
        &self,
        name: &str,
    ) -> Option<(SymbolId, NodeIndex)> {
        let lib_binders = self.get_lib_binders();

        // Check merged/local symbols first.
        if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(val_symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(val_sym_id, &lib_binders)
            && (val_symbol.flags & symbol_flags::VALUE) != 0
            && !val_symbol.is_type_only
            && val_symbol.value_declaration.is_some()
        {
            return Some((val_sym_id, val_symbol.value_declaration));
        }

        // Then scan lib binders directly.
        for lib_binder in &lib_binders {
            if let Some(val_sym_id) = lib_binder.file_locals.get(name)
                && let Some(val_symbol) = lib_binder.get_symbol(val_sym_id)
                && (val_symbol.flags & symbol_flags::VALUE) != 0
                && !val_symbol.is_type_only
                && val_symbol.value_declaration.is_some()
            {
                return Some((val_sym_id, val_symbol.value_declaration));
            }
        }

        None
    }

    // =========================================================================
    // Global Symbol Resolution
    // =========================================================================

    /// Resolve a global value symbol by name from `file_locals` and lib binders.
    ///
    /// This is used for looking up global values like `console`, `Math`, `globalThis`, etc.
    /// It checks:
    /// 1. Local `file_locals` (for user-defined globals and merged lib symbols)
    /// 2. Lib binders' `file_locals` (only when `lib_symbols_merged` is false)
    pub(crate) fn resolve_global_value_symbol(&self, name: &str) -> Option<SymbolId> {
        // First check local file_locals
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(sym_id);
        }

        // Skip lib binder scan if lib symbols are merged - they're all in file_locals already
        if self.ctx.binder.lib_symbols_are_merged() {
            return None;
        }

        // Legacy path: check lib binders for global symbols
        let lib_binders = self.get_lib_binders();
        for lib_binder in &lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    // =========================================================================
    // Heritage Symbol Resolution
    // =========================================================================

    /// Resolve a heritage clause expression to its symbol.
    ///
    /// Heritage clauses appear in `extends` and `implements` clauses of classes and interfaces.
    /// This function handles:
    /// - Simple identifiers (e.g., `class B extends A`)
    /// - Qualified names (e.g., `class B extends Namespace.A`)
    /// - Property access expressions (e.g., `class B extends module.A`)
    pub(crate) fn resolve_heritage_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.resolve_qualified_symbol(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left_sym_raw = self.resolve_heritage_symbol(access.expression)?;
            let name = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.clone())?;

            // First, check the raw symbol's direct exports (for namespace symbols)
            if let Some(left_symbol) = self.ctx.binder.get_symbol(left_sym_raw) {
                if let Some(exports) = left_symbol.exports.as_ref()
                    && let Some(member_sym) = exports.get(&name)
                {
                    return Some(member_sym);
                }

                // For import aliases (import X = require("./module")), X represents
                // the entire module namespace. Look up the member in module_exports.
                if let Some(ref module_specifier) = left_symbol.import_module {
                    if (left_symbol.flags & symbol_flags::ALIAS) != 0
                        && self
                            .ctx
                            .module_resolves_to_non_module_entity(module_specifier)
                    {
                        return None;
                    }
                    let mut visited_aliases = Vec::new();
                    if let Some(member_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        &name,
                        &mut visited_aliases,
                    ) {
                        return Some(member_sym);
                    }
                }
            }

            // Try resolving the alias to get the actual symbol (for non-require aliases
            // like `import X = SomeNamespace`)
            let mut visited_aliases = Vec::new();
            if let Some(resolved_sym) =
                self.resolve_alias_symbol(left_sym_raw, &mut visited_aliases)
                && resolved_sym != left_sym_raw
                && let Some(resolved_symbol) = self.ctx.binder.get_symbol(resolved_sym)
            {
                if let Some(exports) = resolved_symbol.exports.as_ref()
                    && let Some(member_sym) = exports.get(&name)
                {
                    return Some(member_sym);
                }
                // Also check module_exports on the resolved symbol
                if let Some(ref module_specifier) = resolved_symbol.import_module
                    && let Some(member_sym) = self.resolve_reexported_member_symbol(
                        module_specifier,
                        &name,
                        &mut visited_aliases,
                    )
                {
                    return Some(member_sym);
                }
            }

            return None;
        }

        None
    }

    /// Check if an expression is a property access on an unresolved import.
    ///
    /// Used to suppress TS2304 errors when TS2307 was already emitted for the module.
    pub(crate) fn is_property_access_on_unresolved_import(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Handle property access expressions (e.g., B.B in extends B.B)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return false;
            };
            // Check if the left side is an unresolved import or a property access on one
            return self.is_unresolved_import_symbol(access.expression)
                || self.is_property_access_on_unresolved_import(access.expression);
        }

        // Handle qualified names (e.g., A.B in type position)
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return false;
            };
            return self.is_unresolved_import_symbol(qn.left)
                || self.is_property_access_on_unresolved_import(qn.left);
        }

        // Direct identifier - check if it's an unresolved import
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unresolved_import_symbol(idx);
        }

        false
    }

    /// Check if an identifier refers to an unresolved import symbol.
    ///
    /// Returns true if:
    /// - The symbol is an ALIAS (import)
    /// - The imported module cannot be resolved through any of:
    ///   - `module_exports`
    ///   - `shorthand_ambient_modules`
    ///   - `declared_modules`
    ///   - CLI-resolved modules
    pub(crate) fn is_unresolved_import_symbol(&self, idx: NodeIndex) -> bool {
        tracing::info!(
            "DEBUG MODULE EXPORTS: {:?}",
            self.ctx.binder.module_exports.keys()
        );
        let res = self.is_unresolved_import_symbol_impl(idx);
        tracing::info!("DEBUG is_unresolved_import_symbol {:?} -> {}", idx, res);
        res
    }
    fn is_unresolved_import_symbol_impl(&self, idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(idx) else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if this is an ALIAS symbol (import)
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }

        // Check if it has an import_module - if so, check if that module is resolved
        if let Some(ref module_name) = symbol.import_module {
            // Check various ways a module can be resolved
            if self.ctx.binder.module_exports.contains_key(module_name) {
                return false; // Module is resolved (has exports)
            }
            // Check if this is a shorthand ambient module (no body/exports)
            // These should be treated as unresolved imports (any type)
            if self
                .ctx
                .binder
                .shorthand_ambient_modules
                .contains(module_name)
            {
                return true; // Shorthand ambient module - treat as unresolved/any
            }
            if self.is_ambient_module_match(module_name) {
                return false; // Ambient module pattern matches (with body/exports)
            }
            if let Some(ref resolved) = self.ctx.resolved_modules
                && resolved.contains(module_name)
            {
                return false; // CLI resolved module
            }
            // Module is not resolved - this is an unresolved import
            return true;
        }

        // For import equals declarations without import_module set,
        // check if the value_declaration is an import equals with a require
        if symbol.value_declaration.is_some() {
            let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
                return false;
            };
            if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import) = self.ctx.arena.get_import_decl(decl_node)
                && let Some(ref_node) = self.ctx.arena.get(import.module_specifier)
                && ref_node.kind == SyntaxKind::StringLiteral as u16
                && let Some(lit) = self.ctx.arena.get_literal(ref_node)
            {
                let module_name = &lit.text;
                if !self.ctx.binder.module_exports.contains_key(module_name)
                    && !self
                        .ctx
                        .binder
                        .shorthand_ambient_modules
                        .contains(module_name)
                    && !self.ctx.binder.declared_modules.contains(module_name)
                    && !self
                        .ctx
                        .resolved_modules
                        .as_ref()
                        .is_some_and(|r| r.contains(module_name))
                    && self.ctx.resolve_import_target(module_name).is_none()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a module specifier matches a declared or shorthand ambient module pattern.
    ///
    /// Supports simple wildcard patterns using `*` (e.g., "foo*baz", "*!text").
    pub(crate) fn is_ambient_module_match(&self, module_name: &str) -> bool {
        if self.binder_has_ambient_module(self.ctx.binder, module_name) {
            return true;
        }

        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if self.binder_has_ambient_module(binder, module_name) {
                    return true;
                }
            }
        }

        false
    }

    fn binder_has_ambient_module(
        &self,
        binder: &tsz_binder::BinderState,
        module_name: &str,
    ) -> bool {
        if self.matches_module_pattern(&binder.declared_modules, module_name)
            || self.matches_module_pattern(&binder.shorthand_ambient_modules, module_name)
        {
            return true;
        }

        false
    }

    fn matches_module_pattern(
        &self,
        patterns: &rustc_hash::FxHashSet<String>,
        module_name: &str,
    ) -> bool {
        patterns
            .iter()
            .any(|pattern| Self::module_name_matches_pattern(pattern, module_name))
    }

    fn module_name_matches_pattern(pattern: &str, module_name: &str) -> bool {
        let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
        let module_name = module_name.trim().trim_matches('"').trim_matches('\'');

        if !pattern.contains('*') {
            return pattern == module_name;
        }

        // Use globset for robust wildcard matching (handles multiple '*' correctly)
        // Allow '*' to match path separators so patterns like "*!text" match "./file!text".
        if let Ok(glob) = globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
        {
            let matcher = glob.compile_matcher();
            return matcher.is_match(module_name);
        }

        false
    }

    // =========================================================================
    // Require/Import Resolution
    // =========================================================================

    /// Extract the module specifier from a `require()` call expression or
    /// a string literal (for import equals declarations where the parser
    /// stores only the string literal, not the full `require()` call).
    ///
    /// Returns the module path string (e.g., `'./util'` from `require('./util')`).
    pub(crate) fn get_require_module_specifier(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        // For import equals declarations, the parser stores just the string literal
        // e.g., `import x = require('./util')` has module_specifier = StringLiteral('./util')
        if node.kind == SyntaxKind::StringLiteral as u16 {
            let literal = self.ctx.arena.get_literal(node)?;
            // Strip surrounding quotes if present (parser stores raw text with quotes)
            let text = literal.text.trim_matches(|c| c == '"' || c == '\'');
            return Some(text.to_string());
        }

        // Handle full require() call expression (for other contexts)
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_ident = self.ctx.arena.get_identifier_at(call.expression)?;
        if callee_ident.escaped_text != "require" {
            return None;
        }

        let args = call.arguments.as_ref()?;
        let first_arg = args.nodes.first().copied()?;
        let literal = self.ctx.arena.get_literal_at(first_arg)?;
        Some(literal.text.clone())
    }

    /// Resolve a `require()` call to its symbol.
    ///
    /// For `require()` calls, we don't resolve to a single symbol.
    /// Instead, `compute_type_of_symbol` handles this by creating a module namespace type.
    pub(crate) fn resolve_require_call_symbol(
        &self,
        idx: NodeIndex,
        _visited_aliases: Option<&mut Vec<SymbolId>>,
    ) -> Option<SymbolId> {
        // For require() calls, we don't resolve to a single symbol.
        // Instead, compute_type_of_symbol handles this by creating a module namespace type.
        // This function now just returns None to indicate no single symbol resolution.
        let _ = self.get_require_module_specifier(idx)?;
        // Module resolution for require() is handled in compute_type_of_symbol
        // by creating an object type from module_exports.
        None
    }

    // =========================================================================
    // Type Query Resolution
    // =========================================================================

    /// Find the missing left-most identifier in a type query expression.
    ///
    /// For `typeof A.B.C`, if `A` is unresolved, this returns the node for `A`.
    pub(crate) fn missing_type_query_left(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                if self.resolve_identifier_symbol(current).is_none() {
                    // globalThis is a synthetic global in tsc with no binder symbol.
                    // Don't report it as missing in typeof qualified expressions
                    // (e.g., `typeof globalThis.isNaN`).
                    if let Some(ident) = self.ctx.arena.get_identifier(node)
                        && ident.escaped_text == "globalThis"
                    {
                        return None;
                    }
                    return Some(current);
                }
                return None;
            }
            if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                return None;
            }
            let qn = self.ctx.arena.get_qualified_name(node)?;
            current = qn.left;
        }
    }

    /// Report a type query missing member error.
    ///
    /// For `typeof A.B` where `B` is not found in `A`'s exports, emits TS2694.
    /// Returns true if an error was reported.
    pub(crate) fn report_type_query_missing_member(&mut self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return false,
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return false,
        };

        let left_sym = match self.resolve_qualified_symbol(qn.left) {
            Some(sym) => sym,
            None => return false,
        };
        let lib_binders = self.get_lib_binders();
        let left_symbol = match self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) {
            Some(symbol) => symbol,
            None => return false,
        };

        // Only report TS2694 for namespace/module/enum/class symbols.
        // For regular variables (e.g., `typeof x.p` where x is a local variable),
        // the qualified name refers to a property access, not a namespace member.
        let is_namespace_like = left_symbol.flags
            & (symbol_flags::MODULE
                | CLASS
                | symbol_flags::REGULAR_ENUM
                | symbol_flags::CONST_ENUM
                | symbol_flags::INTERFACE)
            != 0;
        if !is_namespace_like {
            return false;
        }

        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return false,
        };

        // Check direct exports first
        if let Some(exports) = left_symbol.exports.as_ref()
            && exports.has(right_name)
        {
            return false;
        }

        // For classes, check if the member exists in the class's members (static members)
        // This handles `typeof C.staticMember` where C is a class
        if left_symbol.flags & CLASS != 0
            && let Some(members) = left_symbol.members.as_ref()
            && members.has(right_name)
        {
            return false;
        }

        // Check for re-exports from other modules
        // This handles cases like: export { foo } from './bar'
        if let Some(ref module_specifier) = left_symbol.import_module {
            if (left_symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                let namespace_name = self
                    .entity_name_text(qn.left)
                    .unwrap_or_else(|| left_symbol.escaped_name.clone());
                self.error_namespace_no_export(&namespace_name, right_name, qn.right);
                return true;
            }
            let mut visited_aliases = Vec::new();
            if self
                .resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    &mut visited_aliases,
                )
                .is_some()
            {
                return false;
            }
        }

        let namespace_name = self
            .entity_name_text(qn.left)
            .unwrap_or_else(|| left_symbol.escaped_name.clone());
        self.error_namespace_no_export(&namespace_name, right_name, qn.right);
        true
    }

    // =========================================================================
    // Test Option Resolution
    // =========================================================================

    /// Parse a boolean option from test file comments.
    ///
    /// Looks for patterns like `// @key: true` or `// @key: false` in the first 32 lines.
    pub(crate) fn parse_test_option_bool(text: &str, key: &str) -> Option<bool> {
        for line in text.lines().take(32) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let is_comment =
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*');
            if !is_comment {
                break;
            }

            let lower = trimmed.to_ascii_lowercase();
            let Some(pos) = lower.find(key) else {
                continue;
            };
            let after_key = &lower[pos + key.len()..];
            let Some(colon_pos) = after_key.find(':') else {
                continue;
            };
            let value = after_key[colon_pos + 1..].trim();

            // Parse boolean value, handling comma-separated values like "true, false"
            // Also handle trailing commas, semicolons, and other delimiters
            let value_clean = if let Some(comma_pos) = value.find(',') {
                &value[..comma_pos]
            } else if let Some(semicolon_pos) = value.find(';') {
                &value[..semicolon_pos]
            } else {
                value
            }
            .trim();

            match value_clean {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => continue,
            }
        }
        None
    }

    /// Resolve a boolean compiler option from source file comments.
    /// Checks for the option-specific pragma first, then optionally checks `@strict`,
    /// and falls back to the provided default.
    fn resolve_bool_option(text: &str, pragma: &str, strict_fallback: bool, default: bool) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, pragma) {
            return value;
        }
        if strict_fallback && let Some(strict) = Self::parse_test_option_bool(text, "@strict") {
            return strict;
        }
        default
    }

    /// Resolve all compiler options from source file comment pragmas.
    /// Called once per file to override compiler options with test pragmas.
    pub(crate) fn resolve_compiler_options_from_source(&mut self, text: &str) {
        // Snapshot current defaults before mutation to avoid aliased borrows.
        let defaults = self.ctx.compiler_options.clone();
        let opts = &mut self.ctx.compiler_options;
        // Options that fall back to @strict
        opts.no_implicit_any =
            Self::resolve_bool_option(text, "@noimplicitany", true, defaults.no_implicit_any);
        opts.use_unknown_in_catch_variables = Self::resolve_bool_option(
            text,
            "@useunknownincatchvariables",
            true,
            defaults.use_unknown_in_catch_variables,
        );
        opts.no_implicit_this =
            Self::resolve_bool_option(text, "@noimplicitthis", true, defaults.no_implicit_this);
        opts.strict_property_initialization = Self::resolve_bool_option(
            text,
            "@strictpropertyinitialization",
            true,
            defaults.strict_property_initialization,
        );
        opts.strict_null_checks =
            Self::resolve_bool_option(text, "@strictnullchecks", true, defaults.strict_null_checks);
        opts.strict_function_types = Self::resolve_bool_option(
            text,
            "@strictfunctiontypes",
            true,
            defaults.strict_function_types,
        );
        // Options without @strict fallback
        opts.no_implicit_returns = Self::resolve_bool_option(
            text,
            "@noimplicitreturns",
            false,
            defaults.no_implicit_returns,
        );
        opts.no_implicit_override = Self::resolve_bool_option(
            text,
            "@noimplicitoverride",
            false,
            defaults.no_implicit_override,
        );
        opts.no_unused_locals =
            Self::resolve_bool_option(text, "@nounusedlocals", false, defaults.no_unused_locals);
        opts.no_unused_parameters = Self::resolve_bool_option(
            text,
            "@nounusedparameters",
            false,
            defaults.no_unused_parameters,
        );
        opts.always_strict =
            Self::resolve_bool_option(text, "@alwaysstrict", false, defaults.always_strict);
        // Option<bool> variant
        opts.allow_unreachable_code = Self::parse_test_option_bool(text, "@allowunreachablecode")
            .map(Some)
            .unwrap_or(defaults.allow_unreachable_code);
    }

    // =========================================================================
    // Duplicate Declaration Resolution
    // =========================================================================

    /// Resolve the declaration node for duplicate identifier checking.
    ///
    /// For some nodes (like short-hand properties), we need to walk up to find
    /// the actual declaration node to report the error on.
    pub(crate) fn resolve_duplicate_decl_node(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = arena.get(current)?;
            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_CLAUSE
                | syntax_kind_ext::NAMESPACE_IMPORT
                | syntax_kind_ext::IMPORT_SPECIFIER
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_SPECIFIER
                | syntax_kind_ext::CONSTRUCTOR => {
                    return Some(current);
                }
                _ => {
                    if let Some(ext) = arena.get_extended(current) {
                        current = ext.parent;
                    } else {
                        return None;
                    }
                }
            }
        }
        None
    }

    // =========================================================================
    // Class Access Resolution
    // =========================================================================

    /// Resolve the class for a member access expression.
    ///
    /// Returns the class declaration node and whether the access is on the constructor type.
    /// Used for checking private/protected member accessibility.
    pub(crate) fn resolve_class_for_access(
        &mut self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        if self.is_this_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
        {
            return Some((class_info.class_idx, self.is_constructor_type(object_type)));
        }

        if self.is_super_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
            && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
        {
            return Some((base_idx, self.is_constructor_type(object_type)));
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::CLASS != 0
            && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
        {
            return Some((class_idx, true));
        }

        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(class_idx) = self.get_class_decl_from_type(object_type)
        {
            return Some((class_idx, false));
        }

        None
    }

    /// Resolve the receiver class for a member access expression.
    ///
    /// Similar to `resolve_class_for_access`, but returns only the class node.
    /// Used for determining what class the receiver belongs to.
    pub(crate) fn resolve_receiver_class_for_access(
        &self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<NodeIndex> {
        if self.is_this_expression(expr_idx) || self.is_super_expression(expr_idx) {
            return self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        }

        if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::CLASS != 0
        {
            return self.get_class_declaration_from_symbol(sym_id);
        }

        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(class_idx) = self.get_class_decl_from_type(object_type)
        {
            return Some(class_idx);
        }

        None
    }

    /// Resolves a string identifier relative to the scope of a given node.
    pub(crate) fn resolve_name_at_node(&self, name: &str, node_idx: NodeIndex) -> Option<SymbolId> {
        let ignore_libs = !self.ctx.has_lib_loaded();
        let lib_binders = if ignore_libs {
            Vec::new()
        } else {
            self.get_lib_binders()
        };
        let is_from_lib = |sym_id: SymbolId| self.ctx.symbol_is_from_lib(sym_id);
        let should_skip_lib_symbol = |sym_id: SymbolId| ignore_libs && is_from_lib(sym_id);

        let result = self.ctx.binder.resolve_name_with_filter(
            name,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return is_from_lib(sym_id)
                            && (symbol.flags & tsz_binder::symbol_flags::EXPORT_VALUE) != 0;
                    }
                }
                true
            },
        );

        if result.is_none() && !ignore_libs {
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name)
                    && !should_skip_lib_symbol(lib_sym_id)
                {
                    let Some(file_sym_id) = self.ctx.binder.file_locals.get(name) else {
                        continue;
                    };
                    return Some(file_sym_id);
                }
            }
        }

        result
    }
}
