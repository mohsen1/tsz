//! Duplicate import-equals alias diagnostics.

use crate::state::CheckerState;
use std::collections::HashMap;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

/// Whether `kind` starts a new declaration container for an `import X = ...`
/// alias.
///
/// `tsc`'s binder advances its `container` cursor at the source file, a module
/// (namespace) body, a class body, and every function-like node — never at a
/// plain `Block` or other block-scope-only construct, because an alias is not
/// block-scoped. Grouping duplicate aliases therefore has to key on these
/// nodes, and the set is exactly the complement of the transparent statements
/// [`CheckerState::collect_import_equals_transparently`] walks through.
const fn is_alias_declaration_container(kind: u16) -> bool {
    matches!(
        kind,
        syntax_kind_ext::SOURCE_FILE
            | syntax_kind_ext::MODULE_DECLARATION
            | syntax_kind_ext::MODULE_BLOCK
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::CLASS_EXPRESSION
            | syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            | syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::CONSTRUCTOR
            | syntax_kind_ext::GET_ACCESSOR
            | syntax_kind_ext::SET_ACCESSOR
    )
}

/// Whether a container of `kind` already has its own
/// [`CheckerState::check_import_alias_duplicates`] call site.
///
/// The source file and namespace bodies are scanned directly from
/// `check_source_file` / the module-declaration bridge; re-grouping them in the
/// nested-container sweep would report every duplicate twice. A class body is
/// listed here too — an `import X = ...` cannot be a direct class member, so an
/// alias whose nearest container is the class itself does not exist.
const fn container_has_dedicated_scan(kind: u16) -> bool {
    matches!(
        kind,
        syntax_kind_ext::SOURCE_FILE
            | syntax_kind_ext::MODULE_DECLARATION
            | syntax_kind_ext::MODULE_BLOCK
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::CLASS_EXPRESSION
    )
}

impl<'a> CheckerState<'a> {
    /// Collect every `import X = ...` declaration reachable from `stmt_idx`
    /// without crossing a declaration-container boundary.
    ///
    /// `tsc`'s binder keeps two cursors while binding: `container` (source
    /// file, module body, class body, or any function-like node) and
    /// `blockScopeContainer` (additionally every plain `Block`/`if`/`for`/
    /// `try`/`switch`/... body). An alias is not block-scoped, so a
    /// position-invalid `import x = ...` nested inside `if`/`for`/`try`/
    /// `switch`/labeled/`with` bodies is recorded in the *container*, exactly
    /// like a bare `{ }` block (see #16428). This walk mirrors that
    /// transparency so two aliases nested in different blocks of the same
    /// container are grouped together for the TS2300 check below, while a
    /// function body, class body, or nested namespace — genuine containers —
    /// stop the walk, matching `nearest_declaration_container_scope`.
    fn collect_import_equals_transparently(&self, stmt_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => out.push(stmt_idx),
            syntax_kind_ext::BLOCK => {
                // `get_block` also covers CLASS_STATIC_BLOCK_DECLARATION and
                // CASE_BLOCK, both excluded above/below — this arm only ever
                // sees a plain `{ }` block here.
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &inner in &block.statements.nodes {
                        self.collect_import_equals_transparently(inner, out);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.collect_import_equals_transparently(if_stmt.then_statement, out);
                    if if_stmt.else_statement.is_some() {
                        self.collect_import_equals_transparently(if_stmt.else_statement, out);
                    }
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_import_equals_transparently(loop_data.statement, out);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_import_equals_transparently(for_in_of.statement, out);
                }
            }
            syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.ctx.arena.get_with_statement(node) {
                    self.collect_import_equals_transparently(with_stmt.then_statement, out);
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_import_equals_transparently(try_data.try_block, out);
                    if try_data.catch_clause.is_some()
                        && let Some(catch_node) = self.ctx.arena.get(try_data.catch_clause)
                        && let Some(catch_data) = self.ctx.arena.get_catch_clause(catch_node)
                    {
                        self.collect_import_equals_transparently(catch_data.block, out);
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_import_equals_transparently(try_data.finally_block, out);
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                            continue;
                        };
                        if let Some(clause) = self.ctx.arena.get_case_clause(clause_node) {
                            for &inner in &clause.statements.nodes {
                                self.collect_import_equals_transparently(inner, out);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_import_equals_transparently(labeled.statement, out);
                }
            }
            _ => {}
        }
    }

