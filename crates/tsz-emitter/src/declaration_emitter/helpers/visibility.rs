//! Visibility checks, import analysis, and declaration collection

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn statement_has_effective_export(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if self.stmt_has_export_modifier(stmt_node) {
            return true;
        }
        if !self.source_is_js_file {
            return false;
        }

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(stmt_node)
                .is_some_and(|func| self.is_js_named_exported_name(func.name)),
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(stmt_node)
                .is_some_and(|class| self.is_js_named_exported_name(class.name)),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(stmt_node).is_some_and(|var_stmt| {
                    var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                        self.arena
                            .get(decl_list_idx)
                            .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                            .is_some_and(|decl_list| {
                                decl_list.declarations.nodes.iter().any(|&decl_idx| {
                                    self.arena
                                        .get(decl_idx)
                                        .and_then(|decl_node| {
                                            self.arena.get_variable_declaration(decl_node)
                                        })
                                        .is_some_and(|decl| {
                                            self.is_js_named_exported_name(decl.name)
                                        })
                                })
                            })
                    })
                })
            }
            _ => false,
        }
    }

    /// Return true when declarations are filtered to public API members.
    pub(crate) const fn public_api_filter_enabled(&self) -> bool {
        self.emit_public_api_only && self.public_api_scope_depth == 0
    }

    /// Return true if a top-level declaration should be emitted when API filtering is enabled.
    pub(crate) fn should_emit_public_api_member(&self, modifiers: &Option<NodeList>) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        self.arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword)
    }

    /// Return true if a module declaration should be emitted when API filtering is enabled.
    pub(crate) fn should_emit_public_api_module(
        &self,
        is_exported: bool,
        name_idx: NodeIndex,
    ) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        // Module augmentations (`declare module "foo"` and `declare global`)
        // must always be emitted regardless of the public API filter.
        // They augment external module or global scope and are always part
        // of the declaration output.
        if let Some(name_node) = self.arena.get(name_idx) {
            // String-literal module name: `declare module "some-module" { ... }`
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
            // `declare global { ... }` — the parser represents `global` as
            // an Identifier node with escaped_text "global".
            if let Some(ident) = self.arena.get_identifier(name_node)
                && ident.escaped_text == "global"
            {
                return true;
            }
        }

        is_exported || self.should_emit_public_api_dependency(name_idx)
    }

    /// Return true if a declaration should be skipped because it's a
    /// non-exported value/type inside a non-ambient namespace.
    /// Namespace and import-alias declarations are NOT filtered here
    /// (they may be needed for name resolution and are filtered recursively).
    ///
    /// When `decl_idx` is provided, non-exported members that are referenced
    /// by the exported API surface (via `used_symbols`) are preserved —
    /// TSC emits these so that exported declarations can reference them.
    pub(crate) fn should_skip_ns_internal_member(
        &self,
        modifiers: &Option<NodeList>,
        decl_idx: Option<NodeIndex>,
    ) -> bool {
        if !self.inside_non_ambient_namespace {
            return false;
        }
        // If the member has an `export` keyword, keep it
        if self
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword)
        {
            return false;
        }
        // If the member is referenced by the exported API surface, keep it
        if let Some(idx) = decl_idx
            && self.is_ns_member_used_by_exports(idx)
        {
            return false;
        }
        // Non-exported member inside non-ambient namespace: skip
        true
    }

    /// Check if a non-exported namespace member's symbol appears in `used_symbols`.
    /// Unlike `should_emit_public_api_dependency`, this does NOT short-circuit
    /// when `public_api_filter_enabled()` is false — it always checks the usage set.
    pub(crate) fn is_ns_member_used_by_exports(&self, decl_idx: NodeIndex) -> bool {
        let Some(used) = &self.used_symbols else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        // Direct node-to-symbol lookup (works when decl_idx is the declaration node)
        if let Some(&sym_id) = binder.node_symbols.get(&decl_idx.0) {
            if used.contains_key(&sym_id) {
                return true;
            }
        }
        // Fallback: resolve by identifier text via scope tables.
        // For import-equals declarations, extract the name from the import clause
        // since the declaration node itself is not an identifier.
        let Some(name_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let import_clause_idx = if let Some(import_eq) = self.arena.get_import_decl(name_node) {
            // Also check the import clause's node_symbols (the name identifier
            // may have a different SymbolId than the declaration node).
            if let Some(&clause_sym) = binder.node_symbols.get(&import_eq.import_clause.0) {
                if used.contains_key(&clause_sym) {
                    return true;
                }
            }
            Some(import_eq.import_clause)
        } else {
            None
        };
        let name_text = if let Some(ident) = self.arena.get_identifier(name_node) {
            Some(ident.escaped_text.clone())
        } else if let Some(clause_idx) = import_clause_idx {
            self.arena
                .get(clause_idx)
                .and_then(|n| self.arena.get_identifier(n))
                .map(|ident| ident.escaped_text.clone())
        } else {
            None
        };
        let Some(name) = name_text else {
            return false;
        };
        // Check all scope tables (not just file_locals) since the symbol
        // may be in a namespace scope
        for scope in &binder.scopes {
            if let Some(sym_id) = scope.table.get(&name)
                && used.contains_key(&sym_id)
            {
                return true;
            }
        }
        if let Some(sym_id) = binder.file_locals.get(&name) {
            return used.contains_key(&sym_id);
        }
        false
    }

    /// Check if a statement node has the `export` keyword modifier.
    pub(crate) fn stmt_has_export_modifier(
        &self,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let k = stmt_node.kind;
        if k == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.arena.get_function(stmt_node) {
                return self
                    .arena
                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::CLASS_DECLARATION {
            if let Some(class) = self.arena.get_class(stmt_node) {
                return self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(iface) = self.arena.get_interface(stmt_node) {
                return self
                    .arena
                    .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            if let Some(alias) = self.arena.get_type_alias(stmt_node) {
                return self
                    .arena
                    .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::ENUM_DECLARATION {
            if let Some(enum_data) = self.arena.get_enum(stmt_node) {
                return self
                    .arena
                    .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
                return self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module) = self.arena.get_module(stmt_node)
        {
            return self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword);
        }
        false
    }

    /// Check whether the leading comments before `pos` contain `@internal`.
    /// Used when `--stripInternal` is enabled to elide internal declarations.
    pub(crate) fn has_internal_annotation(&self, pos: u32) -> bool {
        if !self.strip_internal {
            return false;
        }
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        // Search backwards from `pos` through any comments that precede this node.
        // The `@internal` annotation can appear in `/** @internal */` or `// @internal`.
        for comment in self.all_comments.iter().rev() {
            if comment.end > pos {
                continue;
            }
            // Only consider comments immediately before this position
            // (allow only whitespace between comment end and pos)
            let between = &text[comment.end as usize..pos as usize];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }
            let ct = &text[comment.pos as usize..comment.end as usize];
            if ct.contains("@internal") {
                return true;
            }
            // If this comment doesn't have @internal, don't look further back
            break;
        }
        false
    }

    /// Return true when a declaration symbol is referenced by the exported API surface.
    pub(crate) fn should_emit_public_api_dependency(&self, name_idx: NodeIndex) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        let Some(used) = &self.used_symbols else {
            // Usage analysis unavailable: preserve dependent declarations
            // rather than over-pruning and producing unresolved names.
            return true;
        };
        let Some(binder) = self.binder else {
            return true;
        };
        let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) else {
            // Some declaration name nodes are not mapped directly; fall back
            // to root-scope lookup by identifier text.
            let Some(name_node) = self.arena.get(name_idx) else {
                return false;
            };
            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                return false;
            };
            // Check file_locals first (matches UsageAnalyzer's lookup path)
            if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text) {
                return used.contains_key(&sym_id);
            }
            // Fall back to root scope table
            let Some(root_scope) = binder.scopes.first() else {
                return false;
            };
            let Some(scope_sym_id) = root_scope.table.get(&name_ident.escaped_text) else {
                return false;
            };
            return used.contains_key(&scope_sym_id);
        };

        used.contains_key(&sym_id)
    }

    /// Check if the target of a namespace-path import-equals resolves to a type-level entity.
    ///
    /// For `import x = a.b.c`, resolves the rightmost identifier (`c`) to a symbol and checks
    /// if it has type-level flags (class, interface, enum, namespace, type alias, function).
    /// When the target is a type/namespace entity, the emitted .d.ts type annotations may
    /// reference the alias name, so the import must be preserved. When the target is a plain
    /// variable, the type resolves to a primitive/literal and the alias is not needed.
    pub(crate) fn import_alias_targets_type_entity(&self, module_spec_idx: NodeIndex) -> bool {
        let Some(binder) = self.binder else {
            return true; // conservative: preserve when we can't resolve
        };

        // Try node_symbols on the full module specifier and rightmost name.
        // The binder may map these nodes to the resolved target symbol.
        let rightmost_idx = self.get_rightmost_name(module_spec_idx);
        for &idx in &[module_spec_idx, rightmost_idx] {
            if let Some(&sym_id) = binder.node_symbols.get(&idx.0)
                && let Some(symbol) = binder.symbols.get(sym_id)
            {
                return !self.symbol_is_value_only(symbol);
            }
        }

        // Fall back to name-based lookup on the rightmost identifier.
        let Some(name_node) = self.arena.get(rightmost_idx) else {
            return true; // conservative
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return true; // conservative
        };
        let name = &name_ident.escaped_text;

        // Search all resolution paths for a non-ALIAS symbol.
        // The import's own ALIAS symbol shares the same name, so skip it
        // to find the actual target entity.
        for scope in &binder.scopes {
            if let Some(sym_id) = scope.table.get(name)
                && let Some(symbol) = binder.symbols.get(sym_id)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            {
                return !self.symbol_is_value_only(symbol);
            }
        }
        if let Some(sym_id) = binder.file_locals.get(name)
            && let Some(symbol) = binder.symbols.get(sym_id)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
        {
            return !self.symbol_is_value_only(symbol);
        }

        // All symbols found were aliases — can't determine target.
        // Preserve conservatively.
        true
    }

    /// Get the rightmost identifier from a qualified name or property access.
    pub(in crate::declaration_emitter) fn get_rightmost_name(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.arena.get(idx) else {
            return idx;
        };
        if let Some(qn) = self.arena.get_qualified_name(node) {
            return self.get_rightmost_name(qn.right);
        }
        if let Some(access) = self.arena.get_access_expr(node) {
            return self.get_rightmost_name(access.name_or_argument);
        }
        idx
    }

    /// Check if a symbol is value-only (plain variable, no type/namespace/class flags).
    /// Value-only entities resolve to primitive types in .d.ts and don't need aliases.
    const fn symbol_is_value_only(&self, symbol: &tsz_binder::Symbol) -> bool {
        const VALUE_ONLY_FLAGS: u32 = tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
            | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
            | tsz_binder::symbol_flags::PROPERTY;
        const NON_VALUE_ONLY_FLAGS: u32 = tsz_binder::symbol_flags::CLASS
            | tsz_binder::symbol_flags::INTERFACE
            | tsz_binder::symbol_flags::ENUM
            | tsz_binder::symbol_flags::TYPE_ALIAS
            | tsz_binder::symbol_flags::VALUE_MODULE
            | tsz_binder::symbol_flags::NAMESPACE_MODULE
            | tsz_binder::symbol_flags::FUNCTION
            | tsz_binder::symbol_flags::METHOD
            | tsz_binder::symbol_flags::ENUM_MEMBER;
        let flags = symbol.flags;
        (flags & VALUE_ONLY_FLAGS) != 0 && (flags & NON_VALUE_ONLY_FLAGS) == 0
    }

    /// Get the function/method name as a string for overload tracking
    pub(crate) fn get_function_name(&self, func_idx: NodeIndex) -> Option<String> {
        let func_node = self.arena.get(func_idx)?;

        // Try to get as function first
        let name_node = if let Some(func) = self.arena.get_function(func_node) {
            self.arena.get(func.name)?
        // Try to get as method
        } else if let Some(method) = self.arena.get_method_decl(func_node) {
            self.arena.get(method.name)?
        } else {
            return None;
        };

        // Extract identifier names directly
        if name_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        } else {
            // For computed property names and other non-identifier names,
            // use the source text span as a key for overload tracking
            self.get_source_slice(name_node.pos, name_node.end)
        }
    }

    /// Check if an import specifier should be emitted based on usage analysis.
    ///
    /// Returns true if:
    /// - No usage tracking is enabled (`used_symbols` is None)
    /// - The specifier's symbol is in the `used_symbols` set
    pub(crate) fn should_emit_import_specifier(&self, specifier_idx: NodeIndex) -> bool {
        // If no usage tracking, emit everything
        let Some(used) = &self.used_symbols else {
            return true;
        };

        // If no binder, we can't check symbols - emit conservatively
        let Some(binder) = &self.binder else {
            return true;
        };

        // Get the specifier node to extract its name
        let Some(spec_node) = self.arena.get(specifier_idx) else {
            return true;
        };

        // Only ImportSpecifier/ExportSpecifier nodes have symbols (on their name field)
        // For other node types, emit conservatively
        if spec_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
            && spec_node.kind != tsz_parser::parser::syntax_kind_ext::EXPORT_SPECIFIER
        {
            return true;
        }

        let Some(specifier) = self.arena.get_specifier(spec_node) else {
            return true;
        };

        self.imported_name_is_used(binder, used, specifier.name)
    }

    pub(in crate::declaration_emitter) fn imported_name_is_used(
        &self,
        binder: &BinderState,
        used: &rustc_hash::FxHashMap<SymbolId, super::super::usage_analyzer::UsageKind>,
        name_idx: NodeIndex,
    ) -> bool {
        let sym_id = if let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) {
            sym_id
        } else {
            let Some(name_node) = self.arena.get(name_idx) else {
                return true;
            };
            let Some(ident) = self.arena.get_identifier(name_node) else {
                return true;
            };
            let Some(sym_id) = binder.file_locals.get(&ident.escaped_text) else {
                return true;
            };
            sym_id
        };

        self.import_symbol_is_used(binder, used, sym_id)
    }

    pub(in crate::declaration_emitter) fn import_symbol_is_used(
        &self,
        binder: &BinderState,
        used: &rustc_hash::FxHashMap<SymbolId, super::super::usage_analyzer::UsageKind>,
        sym_id: SymbolId,
    ) -> bool {
        if used.contains_key(&sym_id) {
            return true;
        }

        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(import_module) = symbol.import_module.as_deref() else {
            return false;
        };
        let local_name = symbol.escaped_name.as_str();
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(&symbol.escaped_name);

        if local_name != import_name {
            return false;
        }

        used.iter().any(|(&used_sym_id, _)| {
            let Some(used_symbol) = binder.symbols.get(used_sym_id) else {
                return false;
            };

            if used_symbol.escaped_name != import_name
                && used_symbol.import_name.as_deref() != Some(import_name)
            {
                return false;
            }

            used_symbol.import_module.as_deref() == Some(import_module)
                || self.resolve_symbol_module_path(used_sym_id).as_deref() == Some(import_module)
        })
    }

    pub(crate) fn can_reference_local_import_alias_by_name(&self, sym_id: SymbolId) -> bool {
        let (Some(binder), Some(used)) = (self.binder, self.used_symbols.as_ref()) else {
            return true;
        };

        self.import_symbol_is_used(binder, used, sym_id)
    }

    /// Count how many import specifiers in an `ImportClause` should be emitted.
    ///
    /// Returns (`default_count`, `named_count`) where:
    /// - `default_count`: 1 if default import is used, 0 otherwise
    /// - `named_count`: number of used named import specifiers
    pub(crate) fn count_used_imports(
        &self,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> (usize, usize) {
        let mut default_count = 0;
        let mut named_count = 0;

        if let Some(used) = &self.used_symbols
            && let Some(binder) = &self.binder
        {
            // Check default import
            if import.import_clause.is_some()
                && let Some(clause_node) = self.arena.get(import.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                if clause.name.is_some() && self.imported_name_is_used(binder, used, clause.name) {
                    default_count = 1;
                }

                // Count named imports
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                    && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                {
                    if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                        if self.imported_name_is_used(binder, used, bindings.name) {
                            named_count = 1;
                        }
                    } else {
                        for &spec_idx in &bindings.elements.nodes {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(specifier) = self.arena.get_specifier(spec_node)
                                && self.imported_name_is_used(binder, used, specifier.name)
                            {
                                named_count += 1;
                            }
                        }
                    }
                }
            }
        } else {
            // No usage tracking available (e.g., --noCheck --noLib mode).
            // In this mode, tsc would have type info to decide which imports are needed,
            // but we don't. Apply conservative heuristics:
            // - Type-only imports: keep (likely needed for type references)
            // - Named imports with specifiers: keep (may reference types)
            // - Namespace imports (import * as ns): skip (almost always value-level)
            // - Empty imports (import {}): skip
            if import.import_clause.is_some()
                && let Some(clause_node) = self.arena.get(import.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                // Type-only imports are likely needed for type references
                let is_type_only = clause.is_type_only;

                // Default import - keep for type-only, skip otherwise without tracking
                default_count = if is_type_only {
                    usize::from(clause.name.is_some())
                } else {
                    0
                };

                // Named bindings: check if there are actually any specifiers
                if clause.named_bindings.is_some() {
                    if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                        && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                    {
                        if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                            // Namespace import (import * as ns): skip in fallback mode
                            // These are almost exclusively for value-level code (ns.method())
                            // and rarely needed in .d.ts output
                            named_count = 0;
                        } else if is_type_only {
                            // Type-only named imports - keep all
                            named_count = bindings.elements.nodes.len();
                        } else {
                            // Regular named imports - keep (may be type references)
                            named_count = bindings.elements.nodes.len();
                        }
                    } else {
                        named_count = if is_type_only { 1 } else { 0 };
                    }
                }
            } else {
                // No import clause - side-effect import handled elsewhere
                default_count = 0;
                named_count = 0;
            }
        }

        (default_count, named_count)
    }

    /// Phase 4: Prepare import aliases before emitting anything.
    ///
    /// This detects name collisions and generates aliases for conflicting imports.
    pub(crate) fn prepare_import_aliases(&mut self, root_idx: NodeIndex) {
        // 1. Collect all top-level local declarations into reserved_names
        self.collect_local_declarations(root_idx);

        // 2. Process required_imports (String-based)
        // We clone keys to avoid borrow checker issues during iteration
        let modules: Vec<String> = self.required_imports.keys().cloned().collect();
        for module in modules {
            // Collect names into a separate vector to release the borrow
            let names: Vec<String> = self
                .required_imports
                .get(&module)
                .map(|v| v.to_vec())
                .unwrap_or_default();
            for name in names {
                self.resolve_import_name(&module, &name);
            }
        }

        // 3. Process foreign_symbols (SymbolId-based) - skip for now
        // This requires grouping by module which needs arena_to_path mapping
    }

    /// Collect local top-level names into `reserved_names`.
    pub(crate) fn collect_local_declarations(&mut self, root_idx: NodeIndex) {
        let Some(root_node) = self.arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return;
        };

        // If we have a binder, use it to get top-level symbols
        if let Some(binder) = self.binder {
            // Get the root scope (scopes is a Vec, not a HashMap)
            if let Some(root_scope) = binder.scopes.first() {
                // Iterate through all symbols in root scope table
                for (name, _sym_id) in root_scope.table.iter() {
                    self.reserved_names.insert(name.clone());
                }
            }
        } else {
            // Fallback: Walk AST statements for top-level declarations
            for &stmt_idx in &source_file.statements.nodes {
                if stmt_idx.is_none() {
                    continue;
                }
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };

                let kind = stmt_node.kind;
                // Collect names from various declaration types
                if kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::ENUM_DECLARATION
                {
                    // Try to get the name
                    if let Some(name) = self.extract_declaration_name(stmt_idx) {
                        self.reserved_names.insert(name);
                    }
                }
            }
        }
    }

    /// Extract the name from a declaration node.
    pub(crate) fn extract_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;

        // Try identifier first
        if let Some(ident) = self.arena.get_identifier(decl_node) {
            return Some(ident.escaped_text.clone());
        }

        // For class/function/interface, the name is in a specific field
        if let Some(func) = self.arena.get_function(decl_node)
            && let Some(name_node) = self.arena.get(func.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(class) = self.arena.get_class(decl_node)
            && let Some(name_node) = self.arena.get(class.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(iface) = self.arena.get_interface(decl_node)
            && let Some(name_node) = self.arena.get(iface.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(alias) = self.arena.get_type_alias(decl_node)
            && let Some(name_node) = self.arena.get(alias.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(enum_data) = self.arena.get_enum(decl_node)
            && let Some(name_node) = self.arena.get(enum_data.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }

        None
    }

    /// Resolve name for string imports, generating alias if needed.
    pub(crate) fn resolve_import_name(&mut self, module: &str, name: &str) {
        if self.reserved_names.contains(name) {
            // Collision! Generate alias
            let alias = self.generate_unique_name(name);
            self.import_string_aliases
                .insert((module.to_string(), name.to_string()), alias.clone());
            self.reserved_names.insert(alias);
        } else {
            // No collision, reserve the name
            self.reserved_names.insert(name.to_string());
        }
    }

    /// Generate unique name (e.g., "`TypeA_1`").
    pub(crate) fn generate_unique_name(&self, base: &str) -> String {
        let mut i = 1;
        loop {
            let candidate = format!("{base}_{i}");
            if !self.reserved_names.contains(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    pub(crate) fn reset_writer(&mut self) {
        self.writer = SourceWriter::with_capacity(4096);
        self.pending_source_pos = None;
        self.public_api_scope_depth = 0;
        if let Some(state) = &self.source_map_state {
            self.writer.enable_source_map(state.output_name.clone());
            let content = self.source_map_text.map(std::string::ToString::to_string);
            self.writer.add_source(state.source_name.clone(), content);
        }
    }

}
