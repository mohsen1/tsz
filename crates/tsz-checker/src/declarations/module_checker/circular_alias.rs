//! Circular import alias detection (TS2303).
//!
//! An alias declaration whose target resolves back to the alias itself is
//! circular. tsc reports TS2303 once at *every* alias declaration on the cycle,
//! which is more sites than this binder's symbol graph exposes directly: two of
//! the forms that tsc gives their own alias symbol — `export = X` and a local
//! `export { X as Y }` — are folded onto the symbol they name here, so their
//! anchors have to be recovered syntactically from the declaring container.
//! Both companion scans live next to the walk that finds the cycle.

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;

impl CheckerState<'_> {
    /// Walk up from `node_idx` looking for a `MODULE_DECLARATION` whose name is
    /// a string literal (i.e., `declare module "<specifier>"`). Returns the
    /// specifier text. Used by TS2303 cycle suppression to compare a
    /// `require()` target against the enclosing ambient module's name.
    fn enclosing_ambient_module_specifier(&self, node_idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = node_idx;
        for _ in 0..64 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent == NodeIndex::NONE || parent == current {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && let Some(lit) = self.ctx.arena.get_literal(name_node)
            {
                return Some(lit.text.to_string());
            }
            current = parent;
        }
        None
    }

    /// Walk up from `node_idx` to the enclosing `declare module "<specifier>"`
    /// declaration and return both its specifier text and its body node.
    ///
    /// `enclosing_ambient_module_specifier` answers "which module am I in?";
    /// this answers "which statement list declares me?", which is what the
    /// TS2303 companion-`export =` scan needs in order to look inside an
    /// ambient module body instead of only at file top level.
    fn enclosing_ambient_module_block(&self, node_idx: NodeIndex) -> Option<(String, NodeIndex)> {
        use tsz_parser::parser::syntax_kind_ext;
        let mut current = node_idx;
        for _ in 0..64 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent == NodeIndex::NONE || parent == current {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && let Some(lit) = self.ctx.arena.get_literal(name_node)
            {
                return Some((lit.text.to_string(), module.body));
            }
            current = parent;
        }
        None
    }

    /// The statements of the container that declares `decl_idx`, paired with a
    /// check of whether that container's `export =` names `sym_id`.
    ///
    /// An `export = X` never becomes a declaration of `X`'s symbol (see the
    /// call site), so the export side has to be found syntactically, within
    /// whichever statement list actually declares the alias: an ambient
    /// module's body when the alias sits in one, the source file otherwise.
    fn export_equals_sites_for_cyclic_alias(
        &self,
        decl_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
    ) -> Vec<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let owns_export_equals = match self.enclosing_ambient_module_block(decl_idx) {
            Some((specifier, _)) => {
                self.ctx
                    .binder
                    .module_exports
                    .get(&specifier)
                    .and_then(|exports| exports.get("export="))
                    == Some(sym_id)
            }
            None => self.ctx.binder.file_locals.get("export=") == Some(sym_id),
        };

        if !owns_export_equals {
            return Vec::new();
        }

        self.declaring_container_statements(decl_idx)
            .into_iter()
            .filter(|&stmt_idx| {
                self.ctx.arena.get(stmt_idx).is_some_and(|stmt_node| {
                    stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        && self
                            .ctx
                            .arena
                            .get_export_assignment(stmt_node)
                            .is_some_and(|assign| assign.is_export_equals)
                })
            })
            .collect()
    }

    /// The statements of the container that declares `decl_idx`, as a node list.
    ///
    /// An ambient module's own body when `decl_idx` sits inside one, the source
    /// file otherwise. Shared by the two syntactic companion-site scans below,
    /// which both have to stay inside the container the collection scan in
    /// `check_circular_import_aliases` owns.
    fn declaring_container_statements(&self, decl_idx: NodeIndex) -> Vec<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        match self.enclosing_ambient_module_block(decl_idx) {
            Some((_, body)) => self
                .ctx
                .arena
                .get(body)
                .filter(|body_node| body_node.kind == syntax_kind_ext::MODULE_BLOCK)
                .and_then(|body_node| self.ctx.arena.get_module_block(body_node))
                .and_then(|block| block.statements.as_ref())
                .map(|statements| statements.nodes.clone())
                .unwrap_or_default(),
            None => self
                .ctx
                .arena
                .source_files
                .first()
                .map(|source_file| source_file.statements.nodes.clone())
                .unwrap_or_default(),
        }
    }

    /// Local `export { X as Y }` specifier sites, in the container that declares
    /// `decl_idx`, whose exported target is `sym_id`.
    ///
    /// An export specifier with **no `from` clause** does not get its own alias
    /// symbol in this binder. `bind_export_declaration` resolves the specifier's
    /// local name in the current scope and publishes the exported name onto that
    /// *existing* symbol, so a cyclic local alias and every local re-export of it
    /// share one `SymbolId` and the walk in `check_circular_import_aliases` sees
    /// a shorter cycle than tsc does. tsc gives each specifier its own alias
    /// symbol, puts it on the same cycle, and reports TS2303 at every member —
    /// so the extra anchors have to be recovered syntactically, exactly as
    /// `export =` does above and for the same binder reason.
    ///
    /// A specifier that *does* carry a `from` clause is left alone: that form
    /// sets `ALIAS` + `import_module` and is already its own entry in the
    /// collection scan.
    ///
    /// Returns the specifier node — tsc anchors the whole `X as Y` node, not its
    /// name — paired with the exported name the message renders.
    fn local_export_specifier_sites_for_cyclic_alias(
        &self,
        decl_idx: NodeIndex,
        sym_id: tsz_binder::SymbolId,
        cycle_entry_export_name: &str,
    ) -> Vec<(NodeIndex, String)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut sites = Vec::new();
        for stmt_idx in self.declaring_container_statements(decl_idx) {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() {
                continue;
            }
            let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
                continue;
            };

            for &spec_idx in &named_exports.elements.nodes {
                // The specifier that *is* this symbol's own anchor is already
                // reported by the caller; only genuine companions get a site.
                if spec_idx == decl_idx {
                    continue;
                }
                let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                    continue;
                };
                // `X as Y` names local `X`; a bare `X` names local `X`.
                let local_name_idx = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(local_name) = self.get_identifier_text_from_idx(local_name_idx) else {
                    continue;
                };
                if self.resolve_name_at_node(&local_name, local_name_idx) != Some(sym_id) {
                    continue;
                }
                // The message renders the alias tsc would have created, i.e.
                // the *exported* name: `Y` for `X as Y`, `X` for a bare `X`.
                let exported_name_idx = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(exported_name) = self.get_identifier_text_from_idx(exported_name_idx)
                else {
                    continue;
                };
                // Only the specifier the cycle actually re-enters under is a
                // member of it. A second specifier for the same local
                // (`export { X as B, X as C }`) resolves through an already
                // resolved alias, so tsc never revisits it and never reports it.
                if exported_name != cycle_entry_export_name {
                    continue;
                }
                sites.push((spec_idx, exported_name));
            }
        }
        sites
    }

    /// Eagerly checks all alias symbols in the current file for circular definitions.
    /// Emits TS2303 for any alias that circularly references itself.
    pub(crate) fn check_circular_import_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let is_js_file = self.ctx.is_js_file();

        // Collect ALIAS symbols only from scope tables, not from the full symbol arena.
        // After multi-file merge, the global symbol arena contains symbols from ALL files.
        // Iterating symbols.iter() would cause each file to check every file's symbols,
        // leading to duplicate TS2303 emissions. Scope tables contain only this file's symbols.
        let mut local_alias_ids: Vec<tsz_binder::SymbolId> = Vec::new();
        for scope in self.ctx.binder.scopes.iter() {
            for (_, &sym_id) in scope.table.iter() {
                if let Some(s) = self.ctx.binder.symbols.get(sym_id)
                    && s.has_any_flags(symbol_flags::ALIAS)
                    && !s.is_umd_export
                {
                    local_alias_ids.push(sym_id);
                }
            }
        }
        local_alias_ids.sort_unstable_by_key(|s| s.0);
        local_alias_ids.dedup();

        for sym_id in local_alias_ids {
            let sym = match self.ctx.binder.symbols.get(sym_id) {
                Some(s) => s,
                None => continue,
            };

            // In JS files, `import x = require(...)` is TS-only syntax (TS8002).
            // tsc skips semantic analysis for such statements — skip circular check.
            if is_js_file {
                let decl_idx = sym.primary_declaration().unwrap_or(NodeIndex::NONE);
                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    continue;
                }
            }

            let mut current_binder = self.ctx.binder;
            let mut current_file_idx = self.ctx.current_file_idx;
            let mut current_sym_id = sym_id;
            let mut visited = Vec::new();
            let mut visited_sym_ids = Vec::new();
            let mut cycle_detected = false;
            // The export name the most recent hop resolved through, and the one
            // the hop that closed the cycle used. A local `export { X as Y }`
            // only participates in the cycle when the cycle re-enters this file
            // under `Y`; a second specifier for the same local (`X as Z`) is a
            // branch off the cycle, which tsc does not report. Carrying the
            // re-entry name out of the walk is what separates the two.
            let mut last_hop_export_name: Option<String> = None;
            let mut cycle_entry_export_name: Option<String> = None;

            for _ in 0..128 {
                let key = (current_file_idx, current_sym_id.0 as usize);
                if visited.contains(&key) {
                    cycle_entry_export_name = last_hop_export_name.clone();
                    if key.0 == self.ctx.current_file_idx && key.1 == sym_id.0 as usize {
                        // When we get an immediate self-reference (one-step cycle),
                        // it may be a self-import pattern:
                        //   export { f as g } from "./a";  // re-export
                        //   import { g } from "./b";       // self-import
                        // The binder merges both into one symbol. The self-import
                        // resolves to the merged symbol → appears circular.
                        // Don't flag it as circular if the symbol has a re-export
                        // declaration (EXPORT_SPECIFIER with a `from` clause) that
                        // points to a different module, providing a real resolution.
                        if visited.len() == 1 {
                            let has_reexport_from = sym.declarations.iter().any(|&decl_idx| {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                                {
                                    // Check if the parent export declaration has a module
                                    // specifier (`from "..."` clause).
                                    if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                                        let parent = ext.parent;
                                        if let Some(parent_node) = self.ctx.arena.get(parent)
                                            && parent_node.kind == syntax_kind_ext::NAMED_EXPORTS
                                            && let Some(grandparent_ext) =
                                                self.ctx.arena.get_extended(parent)
                                        {
                                            let gp = grandparent_ext.parent;
                                            if let Some(gp_node) = self.ctx.arena.get(gp)
                                                && gp_node.kind
                                                    == syntax_kind_ext::EXPORT_DECLARATION
                                                && let Some(export_decl) =
                                                    self.ctx.arena.get_export_decl(gp_node)
                                            {
                                                return export_decl.module_specifier.is_some();
                                            }
                                        }
                                    }
                                    false
                                } else {
                                    false
                                }
                            });
                            // `import X = require("m")` inside a different
                            // ambient module declaration (e.g.
                            //   declare module "m"      { export = T; }
                            //   declare module "node:m" { import m = require("m"); export = m; }
                            // ) names an external module — the alias resolves
                            // through `m`'s `export = ...`, not back to itself.
                            // Our binder can spuriously map the alias to itself
                            // because `m` is both a sibling declared-module
                            // specifier and an alias name in the same file.
                            // Suppress only for the cross-module-name case;
                            // genuine self-imports
                            //   declare module "moduleC" { import self = require("moduleC"); }
                            // remain TS2303.
                            let require_target_differs_from_enclosing_module =
                                sym.declarations.iter().any(|&decl_idx| {
                                    let Some(n) = self.ctx.arena.get(decl_idx) else {
                                        return false;
                                    };
                                    if n.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                                        return false;
                                    }
                                    let Some(imp) = self.ctx.arena.get_import_decl(n) else {
                                        return false;
                                    };
                                    let Some(target) =
                                        self.get_require_module_specifier(imp.module_specifier)
                                    else {
                                        return false;
                                    };
                                    let Some(enclosing) =
                                        self.enclosing_ambient_module_specifier(decl_idx)
                                    else {
                                        return false;
                                    };
                                    target != enclosing
                                });
                            if !has_reexport_from && !require_target_differs_from_enclosing_module {
                                cycle_detected = true;
                            }
                        } else {
                            cycle_detected = true;
                        }
                    }
                    break;
                }
                visited.push(key);
                visited_sym_ids.push(current_sym_id);

                let curr_sym = match current_binder.symbols.get(current_sym_id) {
                    Some(s) => s,
                    None => break,
                };

                if !curr_sym.has_any_flags(symbol_flags::ALIAS) {
                    break;
                }

                let mut found = false;
                // The export name this hop resolves through, owned up front so
                // it outlives the reference reseating below.
                let mut hop_export_name: Option<String> = None;

                // For import aliases with import_module, use cross-file resolution
                // to properly track which file we're resolving from.
                if let Some(module_name) = curr_sym.import_module() {
                    let export_name = curr_sym
                        .import_name()
                        .unwrap_or(curr_sym.escaped_name.as_str());
                    hop_export_name = Some(export_name.to_string());

                    // Use checker's cross-file module resolution first.
                    // This correctly resolves relative specifiers from the
                    // current file's perspective and switches to the target
                    // file's binder for subsequent resolution.
                    if let Some(target_idx) = self
                        .ctx
                        .resolve_import_target_from_file(current_file_idx, module_name)
                        && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
                    {
                        if let Some(target_sym_id) = target_binder
                            .resolve_import_with_reexports_type_only(module_name, export_name)
                            .map(|(sym_id, _)| sym_id)
                            .or_else(|| {
                                (curr_sym.import_name().is_none())
                                    .then(|| {
                                        target_binder
                                            .resolve_import_with_reexports_type_only(
                                                module_name,
                                                "export=",
                                            )
                                            .map(|(sym_id, _)| sym_id)
                                    })
                                    .flatten()
                            })
                        {
                            current_binder = target_binder;
                            current_file_idx = target_idx;
                            current_sym_id = target_sym_id;
                            found = true;
                        } else {
                            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                            if let Some(sf) = target_arena.source_files.first()
                                && let Some(exports) = self
                                    .ctx
                                    .module_exports_for_module(target_binder, &sf.file_name)
                            {
                                if let Some(target_sym_id) = exports.get(export_name) {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                } else if let Some(target_sym_id) = exports.get("export=") {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                }
                            }
                        }
                    }

                    // Fall back to binder-level resolution (same-file or merged binder)
                    if !found
                        && let Some(resolved_id) =
                            current_binder.resolve_import_symbol(current_sym_id)
                    {
                        current_sym_id = resolved_id;
                        found = true;
                    }

                    // Try current binder's module_exports directly
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get(export_name)
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get("export=")
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }

                    // Fall back to all_binders for cross-file resolution
                    if !found && let Some(binders) = &self.ctx.all_binders {
                        if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name)
                        {
                            for &idx in file_indices {
                                if let Some(b) = binders.get(idx)
                                    && let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        } else {
                            for (idx, b) in binders.iter().enumerate() {
                                if let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                } else if let Some(resolved_id) =
                    current_binder.resolve_import_symbol(current_sym_id)
                {
                    // Non-import alias (e.g., import = require(...)) — use binder resolution
                    current_sym_id = resolved_id;
                    found = true;
                }

                if !found
                    && std::ptr::eq(current_binder as *const _, self.ctx.binder as *const _)
                    && curr_sym.value_declaration.is_some()
                {
                    let decl_idx = curr_sym.value_declaration;
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
                    {
                        let mut base_node = import_decl.module_specifier;
                        while let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(qname) = self.ctx.arena.get_qualified_name(node)
                        {
                            base_node = qname.left;
                        }
                        if let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(ident) = self.ctx.arena.get_identifier(node)
                            && let Some(target_sym_id) =
                                self.resolve_name_at_node(&ident.escaped_text, base_node)
                        {
                            current_sym_id = target_sym_id;
                            found = true;
                        }
                    }
                }

                if !found {
                    break;
                }
                last_hop_export_name = hop_export_name;
            }

            if cycle_detected {
                // tsc 7.0.2 reports TS2303 once at EVERY alias declaration
                // participating in the cycle (each file flags its own
                // specifier; both specifiers within one file too), so there
                // is no cross-file dedup and no blanket same-file
                // suppression — each per-symbol iteration reports its own
                // alias at its own anchor.
                let Some(decl_idx) = sym.primary_declaration() else {
                    continue;
                };
                // `Symbol::first_declaration_span` already derives from the
                // first known `stable_declarations` entry with a
                // `stable_value_declaration` fallback, subsuming the previous
                // hand-rolled chain over the same data.
                let fallback_span = sym.first_declaration_span();

                let mut error_node_idx = decl_idx;

                if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                    if decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                        || decl_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                    {
                        if let Some(spec) = self.ctx.arena.get_specifier(decl_node) {
                            let name_idx = if spec.name.is_some() {
                                spec.name
                            } else {
                                spec.property_name
                            };
                            if name_idx.is_some() {
                                error_node_idx = name_idx;
                            }
                        }
                    } else if decl_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                        && let Some(import_clause) = self.ctx.arena.get_import_clause(decl_node)
                        && import_clause.name.is_some()
                    {
                        error_node_idx = import_clause.name;
                    }
                }

                let message = format_message(
                    diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                    &[&sym.escaped_name],
                );
                let code = diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS;
                if self.get_node_span(error_node_idx).is_some() {
                    self.error_at_node(error_node_idx, &message, code);
                } else if let Some((start, end)) = fallback_span {
                    self.error(start, end.saturating_sub(start), message.clone(), code);
                }

                // `export = X` does NOT create its own alias symbol: the binder
                // records `"export="` in `file_locals` as a second KEY onto X's
                // existing `SymbolId` (nodes/binding.rs, EXPORT_ASSIGNMENT arm),
                // and never registers the `ExportAssignment` node as one of X's
                // declarations. So the export side is unreachable from the
                // symbol — neither by collecting more symbols nor by iterating
                // `sym.declarations` — and the cycle could only ever be reported
                // from the import side.
                //
                // tsc reports at EVERY alias declaration in the cycle, so when
                // the `export =` that owns this symbol names it, emit there
                // too. The scan is scoped to the container that declares the
                // alias — an ambient module's own body when the cycle is
                // written inside `declare module "..."`, the source file
                // otherwise — which preserves the current-file ownership the
                // collection scan above is built on while still reaching a
                // cycle nested in an ambient module.
                for stmt_idx in self.export_equals_sites_for_cyclic_alias(decl_idx, sym_id) {
                    self.error_at_node(stmt_idx, &message, code);
                }

                // A local `export { X as Y }` is the same story one form over:
                // the specifier is a distinct alias *symbol* in tsc and gets its
                // own TS2303, but this binder folds it onto the local symbol it
                // re-exports, so the site is only reachable syntactically. The
                // message names the alias tsc would have created, not `sym`.
                let entry_name = cycle_entry_export_name.clone().unwrap_or_default();
                for (spec_idx, exported_name) in self.local_export_specifier_sites_for_cyclic_alias(
                    decl_idx,
                    sym_id,
                    &entry_name,
                ) {
                    let specifier_message = format_message(
                        diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                        &[&exported_name],
                    );
                    self.error_at_node(spec_idx, &specifier_message, code);
                }
            }
        }
    }

    // =========================================================================
    // CommonJS Circular Alias Detection (TS2303)
    // =========================================================================

    /// Detects circular aliases in CommonJS export property assignments.
    ///
    /// In JS files, `exports.X = exports.Y` (or the `module.exports.X`
    /// spelling of either side) creates an alias from X to Y on the same
    /// module. tsc emits TS2303 only for a *genuine* alias cycle
    /// (X -> ... -> X), at every alias statement on the cycle, each named by
    /// its own alias. Two shapes that look adjacent are NOT circular:
    /// - a chain that merely *ends* at an undefined name
    ///   (`exports.blah = exports.someProp` with no `someProp` anywhere) —
    ///   the failing RHS read surfaces TS2339 through ordinary property
    ///   checking instead;
    /// - a chain that leads *into* a cycle without being on it
    ///   (`exports.x = exports.a` beside an `a <-> b` cycle) — only the
    ///   cycle members report.
    ///
    /// When the module also mixes a bare `module.exports = X` export
    /// assignment with these property exports (the TS2309 surface), tsc
    /// disables CommonJS alias classification wholesale: no TS2303 fires and
    /// the sibling writes/reads surface TS2339 against `X`.
    pub(crate) fn check_commonjs_circular_aliases(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // A bare `module.exports = X` beside sibling property exports is the
        // TS2309 export-assignment mix; the property statements are then not
        // alias declarations at all, so no TS2303 can fire.
        let file_idx = self.ctx.current_file_idx;
        if self
            .resolve_js_export_surface(file_idx)
            .has_commonjs_export_assignment_conflict()
        {
            return;
        }

        // alias_map: property_name -> target_property_name for
        // `exports.X = exports.Y` patterns (functional graph: one outgoing
        // edge per name, last assignment wins).
        let mut alias_map: FxHashMap<String, String> = FxHashMap::default();
        // Every alias statement's LHS site in source order; each site whose
        // name lands on a cycle reports its own TS2303.
        let mut alias_sites: Vec<(String, NodeIndex)> = Vec::new();
        // concrete_props: properties assigned a concrete (non-exports-ref) value
        // e.g., `exports.foo = 42` or `exports.bar = someFunction`
        let mut concrete_props: FxHashSet<String> = FxHashSet::default();

        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            // Check LHS is `exports.X`
            let Some(lhs_prop) = self.get_exports_property_name(bin.left) else {
                continue;
            };

            // Check if RHS is `exports.Y` (alias) or a concrete value
            if let Some(rhs_prop) = self.get_exports_property_name(bin.right) {
                alias_map.insert(lhs_prop.clone(), rhs_prop);
                alias_sites.push((lhs_prop, bin.left));
            } else {
                concrete_props.insert(lhs_prop);
            }
        }

        // Mark the names that sit on a genuine cycle. The alias graph is
        // functional (one outgoing edge per name), so walking from any name
        // either terminates (concrete property or undefined target — both
        // non-circular) or repeats a name; the repeated suffix of the walk is
        // the cycle, and only those names report.
        let mut on_cycle: FxHashSet<String> = FxHashSet::default();
        for start_name in alias_map.keys() {
            if on_cycle.contains(start_name) {
                continue;
            }
            let mut walk_order: Vec<String> = Vec::new();
            let mut walked: FxHashSet<String> = FxHashSet::default();
            let mut current = start_name.clone();
            loop {
                // A concrete property or an already-classified cycle member
                // ends the walk: nothing new on this path is circular (a
                // chain *into* a known cycle is not itself on it).
                if concrete_props.contains(&current) || on_cycle.contains(&current) {
                    break;
                }
                if !walked.insert(current.clone()) {
                    // Repeat found: the walk from `current`'s first
                    // occurrence onward is the cycle.
                    let cycle_start = walk_order
                        .iter()
                        .position(|name| name == &current)
                        .unwrap_or(0);
                    for name in &walk_order[cycle_start..] {
                        on_cycle.insert(name.clone());
                    }
                    break;
                }
                walk_order.push(current.clone());
                match alias_map.get(&current) {
                    Some(next) => current = next.clone(),
                    // Undefined target: the chain dangles, it does not cycle.
                    // The failing RHS read is ordinary property checking's
                    // TS2339, not TS2303.
                    None => break,
                }
            }
        }

        // tsc reports at every alias declaration on the cycle, in source
        // order, each named by its own alias.
        for (name, error_node) in &alias_sites {
            if !on_cycle.contains(name) {
                continue;
            }
            let message = format_message(
                diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                &[name],
            );
            self.error_at_node(
                *error_node,
                &message,
                diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
            );
        }
    }

    /// Helper: if `idx` points to `exports.X` or `module.exports.X`
    /// (property access on the CommonJS export container), return
    /// `Some("X")`. Otherwise `None`.
    fn get_exports_property_name(&self, idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;

        // The object must be the module's export container: either the bare
        // `exports` identifier or the `module.exports` property access.
        if !self.is_commonjs_exports_container(access.expression) {
            return None;
        }

        // Get the property name
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        Some(name_ident.escaped_text.to_string())
    }

    /// Whether `idx` is the CommonJS export container expression: the bare
    /// `exports` identifier or the `module.exports` property access.
    fn is_commonjs_exports_container(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports");
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            let base_is_module = self
                .ctx
                .arena
                .get(access.expression)
                .filter(|base| base.kind == SyntaxKind::Identifier as u16)
                .and_then(|base| self.ctx.arena.get_identifier(base))
                .is_some_and(|ident| ident.escaped_text == "module");
            let member_is_exports = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|name| self.ctx.arena.get_identifier(name))
                .is_some_and(|ident| ident.escaped_text == "exports");
            return base_is_module && member_is_exports;
        }
        false
    }
}