    /// Check for duplicate import alias declarations within a scope.
    ///
    /// TS2300: Emitted when multiple `import X = ...` declarations have the same name
    /// within the same scope (namespace, module, or file), including a
    /// position-invalid alias nested in a different block of that scope
    /// (`{ import x = ...; } { import x = ...; }`, #16429).
    pub(crate) fn check_import_alias_duplicates(&mut self, statements: &[NodeIndex]) {
        let mut alias_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        let mut collected = Vec::new();
        for &stmt_idx in statements {
            self.collect_import_equals_transparently(stmt_idx, &mut collected);
        }

        for stmt_idx in collected {
            // `collect_import_equals_transparently` yields only `import =` nodes.
            let Some(alias_name) = self.import_equals_alias_name(stmt_idx) else {
                continue;
            };
            alias_map.entry(alias_name).or_default().push(stmt_idx);
        }

        for (alias_name, indices) in alias_map {
            self.report_duplicate_alias_group(&alias_name, &indices);
        }
    }

    /// Report TS2300 (or the wrong-meaning diagnostic) at every alias in one
    /// same-name group. A group of one is not a duplicate and reports nothing.
    fn report_duplicate_alias_group(&mut self, alias_name: &str, indices: &[NodeIndex]) {
        if indices.len() <= 1 {
            return;
        }

        for &import_idx in indices {
            let Some(import_node) = self.ctx.arena.get(import_idx) else {
                continue;
            };
            let Some(import_decl) = self.ctx.arena.get_import_decl(import_node) else {
                continue;
            };

            let alias_node = import_decl.import_clause;
            let Some(sym_id) = self.resolve_identifier_symbol(alias_node) else {
                tracing::trace!("Could not resolve identifier symbol");
                continue;
            };
            let symbol = self
                .ctx
                .binder
                .symbols
                .get(sym_id)
                .expect("sym_id resolved from resolve_identifier_symbol");
            tracing::trace!("Symbol flags: {:?}", symbol.flags);
            if self.symbol_is_value_only(sym_id, Some(alias_name)) {
                self.report_wrong_meaning_diagnostic(
                    alias_name,
                    import_decl.import_clause,
                    crate::query_boundaries::name_resolution::NameLookupKind::Value,
                );
            } else {
                self.error_at_node(
                    import_decl.import_clause,
                    &format!("Duplicate identifier '{alias_name}'."),
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }
    }

    /// The nearest enclosing declaration container of `idx`, or `None` when the
    /// parent chain is incomplete.
    fn nearest_alias_container(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut cursor = self.ctx.arena.parent_of(idx)?;
        while cursor.is_some() {
            let kind = self.ctx.arena.get(cursor)?.kind;
            if is_alias_declaration_container(kind) {
                return Some(cursor);
            }
            cursor = self.ctx.arena.parent_of(cursor)?;
        }
        None
    }

    /// The alias name an `import X = ...` declaration binds, or `None` for a
    /// non-alias node.
    fn import_equals_alias_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import_decl = self.ctx.arena.get_import_decl(node)?;
        let alias_node = self.ctx.arena.get(import_decl.import_clause)?;
        let alias_id = self.ctx.arena.get_identifier(alias_node)?;
        Some(alias_id.escaped_text.to_string())
    }

    /// Every `import X = ...` declaration that collides with the one at
    /// `node_idx` — same alias name, same nearest declaration container — as a
    /// group of two or more, or `None` when `node_idx` is not a colliding alias.
    ///
    /// This is the grouping [`Self::check_import_alias_duplicates`] and
    /// [`Self::check_import_alias_duplicates_in_nested_containers`] key TS2300
    /// on, reused from the specifier-resolution side: `tsc` resolves at most one
    /// specifier per colliding group (the first declaration by source position),
    /// because each alias binds its own distinct `SymbolId` and only the group's
    /// first reaches `resolveExternalModuleName`. Returned in source-position
    /// order so the caller can identify that first declaration.
    pub(crate) fn import_equals_colliding_group(
        &self,
        node_idx: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
        // Name first: it short-circuits (and skips the container parent-walk) for
        // a non-`import =` node, the common caller.
        let name = self.import_equals_alias_name(node_idx)?;
        let container = self.nearest_alias_container(node_idx)?;

        let mut group: Vec<NodeIndex> = self
            .ctx
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            .map(|(raw, _)| NodeIndex(raw as u32))
            // Name match (a few arena reads) before the container parent-walk.
            .filter(|&idx| self.import_equals_alias_name(idx).as_deref() == Some(name.as_str()))
            .filter(|&idx| self.nearest_alias_container(idx) == Some(container))
            .collect();

        if group.len() <= 1 {
            return None;
        }
        group.sort_by_key(|&idx| self.ctx.arena.get(idx).map(|n| n.pos).unwrap_or(u32::MAX));
        Some(group)
    }

    /// TS2300 for duplicate `import X = ...` aliases inside a function-like
    /// body or a class static block.
    ///
    /// [`Self::check_import_alias_duplicates`] is only ever invoked with a
    /// source file's or a namespace body's statement list, and its transparent
    /// walk deliberately stops at a function body — so two same-name aliases
    /// directly inside `function f() { ... }`, a method, a constructor, an
    /// accessor, an arrow/function expression body, or a `static { ... }` block
    /// were never grouped by anything and reported only TS1232. `tsc` reports
    /// TS1232 *and* TS2300 at each of them, because its binder records a
    /// non-block-scoped alias in the enclosing container and the redeclaration
    /// collides there.
    ///
    /// This sweep closes that gap for every such container at once by grouping
    /// each alias under [`Self::nearest_alias_container`] instead of relying on
    /// a call site per container kind — arrow and function-expression bodies
    /// hang off expressions, so no statement-list-driven pass reaches them.
    /// Containers that already have a dedicated scan are skipped, so nothing is
    /// reported twice.
    pub(crate) fn check_import_alias_duplicates_in_nested_containers(&mut self) {
        // Aliases are rare and the pools are per-file; skip the sweep entirely
        // when the file parsed no import-like declaration at all.
        if self.ctx.arena.import_decls.is_empty() {
            return;
        }

        // One sequential pass over the thin node headers. Aliases are rare, so
        // the parent walk and the grouping below only run for the few hits;
        // everything else costs a single `u16` compare over contiguous memory.
        let alias_nodes: Vec<NodeIndex> = self
            .ctx
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            .map(|(raw, _)| NodeIndex(raw as u32))
            .collect();
        if alias_nodes.is_empty() {
            return;
        }

        let mut groups: HashMap<(NodeIndex, String), Vec<NodeIndex>> = HashMap::new();
        // Grouping keys are recorded in arena order so the diagnostics this
        // sweep emits do not depend on `HashMap` iteration order.
        let mut group_order: Vec<(NodeIndex, String)> = Vec::new();

        for idx in alias_nodes {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            let Some(container) = self.nearest_alias_container(idx) else {
                continue;
            };
            let Some(container_kind) = self.ctx.arena.get(container).map(|n| n.kind) else {
                continue;
            };
            if container_has_dedicated_scan(container_kind) {
                continue;
            }

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };
            let Some(alias_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(alias_id) = self.ctx.arena.get_identifier(alias_node) else {
                continue;
            };

            let key = (container, alias_id.escaped_text.to_string());
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    group_order.push(key.clone());
                    Vec::new()
                })
                .push(idx);
        }

