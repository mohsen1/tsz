//! Visibility checks, import analysis, and declaration collection

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use crate::transforms::emit_utils::string_literal_text;
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
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena, NodeView};
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

        self.has_export_modifier(modifiers)
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
            if name_node.is_string_literal() {
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
        if self.has_export_modifier(modifiers) {
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
        // Direct node-to-symbol lookup
        if let Some(&sym_id) = binder.node_symbols.get(&decl_idx.0)
            && used.contains_key(&sym_id)
        {
            return true;
        }
        // The binder maps declaration NAME nodes (not the declaration
        // node itself) into node_symbols.  For class / function /
        // interface / type-alias / enum, extract the name and retry.
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        if let Some(name_ni) = self.get_declaration_name_idx(decl_node)
            && let Some(&sym_id) = binder.node_symbols.get(&name_ni.0)
            && used.contains_key(&sym_id)
        {
            return true;
        }
        // For import-equals declarations extract the name from the
        // import clause since the declaration node is not an identifier.
        let import_clause_idx = if let Some(import_eq) = self.arena.get_import_decl(decl_node) {
            if let Some(&clause_sym) = binder.node_symbols.get(&import_eq.import_clause.0)
                && used.contains_key(&clause_sym)
            {
                return true;
            }
            Some(import_eq.import_clause)
        } else {
            None
        };
        // Resolve identifier text for scope-table fallback.
        let name_text = if let Some(ni) = self.get_declaration_name_idx(decl_node) {
            self.arena
                .get(ni)
                .and_then(|n| self.arena.get_identifier(n))
                .map(|ident| ident.escaped_text.clone())
        } else if let Some(ident) = self.arena.get_identifier(decl_node) {
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
        for scope in binder.scopes.iter() {
            if let Some(sym_id) = scope.table.get(&name)
                && used.contains_key(&sym_id)
            {
                return true;
            }
        }
        if let Some(sym_id) = binder.file_locals.get(&name) {
            return used.contains_key(&sym_id);
        }
        self.namespace_member_referenced_by_exported_object_literal(decl_idx, &name)
    }

    pub(crate) fn namespace_member_referenced_by_exported_object_literal(
        &self,
        _decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        self.arena.nodes.iter().any(|node| {
            let Some(module_block) = self.arena.get_module_block(node) else {
                return false;
            };
            let Some(statements) = module_block.statements.as_ref() else {
                return false;
            };
            statements.nodes.iter().copied().any(|stmt_idx| {
                self.exported_variable_statement_initializer_references_name(stmt_idx, name)
            })
        }) || self.object_literal_member_references_name(name)
    }

    fn object_literal_member_references_name(&self, name: &str) -> bool {
        self.arena.nodes.iter().any(|node| {
            let Some(object) = self.arena.get_literal_expr(node) else {
                return false;
            };
            object.elements.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
                    return self.get_identifier_text(shorthand.name).as_deref() == Some(name);
                }
                if let Some(property) = self.arena.get_property_assignment(member_node) {
                    return self.get_identifier_text(property.initializer).as_deref() == Some(name);
                }
                false
            })
        })
    }

    fn exported_variable_statement_initializer_references_name(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return false;
        };
        let has_export_modifier = self.stmt_has_export_modifier(stmt_node)
            || self
                .get_source_slice(stmt_node.pos, stmt_node.end)
                .is_some_and(|text| text.trim_start().starts_with("export "));
        if !has_export_modifier {
            return false;
        }
        var_stmt
            .declarations
            .nodes
            .iter()
            .copied()
            .any(|decl_list_idx| {
                self.arena
                    .get(decl_list_idx)
                    .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                    .is_some_and(|decl_list| {
                        decl_list
                            .declarations
                            .nodes
                            .iter()
                            .copied()
                            .any(|var_decl_idx| {
                                self.exported_variable_initializer_references_name(
                                    var_decl_idx,
                                    name,
                                )
                            })
                    })
            })
    }

    fn exported_variable_initializer_references_name(
        &self,
        var_decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(var_decl_node) = self.arena.get(var_decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(var_decl_node) else {
            return false;
        };
        if !var_decl.initializer.is_some() {
            return false;
        }
        let Some(init_node) = self.arena.get(var_decl.initializer) else {
            return false;
        };
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        object.elements.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
                return self.get_identifier_text(shorthand.name).as_deref() == Some(name);
            }
            if let Some(property) = self.arena.get_property_assignment(member_node) {
                return self.get_identifier_text(property.initializer).as_deref() == Some(name);
            }
            false
        })
    }

    /// Extract the name `NodeIndex` from a declaration node.
    fn get_declaration_name_idx(&self, node: &Node) -> Option<NodeIndex> {
        let k = node.kind;
        if k == syntax_kind_ext::CLASS_DECLARATION {
            self.arena
                .get_class(node)
                .and_then(|c| if c.name.is_some() { Some(c.name) } else { None })
        } else if k == syntax_kind_ext::FUNCTION_DECLARATION {
            self.arena
                .get_function(node)
                .and_then(|f| if f.name.is_some() { Some(f.name) } else { None })
        } else if k == syntax_kind_ext::INTERFACE_DECLARATION {
            self.arena.get_interface(node).map(|i| i.name)
        } else if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            self.arena.get_type_alias(node).map(|t| t.name)
        } else if k == syntax_kind_ext::ENUM_DECLARATION {
            self.arena.get_enum(node).map(|e| e.name)
        } else {
            None
        }
    }

    /// Strict variant of `should_emit_public_api_dependency` that returns
    /// `false` when usage analysis is unavailable, preventing non-exported
    /// class declarations from leaking into .d.ts output.
    pub(crate) fn is_confirmed_public_api_dependency(&self, name_idx: NodeIndex) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }
        let Some(used) = &self.used_symbols else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let direct_sym = binder.node_symbols.get(&name_idx.0).copied();
        if let Some(sym_id) = direct_sym
            && used.contains_key(&sym_id)
        {
            return true;
        }
        // The declaration's name may resolve to a different SymbolId than the
        // reference site that marked the symbol used (e.g., heritage clauses
        // inside a namespace can resolve through scope tables that don't share
        // the binder's `node_symbols` mapping). Fall back to name-based lookup
        // across all scopes so a same-named symbol marked-used elsewhere keeps
        // the declaration alive. Matches privacyClassExtendsClauseDeclFile.
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return false;
        };
        if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text)
            && used.contains_key(&sym_id)
        {
            return true;
        }
        for scope in binder.scopes.iter() {
            if let Some(sym_id) = scope.table.get(&name_ident.escaped_text)
                && used.contains_key(&sym_id)
            {
                return true;
            }
        }
        false
    }

    /// Check if a statement node has the `export` keyword modifier.
    pub(crate) fn stmt_has_export_modifier(&self, stmt_node: &Node) -> bool {
        self.node_has_export_modifier(stmt_node)
    }

    fn node_has_export_modifier(&self, node: &Node) -> bool {
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(node)
                .is_some_and(|func| self.has_export_modifier(&func.modifiers)),
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(node)
                .is_some_and(|class| self.has_export_modifier(&class.modifiers)),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                .arena
                .get_interface(node)
                .is_some_and(|iface| self.has_export_modifier(&iface.modifiers)),
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                .arena
                .get_type_alias(node)
                .is_some_and(|alias| self.has_export_modifier(&alias.modifiers)),
            k if k == syntax_kind_ext::ENUM_DECLARATION => self
                .arena
                .get_enum(node)
                .is_some_and(|enum_data| self.has_export_modifier(&enum_data.modifiers)),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .arena
                .get_variable(node)
                .is_some_and(|var_stmt| self.has_export_modifier(&var_stmt.modifiers)),
            k if k == syntax_kind_ext::MODULE_DECLARATION => self
                .arena
                .get_module(node)
                .is_some_and(|module| self.has_export_modifier(&module.modifiers)),
            _ => false,
        }
    }

    fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword)
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

        if self.is_js_export_equals_name(name_idx) {
            return true;
        }

        if self.source_is_js_file
            && let Some(name) = self.get_identifier_text(name_idx)
            && self
                .js_namespace_export_aliases
                .values()
                .any(|aliases| aliases.iter().any(|alias| alias.local_name == name))
        {
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

        // For destructuring patterns (`const { foo } = ...`), recurse into
        // the binding identifiers and return true if any one is referenced
        // by the public API. Without this, `export type T = typeof foo`
        // referencing a destructured const elides the const declaration.
        // Matches declarationEmitNonExportedBindingPattern.
        if let Some(name_node) = self.arena.get(name_idx)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            return self.binding_pattern_has_used_identifier(name_idx);
        }

        if let Some(&sym_id) = binder.node_symbols.get(&name_idx.0)
            && used.contains_key(&sym_id)
        {
            return true;
        }

        // Some declaration name nodes are mapped to a different SymbolId than
        // the export-specifier reference that marked the local binding as used.
        // Fall back to name lookup even when a direct node symbol exists so
        // ambiguous type-modifier export clauses keep their referenced locals.
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return false;
        };
        // Check file_locals first (matches UsageAnalyzer's lookup path)
        if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text)
            && used.contains_key(&sym_id)
        {
            return true;
        }
        // Fall back to scope tables. Namespace members can be declared outside
        // file_locals, and UsageAnalyzer marks those symbols from exported
        // namespace member types.
        binder.scopes.iter().any(|scope| {
            scope
                .table
                .get(&name_ident.escaped_text)
                .is_some_and(|scope_sym_id| used.contains_key(&scope_sym_id))
        })
    }

    pub(crate) fn declared_ambient_value_dependency_is_initializer_only(
        &self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        type_annotation: NodeIndex,
    ) -> bool {
        if !self.public_api_filter_enabled() || initializer.is_some() || type_annotation.is_none() {
            return false;
        }
        let Some(_usage_kind) = self.public_api_dependency_usage_kind(name_idx) else {
            return false;
        };
        let Some(name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        !self.public_api_type_surface_contains_typeof_name(&name)
            && !self.public_api_export_specifier_exports_name(&name)
    }

    fn public_api_dependency_usage_kind(
        &self,
        name_idx: NodeIndex,
    ) -> Option<super::super::usage_analyzer::UsageKind> {
        if !self.public_api_filter_enabled() {
            return Some(
                super::super::usage_analyzer::UsageKind::TYPE
                    | super::super::usage_analyzer::UsageKind::VALUE,
            );
        }
        let used = self.used_symbols.as_ref()?;
        let binder = self.binder?;
        let name_node = self.arena.get(name_idx)?;
        let name_ident = self.arena.get_identifier(name_node)?;

        if let Some(sym_id) = binder.node_symbols.get(&name_idx.0)
            && let Some(kind) = used.get(sym_id)
        {
            return Some(*kind);
        }
        if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text)
            && let Some(kind) = used.get(&sym_id)
        {
            return Some(*kind);
        }
        for scope in binder.scopes.iter() {
            if let Some(sym_id) = scope.table.get(&name_ident.escaped_text)
                && let Some(kind) = used.get(&sym_id)
            {
                return Some(*kind);
            }
        }
        None
    }

    fn public_api_type_surface_contains_typeof_name(&self, name: &str) -> bool {
        let Some(source_file) = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))
        else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                let is_public_surface = self.stmt_has_export_modifier(stmt_node)
                    || stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    || stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT;
                is_public_surface
                    && (self.node_subtree_contains_type_query_name(stmt_idx, name)
                        || self.node_subtree_contains_computed_property_name(stmt_idx, name))
            })
    }

    /// Walk a node subtree looking for a `[<expr>]` computed property name
    /// where `<expr>` (after stripping parens / `as` / non-null) is the
    /// identifier we are tracking. tsc keeps the supporting
    /// `declare const a: symbol;` declaration alive whenever an exported
    /// class member is declared with `[a]() {…}` because the d.ts
    /// emission renders the same identifier in member position.
    fn node_subtree_contains_computed_property_name(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(node)
        {
            let expr_idx = self.skip_parenthesized_non_null_and_comma(computed.expression);
            if self.entity_name_contains_identifier(expr_idx, name) {
                return true;
            }
        }
        self.arena
            .get_children(node_idx)
            .iter()
            .copied()
            .any(|child_idx| self.node_subtree_contains_computed_property_name(child_idx, name))
    }

    fn node_subtree_contains_type_query_name(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(type_query) = self.arena.get_type_query(node)
            && self.entity_name_contains_identifier(type_query.expr_name, name)
        {
            return true;
        }
        self.arena
            .get_children(node_idx)
            .iter()
            .copied()
            .any(|child_idx| self.node_subtree_contains_type_query_name(child_idx, name))
    }

    fn entity_name_contains_identifier(&self, name_idx: NodeIndex, name: &str) -> bool {
        if self.get_identifier_text(name_idx).as_deref() == Some(name) {
            return true;
        }
        self.arena
            .get_children(name_idx)
            .iter()
            .copied()
            .any(|child_idx| self.entity_name_contains_identifier(child_idx, name))
    }

    fn public_api_export_specifier_exports_name(&self, name: &str) -> bool {
        let Some(source_file) = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))
        else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                match stmt_node.kind {
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        // `export default <expr>` is also represented as an
                        // EXPORT_DECLARATION (with `is_default_export: true`
                        // and the expression placed in `export_clause`).
                        // When the default-exported expression is the
                        // identifier `name`, that's the value-side export
                        // we need to keep the local declaration alive for.
                        if let Some(export) = self.arena.get_export_decl(stmt_node)
                            && export.is_default_export
                            && export.export_clause.is_some()
                            && self.entity_name_contains_identifier(export.export_clause, name)
                        {
                            return true;
                        }
                        self.node_subtree_contains_export_specifier_name(stmt_idx, name)
                    }
                    // `export = X` (commonjs) exports the value-side of `X`
                    // as the entire module. Without this branch a non-exported
                    // declaration whose only public-API consumer is an
                    // `export = X` assignment would be filtered out by the
                    // initializer-only-dependency check.
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => self
                        .arena
                        .get_export_assignment(stmt_node)
                        .is_some_and(|assignment| {
                            self.entity_name_contains_identifier(assignment.expression, name)
                        }),
                    _ => false,
                }
            })
    }

    fn node_subtree_contains_export_specifier_name(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::EXPORT_SPECIFIER
            && let Some(specifier) = self.arena.get_specifier(node)
            && (self.get_identifier_text(specifier.name).as_deref() == Some(name)
                || self.get_identifier_text(specifier.property_name).as_deref() == Some(name))
        {
            return true;
        }
        self.arena
            .get_children(node_idx)
            .iter()
            .copied()
            .any(|child_idx| self.node_subtree_contains_export_specifier_name(child_idx, name))
    }

    pub(crate) fn public_api_dependency_is_type_only_exported_type_side(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return false;
        };

        if self.current_file_has_exported_type_side_named(&name_ident.escaped_text) {
            return true;
        }

        let Some(binder) = self.binder else {
            return false;
        };
        let mut symbol_ids = Vec::new();
        if let Some(sym_id) = binder.node_symbols.get(&name_idx.0).copied() {
            symbol_ids.push(sym_id);
        }
        if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text) {
            symbol_ids.push(sym_id);
        }
        symbol_ids.extend(
            binder
                .scopes
                .iter()
                .filter_map(|scope| scope.table.get(&name_ident.escaped_text)),
        );

        symbol_ids
            .iter()
            .copied()
            .any(|sym_id| self.symbol_has_exported_type_side(sym_id, binder))
    }

    fn symbol_has_exported_type_side(&self, sym_id: SymbolId, binder: &BinderState) -> bool {
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        if symbol.flags & (symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS) == 0 {
            return false;
        }

        symbol
            .declarations
            .iter()
            .copied()
            .any(|decl_idx| self.declaration_is_exported_type_side(decl_idx))
    }

    fn current_file_has_exported_type_side_named(&self, name: &str) -> bool {
        let Some(source_idx) = self.current_source_file_idx else {
            return false;
        };
        let Some(source_node) = self.arena.get(source_idx) else {
            return false;
        };
        let Some(source_file) = self.arena.get_source_file(source_node) else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                if let Some(export) = self.arena.get_export_decl(stmt_node) {
                    return export.module_specifier.is_none()
                        && self.declaration_is_type_side_named(export.export_clause, Some(name));
                }
                match stmt_node.kind {
                    k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        let Some(iface) = self.arena.get_interface(stmt_node) else {
                            return false;
                        };
                        self.node_has_export_modifier(stmt_node)
                            && self.get_identifier_text(iface.name).as_deref() == Some(name)
                    }
                    k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                        let Some(alias) = self.arena.get_type_alias(stmt_node) else {
                            return false;
                        };
                        self.node_has_export_modifier(stmt_node)
                            && self.get_identifier_text(alias.name).as_deref() == Some(name)
                    }
                    _ => false,
                }
            })
    }

    /// Walk a binding pattern's leaf identifiers and return true if any one
    /// is in `used_symbols`.
    fn binding_pattern_has_used_identifier(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.should_emit_public_api_dependency(name_idx)
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                let Some(pattern) = self.arena.get_binding_pattern(name_node) else {
                    return false;
                };
                pattern.elements.nodes.iter().copied().any(|elem_idx| {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        return false;
                    };
                    if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        return false;
                    }
                    if elem_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                        return false;
                    }
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        return false;
                    };
                    self.binding_pattern_has_used_identifier(elem.name)
                })
            }
            _ => false,
        }
    }

    fn declaration_is_exported_type_side(&self, decl_idx: NodeIndex) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        if let Some(export) = self.arena.get_export_decl(decl_node) {
            return export.module_specifier.is_none()
                && self.declaration_is_type_side_named(export.export_clause, None);
        }
        // Direct interface/type-alias declaration: require the `export`
        // modifier so a non-exported `type fn = …` does not falsely mark a
        // value-side const named `fn` as "type-only re-exported".
        let has_export = match decl_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
            {
                self.node_has_export_modifier(decl_node)
            }
            _ => false,
        };
        has_export && self.declaration_is_type_side_named(decl_idx, None)
    }

    fn declaration_is_type_side_named(&self, decl_idx: NodeIndex, name: Option<&str>) -> bool {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        match decl_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(decl_node).is_some_and(|iface| {
                    name.is_none_or(|name| {
                        self.get_identifier_text(iface.name).as_deref() == Some(name)
                    })
                })
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(decl_node).is_some_and(|alias| {
                    name.is_none_or(|name| {
                        self.get_identifier_text(alias.name).as_deref() == Some(name)
                    })
                })
            }
            _ => false,
        }
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
        for scope in binder.scopes.iter() {
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
        } else {
            // Try to get as method
            let method = self.arena.get_method_decl(func_node)?;
            self.arena.get(method.name)?
        };

        // Extract identifier names directly
        if name_node.is_identifier() {
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

        if self.import_specifier_has_emitted_canonical_name(binder, used, specifier) {
            return false;
        }

        self.imported_name_is_used(binder, used, specifier.name)
    }

    fn import_specifier_has_emitted_canonical_name(
        &self,
        binder: &BinderState,
        used: &rustc_hash::FxHashMap<SymbolId, super::super::usage_analyzer::UsageKind>,
        specifier: &tsz_parser::parser::node::SpecifierData,
    ) -> bool {
        let Some(import_name) = self.canonical_named_import_name_for_alias(specifier.name) else {
            return false;
        };
        let Some(&canonical_sym_id) = self.import_name_map.get(import_name) else {
            return false;
        };

        self.import_symbol_is_used(binder, used, canonical_sym_id)
    }

    pub(crate) fn canonical_named_import_name_for_alias(
        &self,
        name_idx: NodeIndex,
    ) -> Option<&str> {
        let binder = self.binder?;
        let name_node = self.arena.get(name_idx)?;
        let ident = self.arena.get_identifier(name_node)?;
        let local_name = ident.escaped_text.as_str();
        let sym_id = binder
            .node_symbols
            .get(&name_idx.0)
            .copied()
            .or_else(|| binder.file_locals.get(local_name))?;
        let symbol = binder.symbols.get(sym_id)?;
        let import_module = symbol.import_module.as_deref()?;
        let import_name = symbol.import_name.as_deref()?;

        if import_name == "*"
            || import_name == local_name
            || self.import_name_map.get(import_name).copied() == Some(sym_id)
        {
            return None;
        }

        if !self.import_alias_has_sibling_canonical_specifier(sym_id, import_name) {
            return None;
        }

        let canonical_sym_id = self.import_name_map.get(import_name).copied()?;
        let canonical_symbol = binder.symbols.get(canonical_sym_id)?;
        if canonical_symbol.escaped_name != import_name
            || canonical_symbol.import_module.as_deref() != Some(import_module)
        {
            return None;
        }

        Some(import_name)
    }

    fn import_alias_has_sibling_canonical_specifier(
        &self,
        sym_id: SymbolId,
        import_name: &str,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(alias_specifier_idx) = self.enclosing_import_specifier(decl_idx) else {
                return false;
            };
            let Some(named_imports_idx) = self.arena.parent_of(alias_specifier_idx) else {
                return false;
            };
            let Some(named_imports_node) = self.arena.get(named_imports_idx) else {
                return false;
            };
            let Some(named_imports) = self.arena.get_named_imports(named_imports_node) else {
                return false;
            };

            named_imports
                .elements
                .nodes
                .iter()
                .copied()
                .any(|spec_idx| {
                    if spec_idx == alias_specifier_idx {
                        return false;
                    }
                    let Some(spec_node) = self.arena.get(spec_idx) else {
                        return false;
                    };
                    let Some(specifier) = self.arena.get_specifier(spec_node) else {
                        return false;
                    };
                    if specifier.property_name.is_some() {
                        return false;
                    }
                    self.get_identifier_text(specifier.name).as_deref() == Some(import_name)
                })
        })
    }

    fn enclosing_import_specifier(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        loop {
            let node = self.arena.get(idx)?;
            if node.kind == syntax_kind_ext::IMPORT_SPECIFIER {
                return Some(idx);
            }
            idx = self.arena.parent_of(idx)?;
        }
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

            if self.symbol_declared_in_ambient_module(used_sym_id, import_module) {
                return false;
            }

            used_symbol.import_module.as_deref() == Some(import_module)
                || self.resolve_symbol_module_path(used_sym_id).as_deref() == Some(import_module)
        })
    }

    fn symbol_declared_in_ambient_module(&self, sym_id: SymbolId, module_specifier: &str) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let mut current_idx = decl_idx;
            while let Some(parent_idx) = self.arena.parent_of(current_idx) {
                let Some(parent_node) = self.arena.get(parent_idx) else {
                    return false;
                };
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module) = self.arena.get_module(parent_node)
                    && string_literal_text(self.arena, module.name).as_deref()
                        == Some(module_specifier)
                {
                    return true;
                }
                current_idx = parent_idx;
            }
            false
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
                        if self.imported_name_is_used(binder, used, bindings.name)
                            || self.namespace_import_needed_for_shadowed_self_type(
                                bindings.name,
                                import.module_specifier,
                            )
                        {
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

                // Default imports can be used only in declaration types even
                // when the original import was not written as `import type`.
                // Without usage tracking, preserve them conservatively.
                default_count = usize::from(clause.name.is_some());

                // Named bindings: check if there are actually any specifiers
                if clause.named_bindings.is_some() {
                    if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                        && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                    {
                        if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                            // Namespace import (import * as ns): in noCheck fallback,
                            // preserve type-only namespace imports so declaration output
                            // can still reference the namespace type alias (e.g. ns.Foo).
                            named_count = usize::from(
                                clause.is_type_only
                                    && self
                                        .import_alias_targets_type_entity(import.module_specifier),
                            );
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

    /// fourth pass: Prepare import aliases before emitting anything.
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
            let content = state
                .include_sources_content
                .then(|| self.source_map_text.map(std::string::ToString::to_string))
                .flatten();
            self.writer.add_source(state.source_name.clone(), content);
        }
    }
}
