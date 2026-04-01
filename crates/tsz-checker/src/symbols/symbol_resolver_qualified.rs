//! Qualified name resolution, re-export resolution, namespace member resolution,
//! and cross-file symbol lookup.
//!
//! Split from `symbol_resolver.rs` — handles:
//! - Qualified name resolution (value and type position)
//! - Private identifier resolution
//! - Cross-file / all-binder symbol lookup
//! - Namespace member resolution
//! - Re-export chain following
//! - Import-equals alias member resolution

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::symbol_resolver::TypeSymbolResolution;

// =============================================================================
// Qualified Name & Cross-File Resolution Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Resolve a private identifier to its symbols across class scopes.
    ///
    /// Private identifiers (e.g., `#foo`) are only valid within class bodies.
    /// This function walks the scope chain and collects all symbols with the
    /// matching private name from class scopes.
    ///
    /// Returns a tuple of (`symbols_found`, `saw_class_scope`) where:
    /// - `symbols_found`: Vec of `SymbolIds` for all matching private members
    /// - `saw_class_scope`: true if any class scope was encountered
    pub(crate) fn resolve_private_identifier_symbols(
        &self,
        idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        self.ctx
            .binder
            .resolve_private_identifier_symbols(self.ctx.arena, idx)
    }

    /// Resolve a qualified name or identifier to a symbol ID.
    ///
    /// Handles both simple identifiers and qualified names (e.g., `A.B.C`).
    /// Also resolves through alias symbols (imports).
    pub(crate) fn resolve_qualified_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let mut visited_aliases = Vec::new();
        self.resolve_qualified_symbol_inner(idx, &mut visited_aliases, 0)
    }

    /// Resolve a qualified name or identifier for type positions.
    pub(crate) fn resolve_qualified_symbol_in_type_position(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let mut visited_aliases = Vec::new();
        self.resolve_qualified_symbol_inner_in_type_position(idx, &mut visited_aliases, 0)
    }

    /// Inner implementation of qualified symbol resolution for type positions.
    pub(crate) fn resolve_qualified_symbol_inner_in_type_position(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
        depth: usize,
    ) -> TypeSymbolResolution {
        // Prevent stack overflow from deeply nested qualified names
        const MAX_QUALIFIED_NAME_DEPTH: usize = 128;
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return TypeSymbolResolution::NotFound;
        }

        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return TypeSymbolResolution::NotFound,
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            let lib_binders = self.get_lib_binders();
            return match self.resolve_identifier_symbol_in_type_position(idx) {
                TypeSymbolResolution::Type(sym_id) => {
                    if self
                        .ctx
                        .binder
                        .get_symbol_with_libs(sym_id, &lib_binders)
                        .is_some_and(|symbol| (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0)
                    {
                        return TypeSymbolResolution::Type(sym_id);
                    }
                    // Preserve unresolved alias symbols in type position.
                    // `import X = require("...")` aliases may not resolve to a concrete
                    // target symbol, but `X` is still a valid namespace-like type query
                    // anchor (e.g., `typeof X.Member`).
                    let resolved = self
                        .resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id);
                    TypeSymbolResolution::Type(resolved)
                }
                TypeSymbolResolution::ValueOnly(sym_id)
                    if self.is_import_equals_type_anchor(sym_id, &lib_binders) =>
                {
                    let resolved = self
                        .resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id);
                    TypeSymbolResolution::Type(resolved)
                }
                other => other,
            };
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let Some(literal) = self.ctx.arena.get_literal(node) else {
                return TypeSymbolResolution::NotFound;
            };
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&literal.text) {
                let is_value_only = (self
                    .alias_resolves_to_value_only(sym_id, Some(&literal.text))
                    || self.symbol_is_value_only(sym_id, Some(&literal.text)))
                    && !self.symbol_is_type_only(sym_id, Some(&literal.text));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(sym_id);
                }
                let Some(sym_id) = self.resolve_alias_symbol(sym_id, visited_aliases) else {
                    return TypeSymbolResolution::NotFound;
                };
                return TypeSymbolResolution::Type(sym_id);
            }
            return TypeSymbolResolution::NotFound;
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return TypeSymbolResolution::NotFound;
            };

            let left_sym = match self.resolve_qualified_symbol_inner_in_type_position(
                access.expression,
                visited_aliases,
                depth + 1,
            ) {
                TypeSymbolResolution::Type(sym_id) => sym_id,
                other => return other,
            };

            let left_sym = self
                .resolve_alias_symbol(left_sym, visited_aliases)
                .unwrap_or(left_sym);

            let right_name = match self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.as_str())
            {
                Some(name) => name,
                None => return TypeSymbolResolution::NotFound,
            };

            let lib_binders = self.get_lib_binders();
            let Some(left_symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders)
            else {
                return TypeSymbolResolution::NotFound;
            };

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(member_sym, Some(right_name))
                    || self.symbol_is_value_only(member_sym, Some(right_name)))
                    && !self.symbol_is_type_only(member_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(member_sym);
                }
                let member_sym = self
                    .resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym);
                return TypeSymbolResolution::Type(member_sym);
            }

            if let Some(ref module_specifier) = left_symbol.import_module
                && !((left_symbol.flags & symbol_flags::ALIAS) != 0
                    && self
                        .ctx
                        .module_resolves_to_non_module_entity(module_specifier))
                && let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    visited_aliases,
                )
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(reexported_sym, Some(right_name))
                    || self.symbol_is_value_only(reexported_sym, Some(right_name)))
                    && !self.symbol_is_type_only(reexported_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(reexported_sym);
                }
                return TypeSymbolResolution::Type(reexported_sym);
            }

            if let Some(reexported_sym) =
                self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(reexported_sym, Some(right_name))
                    || self.symbol_is_value_only(reexported_sym, Some(right_name)))
                    && !self.symbol_is_type_only(reexported_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(reexported_sym);
                }
                return TypeSymbolResolution::Type(reexported_sym);
            }

            if let Some(ref module_specifier) = left_symbol.import_module
                && let Some(augmented_sym) = self.resolve_module_augmentation_member_symbol(
                    module_specifier,
                    right_name,
                    visited_aliases,
                )
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(augmented_sym, Some(right_name))
                    || self.symbol_is_value_only(augmented_sym, Some(right_name)))
                    && !self.symbol_is_type_only(augmented_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(augmented_sym);
                }
                return TypeSymbolResolution::Type(augmented_sym);
            }

            return TypeSymbolResolution::NotFound;
        }

        if node.kind != tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
            return TypeSymbolResolution::NotFound;
        }

        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return TypeSymbolResolution::NotFound,
        };
        let left_sym = match self.resolve_qualified_symbol_inner_in_type_position(
            qn.left,
            visited_aliases,
            depth + 1,
        ) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            other => return other,
        };
        let left_sym = self
            .resolve_alias_symbol(left_sym, visited_aliases)
            .unwrap_or(left_sym);
        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return TypeSymbolResolution::NotFound,
        };

        // Look up the symbol across binders (file + libs)
        let lib_binders = self.get_lib_binders();
        let Some(left_symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) else {
            return TypeSymbolResolution::NotFound;
        };
        // First try direct exports
        if let Some(exports) = left_symbol.exports.as_ref()
            && let Some(member_sym) = exports.get(right_name)
        {
            let is_value_only = (self.alias_resolves_to_value_only(member_sym, Some(right_name))
                || self.symbol_is_value_only(member_sym, Some(right_name)))
                && !self.symbol_is_type_only(member_sym, Some(right_name));
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(member_sym);
            }
            return TypeSymbolResolution::Type(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        // If not found in direct exports, check for re-exports
        if let Some(ref module_specifier) = left_symbol.import_module {
            if (left_symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return TypeSymbolResolution::NotFound;
            }
            if let Some(reexported_sym) =
                self.resolve_reexported_member_symbol(module_specifier, right_name, visited_aliases)
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(reexported_sym, Some(right_name))
                    || self.symbol_is_value_only(reexported_sym, Some(right_name)))
                    && !self.symbol_is_type_only(reexported_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(reexported_sym);
                }
                return TypeSymbolResolution::Type(reexported_sym);
            }
        }

        if let Some(reexported_sym) =
            self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
        {
            let is_value_only = (self
                .alias_resolves_to_value_only(reexported_sym, Some(right_name))
                || self.symbol_is_value_only(reexported_sym, Some(right_name)))
                && !self.symbol_is_type_only(reexported_sym, Some(right_name));
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(reexported_sym);
            }
            return TypeSymbolResolution::Type(reexported_sym);
        }

        if let Some(ref module_specifier) = left_symbol.import_module
            && let Some(augmented_sym) = self.resolve_module_augmentation_member_symbol(
                module_specifier,
                right_name,
                visited_aliases,
            )
        {
            let is_value_only = (self
                .alias_resolves_to_value_only(augmented_sym, Some(right_name))
                || self.symbol_is_value_only(augmented_sym, Some(right_name)))
                && !self.symbol_is_type_only(augmented_sym, Some(right_name));
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(augmented_sym);
            }
            return TypeSymbolResolution::Type(augmented_sym);
        }

        TypeSymbolResolution::NotFound
    }

    pub(crate) fn resolve_identifier_symbol_from_all_binders(
        &self,
        name: &str,
        mut accept: impl FnMut(SymbolId, &tsz_binder::Symbol) -> bool,
    ) -> Option<SymbolId> {
        // Use the pre-built global index for O(1) lookup instead of O(N) binder scan
        let entries = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name));

        let all_binders = self.ctx.all_binders.as_ref()?;

        if let Some(entries) = entries {
            for &(file_idx, sym_id) in entries {
                let binder = &all_binders[file_idx];
                let Some(sym_symbol) = binder.get_symbol(sym_id) else {
                    continue;
                };
                if !accept(sym_id, sym_symbol) {
                    continue;
                }
                if let Some(local_symbol) = self.ctx.binder.get_symbol(sym_id) {
                    if local_symbol.escaped_name != name && !self.ctx.has_symbol_file_index(sym_id)
                    {
                        self.ctx.register_symbol_file_target(sym_id, file_idx);
                    }
                } else if !self.ctx.has_symbol_file_index(sym_id) {
                    self.ctx.register_symbol_file_target(sym_id, file_idx);
                }
                return Some(sym_id);
            }
        }

        None
    }

    /// Resolve a namespace member across all binders in multi-file mode.
    ///
    /// Cross-file lookup binders have `file_locals` (name->SymbolId) but empty symbol
    /// arenas. So we use the checker's own binder (which has the shared global symbol
    /// arena) to look up symbol data.
    ///
    /// Also handles nested namespaces: for `A.Utils.Plane`, searches parent namespace
    /// exports in each binder's `file_locals` to find the nested `Utils` namespace.
    pub(crate) fn resolve_namespace_member_from_all_binders(
        &self,
        namespace_name: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        let all_binders = self.ctx.all_binders.as_ref()?;

        // Use the pre-built global index for O(1) namespace lookup
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(namespace_name))
        {
            for &(file_idx, ns_sym_id) in entries {
                // Use checker's binder for symbol data (cross-file binders have empty arenas)
                if let Some(ns_symbol) = self.ctx.binder.get_symbol(ns_sym_id)
                    && ns_symbol.flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                        != 0
                    && let Some(exports) = ns_symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(member_name)
                {
                    // Filter out enum members - they should only be accessible via qualified form
                    let is_enum_member = self
                        .ctx
                        .binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                    if !is_enum_member {
                        self.record_cross_file_member(member_id, member_name, file_idx);
                        return Some(member_id);
                    }
                }
            }
        }

        // For nested namespaces (e.g., `Utils` inside `A`): search parent
        // namespace exports in each binder for the target namespace name.
        // This part still scans all_binders since the nested namespace name
        // isn't a file_locals key -- it's an export of a parent namespace.
        for (file_idx, binder) in all_binders.iter().enumerate() {
            for (_, &parent_sym_id) in binder.file_locals.iter() {
                let Some(parent_sym) = self.ctx.binder.get_symbol(parent_sym_id) else {
                    continue;
                };
                if parent_sym.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                    == 0
                {
                    continue;
                }
                if let Some(parent_exports) = parent_sym.exports.as_ref()
                    && let Some(nested_ns_id) = parent_exports.get(namespace_name)
                    && let Some(nested_ns) = self.ctx.binder.get_symbol(nested_ns_id)
                    && nested_ns.flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                        != 0
                    && let Some(nested_exports) = nested_ns.exports.as_ref()
                    && let Some(member_id) = nested_exports.get(member_name)
                {
                    self.record_cross_file_member(member_id, member_name, file_idx);
                    return Some(member_id);
                }
            }
        }

        None
    }

    /// Record a cross-file symbol origin for proper arena delegation.
    fn record_cross_file_member(&self, member_id: SymbolId, member_name: &str, file_idx: usize) {
        if let Some(local_sym) = self.ctx.binder.get_symbol(member_id) {
            if local_sym.escaped_name.as_str() != member_name
                && !self.ctx.has_symbol_file_index(member_id)
            {
                self.ctx.register_symbol_file_target(member_id, file_idx);
            }
        } else if !self.ctx.has_symbol_file_index(member_id) {
            self.ctx.register_symbol_file_target(member_id, file_idx);
        }
    }

    /// Resolve an unqualified name by checking exports of enclosing namespace(s).
    ///
    /// When code inside `namespace A { ... }` in file2 references `Point`,
    /// and `Point` is exported from `namespace A` in file1, the normal scope
    /// chain only sees file2's namespace body. This method walks up the AST
    /// to find enclosing `MODULE_DECLARATION` nodes and checks their merged
    /// symbol exports for the name.
    pub(crate) fn resolve_unqualified_name_in_enclosing_namespace(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        // Only applies in global scripts -- in external modules, namespaces
        // in different files do NOT merge (each file is its own module).
        if self.ctx.binder.is_external_module() {
            return None;
        }

        let arena = self.ctx.arena;
        let mut current = node_idx;

        // Walk up the AST looking for enclosing MODULE_DECLARATION nodes
        for _ in 0..100 {
            let ext = arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let parent_node = arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Found an enclosing namespace. Get its name.
                if let Some(module_data) = arena.get_module(parent_node)
                    && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
                {
                    // Same-block namespace members are visible inside the block
                    // even when they are not exported. Consult the namespace
                    // body's persistent scope before falling back to exports.
                    if module_data.body.is_some()
                        && let Some(&scope_id) =
                            self.ctx.binder.node_scope_ids.get(&module_data.body.0)
                        && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                        && let Some(member_id) = scope.table.get(name)
                    {
                        let is_enum_member = self
                            .ctx
                            .binder
                            .get_symbol(member_id)
                            .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                        if !is_enum_member {
                            return Some(member_id);
                        }
                    }

                    let ns_name = ns_name_ident.escaped_text.as_str();
                    // Look up the name in the merged namespace's exports
                    // First check the global symbol directly
                    if let Some(ns_sym_id) = self.ctx.binder.file_locals.get(ns_name)
                        && let Some(ns_sym) = self.ctx.binder.get_symbol(ns_sym_id)
                        && let Some(exports) = ns_sym.exports.as_ref()
                        && let Some(member_id) = exports.get(name)
                    {
                        // Filter out enum members - they should only be accessible via qualified form
                        let is_enum_member = self
                            .ctx
                            .binder
                            .get_symbol(member_id)
                            .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                        if !is_enum_member {
                            return Some(member_id);
                        }
                    }
                    // Also try cross-file resolution via all binders
                    if let Some(member_id) =
                        self.resolve_namespace_member_from_all_binders(ns_name, name)
                    {
                        return Some(member_id);
                    }
                }
            }
            current = parent_idx;
        }
        None
    }

    pub(crate) fn resolve_unqualified_name_in_enclosing_namespace_for_type_position(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        if self.ctx.binder.is_external_module() {
            return None;
        }

        let arena = self.ctx.arena;
        let mut current = node_idx;

        let member_is_usable_in_type_position = |sym_id: SymbolId| {
            let lib_binders = self.get_lib_binders();
            let flags = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map_or(0, |symbol| symbol.flags);
            if (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0 {
                return true;
            }
            !((self.alias_resolves_to_value_only(sym_id, Some(name))
                || self.symbol_is_value_only(sym_id, Some(name)))
                && !self.symbol_is_type_only(sym_id, Some(name)))
        };

        for _ in 0..100 {
            let ext = arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let parent_node = arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_data) = arena.get_module(parent_node)
                && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
            {
                if module_data.body.is_some()
                    && let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&module_data.body.0)
                    && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                    && let Some(member_id) = scope.table.get(name)
                {
                    let is_enum_member = self
                        .ctx
                        .binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                    if !is_enum_member && member_is_usable_in_type_position(member_id) {
                        return Some(member_id);
                    }
                }

                let ns_name = ns_name_ident.escaped_text.as_str();
                if let Some(ns_sym_id) = self.ctx.binder.file_locals.get(ns_name)
                    && let Some(ns_sym) = self.ctx.binder.get_symbol(ns_sym_id)
                    && let Some(exports) = ns_sym.exports.as_ref()
                    && let Some(member_id) = exports.get(name)
                {
                    let is_enum_member = self
                        .ctx
                        .binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                    if !is_enum_member && member_is_usable_in_type_position(member_id) {
                        return Some(member_id);
                    }
                }

                if let Some(member_id) =
                    self.resolve_namespace_member_from_all_binders(ns_name, name)
                    && member_is_usable_in_type_position(member_id)
                {
                    return Some(member_id);
                }
            }
            current = parent_idx;
        }
        None
    }

    /// Inner implementation of qualified symbol resolution with cycle detection.
    pub(crate) fn resolve_qualified_symbol_inner(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
        depth: usize,
    ) -> Option<SymbolId> {
        // Prevent stack overflow from deeply nested qualified names
        const MAX_QUALIFIED_NAME_DEPTH: usize = 128;
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return None;
        }

        let node = self.ctx.arena.get(idx)?;

        // Skip through parenthesized expressions: `(M).y` should resolve the
        // same qualified symbol as `M.y`.
        if node.kind == tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                return self.resolve_qualified_symbol_inner(
                    paren.expression,
                    visited_aliases,
                    depth + 1,
                );
            }
            return None;
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(idx)?;
            // Preserve alias symbols when alias resolution has no concrete target
            // (e.g., `import X = require("...")` namespace-like aliases).
            return self
                .resolve_alias_symbol(sym_id, visited_aliases)
                .or(Some(sym_id));
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let literal = self.ctx.arena.get_literal(node)?;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&literal.text) {
                return self.resolve_alias_symbol(sym_id, visited_aliases);
            }
            return None;
        }

        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left_sym =
                self.resolve_qualified_symbol_inner(access.expression, visited_aliases, depth + 1)?;
            let left_sym = self
                .resolve_alias_symbol(left_sym, visited_aliases)
                .unwrap_or(left_sym);
            let right_name = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.as_str())?;

            let lib_binders = self.get_lib_binders();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym, &lib_binders)?;

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            if let Some(ref module_specifier) = left_symbol.import_module {
                if (left_symbol.flags & symbol_flags::ALIAS) != 0
                    && self
                        .ctx
                        .module_resolves_to_non_module_entity(module_specifier)
                {
                    return None;
                }
                return self.resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    visited_aliases,
                );
            }

            if let Some(reexported_sym) =
                self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
            {
                return Some(reexported_sym);
            }

            // Cross-file namespace merging fallback: if the member wasn't found in
            // the resolved symbol's exports, check other files' namespace declarations
            // with the same name. This handles `namespace A` declared across files.
            if left_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    left_symbol.escaped_name.as_str(),
                    right_name,
                )
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            return None;
        }

        if node.kind != tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qn = self.ctx.arena.get_qualified_name(node)?;
        let left_sym = self.resolve_qualified_symbol_inner(qn.left, visited_aliases, depth + 1)?;
        let left_sym = self
            .resolve_alias_symbol(left_sym, visited_aliases)
            .unwrap_or(left_sym);
        let right_name = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        let lib_binders = self.get_lib_binders();
        let left_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(left_sym, &lib_binders)?;

        // First try direct exports
        if let Some(exports) = left_symbol.exports.as_ref()
            && let Some(member_sym) = exports.get(right_name)
        {
            return Some(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        // If not found in direct exports, check for re-exports
        // This handles cases like: export { foo } from './bar'
        if let Some(ref module_specifier) = left_symbol.import_module {
            if (left_symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return None;
            }
            if let Some(reexported_sym) =
                self.resolve_reexported_member_symbol(module_specifier, right_name, visited_aliases)
            {
                return Some(reexported_sym);
            }
        }

        if let Some(reexported_sym) =
            self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
        {
            return Some(reexported_sym);
        }

        // Cross-file namespace merging fallback for qualified names in type position.
        if left_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                left_symbol.escaped_name.as_str(),
                right_name,
            )
        {
            return Some(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        None
    }

    fn resolve_member_from_import_equals_alias(
        &self,
        alias_sym: SymbolId,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.ctx.binder.get_symbol(alias_sym)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return None;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_some()
            && let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            && let Some(import) = self.ctx.arena.get_import_decl(decl_node)
        {
            if let Some(module_specifier) =
                self.get_require_module_specifier(import.module_specifier)
            {
                if self
                    .ctx
                    .module_resolves_to_non_module_entity(&module_specifier)
                {
                    return None;
                }
                return self.resolve_reexported_member_symbol(
                    &module_specifier,
                    member_name,
                    visited_aliases,
                );
            }

            let target_sym = self.resolve_qualified_symbol(import.module_specifier)?;
            let lib_binders = self.get_lib_binders();
            let target_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(target_sym, &lib_binders)?;

            if let Some(exports) = target_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(member_name)
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            if let Some(members) = target_symbol.members.as_ref()
                && let Some(member_sym) = members.get(member_name)
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            if target_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    target_symbol.escaped_name.as_str(),
                    member_name,
                )
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }
        }

        None
    }

    /// Resolve a re-exported member symbol by following re-export chains.
    ///
    /// This function handles cases where a namespace member is re-exported from
    /// another module using `export { foo } from './bar'` or `export * from './bar'`.
    pub(crate) fn resolve_reexported_member_symbol(
        &self,
        module_specifier: &str,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let mut visited_modules = rustc_hash::FxHashSet::default();
        self.resolve_reexported_member_symbol_inner(
            module_specifier,
            member_name,
            visited_aliases,
            &mut visited_modules,
        )
    }

    fn resolve_member_from_module_exports(
        &self,
        binder: &tsz_binder::BinderState,
        exports_table: &tsz_binder::SymbolTable,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let can_resolve_aliases = std::ptr::eq(binder, self.ctx.binder);

        if let Some(sym_id) = exports_table.get(member_name) {
            if can_resolve_aliases {
                return Some(
                    self.resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id),
                );
            }
            return Some(sym_id);
        }

        let export_equals_sym = exports_table.get("export=")?;
        let mut candidate_symbol_ids = vec![export_equals_sym];
        if can_resolve_aliases {
            let resolved_export_equals = self
                .resolve_alias_symbol(export_equals_sym, visited_aliases)
                .unwrap_or(export_equals_sym);
            if resolved_export_equals != export_equals_sym {
                candidate_symbol_ids.push(resolved_export_equals);
            }
        }

        for candidate_symbol_id in candidate_symbol_ids {
            let Some(target_symbol) = binder.get_symbol(candidate_symbol_id) else {
                continue;
            };

            if let Some(exports) = target_symbol.exports.as_ref()
                && let Some(sym_id) = exports.get(member_name)
            {
                if can_resolve_aliases {
                    return Some(
                        self.resolve_alias_symbol(sym_id, visited_aliases)
                            .unwrap_or(sym_id),
                    );
                }
                return Some(sym_id);
            }

            if let Some(members) = target_symbol.members.as_ref()
                && let Some(sym_id) = members.get(member_name)
            {
                if can_resolve_aliases {
                    return Some(
                        self.resolve_alias_symbol(sym_id, visited_aliases)
                            .unwrap_or(sym_id),
                    );
                }
                return Some(sym_id);
            }

            // Some binder states keep the namespace merge partner as a distinct symbol.
            // Search same-name symbols with module namespace flags for members.
            for &merged_candidate_id in binder
                .get_symbols()
                .find_all_by_name(&target_symbol.escaped_name)
            {
                let Some(merged_symbol) = binder.get_symbol(merged_candidate_id) else {
                    continue;
                };
                if (merged_symbol.flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE))
                    == 0
                {
                    continue;
                }

                if let Some(exports) = merged_symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    if can_resolve_aliases {
                        return Some(
                            self.resolve_alias_symbol(sym_id, visited_aliases)
                                .unwrap_or(sym_id),
                        );
                    }
                    return Some(sym_id);
                }

                if let Some(members) = merged_symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    if can_resolve_aliases {
                        return Some(
                            self.resolve_alias_symbol(sym_id, visited_aliases)
                                .unwrap_or(sym_id),
                        );
                    }
                    return Some(sym_id);
                }
            }
        }

        None
    }

    fn resolve_module_augmentation_member_symbol(
        &self,
        module_specifier: &str,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if let Some(augmentation) = self
            .get_module_augmentation_declarations(module_specifier, member_name)
            .into_iter()
            .next()
        {
            let binder = augmentation
                .arena
                .as_deref()
                .and_then(|arena| self.ctx.get_binder_for_arena(arena))
                .unwrap_or(self.ctx.binder);
            let sym_id = binder.get_node_symbol(augmentation.node)?;
            if std::ptr::eq(binder, self.ctx.binder) {
                return Some(
                    self.resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id),
                );
            }
            return Some(sym_id);
        }

        None
    }

    /// Inner implementation with cycle detection for module re-exports.
    fn resolve_reexported_member_symbol_inner(
        &self,
        module_specifier: &str,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
        visited_modules: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<SymbolId> {
        // Cycle detection: check if we've already visited this (module, member) pair
        let key = (module_specifier.to_string(), member_name.to_string());
        if visited_modules.contains(&key) {
            return None;
        }
        visited_modules.insert(key);

        // First, check if it's a direct export from this module (ambient modules)
        if let Some(module_exports) = self.ctx.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = self.resolve_member_from_module_exports(
                self.ctx.binder,
                module_exports,
                member_name,
                visited_aliases,
            )
        {
            return Some(sym_id);
        }

        // Cross-file resolution: use canonical file-key lookups via state_type_resolution.
        if let Some(sym_id) = self.resolve_cross_file_export(module_specifier, member_name) {
            return Some(
                self.resolve_alias_symbol(sym_id, visited_aliases)
                    .unwrap_or(sym_id),
            );
        }

        // Check for named re-exports: `export { foo } from 'bar'`
        if let Some(file_reexports) = self.ctx.binder.reexports.get(module_specifier)
            && let Some((source_module, original_name)) = file_reexports.get(member_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
            return self.resolve_reexported_member_symbol_inner(
                source_module,
                name_to_lookup,
                visited_aliases,
                visited_modules,
            );
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // TSC behavior: If two `export *` declarations export the same name,
        // that name is considered AMBIGUOUS and is NOT exported
        // (unless explicitly re-exported by name, which is checked above).
        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_specifier) {
            let mut found_result: Option<SymbolId> = None;
            let mut found_count = 0;

            for source_module in source_modules {
                if let Some(sym_id) = self.resolve_reexported_member_symbol_inner(
                    source_module,
                    member_name,
                    visited_aliases,
                    visited_modules,
                ) {
                    found_count += 1;
                    if found_count == 1 {
                        found_result = Some(sym_id);
                    } else {
                        // Multiple sources export the same name - ambiguous, treat as not exported
                        return None;
                    }
                }
            }

            if found_result.is_some() {
                return found_result;
            }
        }

        None
    }
}