        for key in group_order {
            let Some(indices) = groups.remove(&key) else {
                continue;
            };
            self.report_duplicate_alias_group(&key.1, &indices);
        }
    }

    /// TS2300 for duplicate ES import declaration local bindings.
    ///
    /// `import { x } ...; import { y as x } ...;` reports duplicate
    /// identifiers at both local binding names, independent of whether module
    /// resolution succeeds.
    pub(crate) fn check_import_declaration_duplicate_bindings(&mut self, statements: &[NodeIndex]) {
        let mut binding_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && let Some(ident) = self.ctx.arena.get_identifier_at(clause.name)
            {
                binding_map
                    .entry(ident.escaped_text.to_string())
                    .or_default()
                    .push(clause.name);
            }

            let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                    && let Some(ident) = self.ctx.arena.get_identifier_at(ns.name)
                {
                    binding_map
                        .entry(ident.escaped_text.to_string())
                        .or_default()
                        .push(ns.name);
                }
                continue;
            }

            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                continue;
            }

            let Some(named) = self.ctx.arena.get_named_imports(bindings_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                    continue;
                };
                let local_name_idx = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                if local_name_idx.is_some()
                    && let Some(ident) = self.ctx.arena.get_identifier_at(local_name_idx)
                {
                    binding_map
                        .entry(ident.escaped_text.to_string())
                        .or_default()
                        .push(local_name_idx);
                }
            }
        }

        for (name, binding_indices) in binding_map {
            if binding_indices.len() <= 1 {
                continue;
            }
            for binding_idx in binding_indices {
                self.error_at_node(
                    binding_idx,
                    &format!("Duplicate identifier '{name}'."),
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }
    }
}
