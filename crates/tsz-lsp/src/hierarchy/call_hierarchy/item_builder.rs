use super::{CallHierarchyItem, CallHierarchyProvider, ImportResolutionRequest};
use crate::symbols::document_symbols::SymbolKind;
use crate::utils::{identifier_text, node_range};
use tsz_common::position::Range;
use tsz_parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CallHierarchyProvider<'a> {
    /// Build a `CallHierarchyItem` for a function-like node.
    pub(super) fn make_call_hierarchy_item(
        &self,
        func_idx: NodeIndex,
    ) -> Option<CallHierarchyItem> {
        let node = self.arena.get(func_idx)?;
        if !(node.is_function_like() || Self::is_call_hierarchy_callable_kind(node.kind)) {
            return None;
        }
        if let Some(module_item) = self.export_equals_anonymous_function_item(func_idx) {
            return Some(module_item);
        }

        let name = self.get_function_name(func_idx);
        let kind = self.get_function_symbol_kind(func_idx);
        let range = if let Some((start, end)) = self.callable_range_bounds(func_idx) {
            Range::new(
                self.line_map.offset_to_position(start, self.source_text),
                self.line_map.offset_to_position(end, self.source_text),
            )
        } else {
            node_range(self.arena, self.line_map, self.source_text, func_idx)
        };

        // Selection range is the name identifier range, or the keyword range
        let selection_range =
            if let Some(prop_idx) = self.property_declaration_for_function_initializer(func_idx) {
                self.property_name_selection_range(prop_idx)
                    .unwrap_or_else(|| {
                        node_range(self.arena, self.line_map, self.source_text, func_idx)
                    })
            } else if let Some(name_idx) = self.get_function_name_idx(func_idx) {
                self.identifier_selection_range(name_idx)
                    .unwrap_or_else(|| {
                        node_range(self.arena, self.line_map, self.source_text, name_idx)
                    })
            } else if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                let start = self.line_map.offset_to_position(node.pos, self.source_text);
                let end = self
                    .line_map
                    .offset_to_position(node.pos.saturating_add(6), self.source_text);
                Range::new(start, end)
            } else {
                // For constructors or anonymous functions, use a small range at the start
                let start = self.line_map.offset_to_position(node.pos, self.source_text);
                let end = self
                    .line_map
                    .offset_to_position(node.pos.saturating_add(11), self.source_text); // "constructor" or similar
                Range::new(start, end)
            };

        Some(CallHierarchyItem {
            name,
            kind,
            uri: self.file_name.clone(),
            range,
            selection_range,
            container_name: self
                .container_name_for_callable(func_idx)
                .or_else(|| {
                    self.property_declaration_for_function_initializer(func_idx)
                        .and_then(|_| self.member_container_hint_for_callable(func_idx))
                })
                .or_else(|| self.member_container_hint_for_callable(func_idx)),
        })
    }

    fn export_equals_anonymous_function_item(
        &self,
        func_idx: NodeIndex,
    ) -> Option<CallHierarchyItem> {
        let func_node = self.arena.get(func_idx)?;
        if func_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            || self.get_function_name_idx(func_idx).is_some()
        {
            return None;
        }

        let parent = self.arena.get_extended(func_idx)?.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent)?;
        if parent_node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
            return None;
        }

        let export_assignment = self.arena.get_export_assignment(parent_node)?;
        if !export_assignment.is_export_equals || export_assignment.expression != func_idx {
            return None;
        }

        let start = self.line_map.offset_to_position(0, self.source_text);
        let end = self
            .line_map
            .offset_to_position(self.source_text.len() as u32, self.source_text);

        Some(CallHierarchyItem {
            name: self.file_name.clone(),
            kind: SymbolKind::Module,
            uri: self.file_name.clone(),
            range: Range::new(start, end),
            selection_range: Range::new(start, start),
            container_name: None,
        })
    }

    pub(super) fn export_equals_anonymous_function_callable(&self) -> Option<NodeIndex> {
        for node in &self.arena.nodes {
            if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                continue;
            }
            let Some(export_assignment) = self.arena.get_export_assignment(node) else {
                continue;
            };
            if !export_assignment.is_export_equals || export_assignment.expression.is_none() {
                continue;
            }
            let Some(expr_node) = self.arena.get(export_assignment.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
                continue;
            }
            if self
                .get_function_name_idx(export_assignment.expression)
                .is_some()
            {
                continue;
            }
            return Some(export_assignment.expression);
        }
        None
    }

    /// Build a `CallHierarchyItem` for a declaration node that may be
    /// a function or may be a variable holding a function expression.
    /// Issue #3753: detect when a callee's declaration is bound by an import
    /// statement. Returns enough metadata for the LSP server to re-resolve
    /// the callee in the imported module's source file:
    /// `(module_specifier, local_name, exported_name)`.
    ///
    /// `local_name` is the name visible inside the importing file. For
    /// `import { foo as bar } from "..."` the local is `bar` and the
    /// exported name is `foo`. For `import foo from "..."` the local is
    /// `foo` and the exported name is `default`. For namespace imports
    /// (`import * as ns from "..."`) we return `None` for `exported_name`.
    pub(super) fn import_resolution_request_for_decl(
        &self,
        decl_idx: NodeIndex,
        local_name: &str,
    ) -> Option<ImportResolutionRequest> {
        let node = self.arena.get(decl_idx)?;
        let kind = node.kind;

        // Walk up to find the containing IMPORT_DECLARATION while remembering
        // what kind of import-binding the decl participates in.
        let mut current = decl_idx;
        let mut exported_name: Option<String> = Some(local_name.to_string());
        let mut saw_import_kind = false;

        // The decl itself can be the import-binding name — e.g. for an
        // `import { Foo }` specifier the decl is the IMPORT_SPECIFIER node;
        // for default imports (`import Foo from "x"`) the decl is the
        // IMPORT_CLAUSE; for namespace imports the decl is NAMESPACE_IMPORT.
        if kind == syntax_kind_ext::IMPORT_SPECIFIER {
            if let Some(specifier) = self.arena.get_specifier(node) {
                // tsc preserves the property name on aliased imports as
                // the *exported* name. `import { foo as bar }` → property
                // name = foo, name = bar; the local visible here is `bar`.
                if specifier.property_name.is_some()
                    && let Some(prop_node) = self.arena.get(specifier.property_name)
                    && let Some(ident) = self.arena.get_identifier(prop_node)
                {
                    exported_name = Some(ident.escaped_text.to_string());
                }
            }
            saw_import_kind = true;
        } else if kind == syntax_kind_ext::IMPORT_CLAUSE {
            // Default import: the local-name binding IS the default export.
            exported_name = Some("default".to_string());
            saw_import_kind = true;
        } else if kind == syntax_kind_ext::NAMESPACE_IMPORT {
            // Namespace import — no specific exported name.
            exported_name = None;
            saw_import_kind = true;
        }

        // The decl can also be an inner identifier — when the binder records
        // the IMPORT_SPECIFIER's `name` identifier rather than the specifier
        // node itself. Walk up via the parent chain to find an enclosing
        // import construct.
        let mut steps = 0;
        while !saw_import_kind {
            if steps > 8 {
                break;
            }
            steps += 1;
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            current = parent;
            let parent_kind = parent_node.kind;
            if parent_kind == syntax_kind_ext::IMPORT_SPECIFIER
                || parent_kind == syntax_kind_ext::IMPORT_CLAUSE
                || parent_kind == syntax_kind_ext::NAMESPACE_IMPORT
            {
                if parent_kind == syntax_kind_ext::IMPORT_CLAUSE {
                    exported_name = Some("default".to_string());
                } else if parent_kind == syntax_kind_ext::NAMESPACE_IMPORT {
                    exported_name = None;
                } else if parent_kind == syntax_kind_ext::IMPORT_SPECIFIER
                    && let Some(specifier) = self.arena.get_specifier(parent_node)
                    && specifier.property_name.is_some()
                    && let Some(prop_node) = self.arena.get(specifier.property_name)
                    && let Some(ident) = self.arena.get_identifier(prop_node)
                {
                    exported_name = Some(ident.escaped_text.to_string());
                }
                saw_import_kind = true;
            }
        }
        if !saw_import_kind {
            return None;
        }

        // Walk the rest of the way up to the IMPORT_DECLARATION to read its
        // `module_specifier`.
        for _ in 0..8 {
            let ext = self.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                let import_decl = self.arena.get_import_decl(parent_node)?;
                let spec_node = self.arena.get(import_decl.module_specifier)?;
                let spec_lit = self.arena.get_literal(spec_node)?;
                return Some(ImportResolutionRequest {
                    module_specifier: spec_lit.text.clone(),
                    local_name: local_name.to_string(),
                    exported_name,
                });
            }
            current = parent;
        }
        None
    }

    pub(super) fn make_call_hierarchy_item_for_declaration(
        &self,
        decl_idx: NodeIndex,
        symbol_name: &str,
    ) -> Option<CallHierarchyItem> {
        let node = self.arena.get(decl_idx)?;

        if node.is_class_like() {
            let mut selection_range =
                node_range(self.arena, self.line_map, self.source_text, decl_idx);
            if let Some(class_decl) = self.arena.get_class(node)
                && class_decl.name.is_some()
            {
                selection_range = self
                    .identifier_selection_range(class_decl.name)
                    .unwrap_or_else(|| {
                        node_range(self.arena, self.line_map, self.source_text, class_decl.name)
                    });
            }
            let range = self.class_range(decl_idx).unwrap_or_else(|| {
                node_range(self.arena, self.line_map, self.source_text, decl_idx)
            });
            return Some(CallHierarchyItem {
                name: symbol_name.to_string(),
                kind: SymbolKind::Class,
                uri: self.file_name.clone(),
                range,
                selection_range,
                container_name: self.container_name_for_callable(decl_idx),
            });
        }

        if let Some(callable_idx) = self.callable_from_declaration(decl_idx) {
            return self.make_call_hierarchy_item(callable_idx);
        }

        // If the declaration itself is function-like, use make_call_hierarchy_item
        if node.is_function_like() {
            return self.make_call_hierarchy_item(decl_idx);
        }

        // Otherwise (e.g. class/variable declaration), build an item from declaration info.
        let kind = SymbolKind::Function;
        let selection_range = node_range(self.arena, self.line_map, self.source_text, decl_idx);
        let range = node_range(self.arena, self.line_map, self.source_text, decl_idx);
        Some(CallHierarchyItem {
            name: symbol_name.to_string(),
            kind,
            uri: self.file_name.clone(),
            range,
            selection_range,
            container_name: self.container_name_for_callable(decl_idx),
        })
    }

    fn class_range(&self, class_idx: NodeIndex) -> Option<Range> {
        let class_node = self.arena.get(class_idx)?;
        let class_decl = self.arena.get_class(class_node)?;
        let mut start_offset = class_decl
            .modifiers
            .as_ref()
            .and_then(|mods| mods.nodes.first().copied())
            .and_then(|mod_idx| self.arena.pos_at(mod_idx))
            .unwrap_or(class_node.pos);
        if class_node.pos > 0 {
            let bytes = self.source_text.as_bytes();
            let mut line_start = class_node.pos as usize;
            while line_start > 0 && bytes[line_start - 1] != b'\n' {
                line_start -= 1;
            }
            let prefix = &self.source_text[line_start..class_node.pos as usize];
            if let Some(export_offset) = prefix.find("export") {
                start_offset = (line_start + export_offset) as u32;
            }
        }
        let end_offset = self
            .find_function_body_end_offset_from_source(start_offset)
            .unwrap_or(class_node.end);
        Some(Range::new(
            self.line_map
                .offset_to_position(start_offset, self.source_text),
            self.line_map
                .offset_to_position(end_offset, self.source_text),
        ))
    }

    pub(super) fn make_call_hierarchy_item_for_caller(
        &self,
        caller_idx: NodeIndex,
    ) -> Option<CallHierarchyItem> {
        if let Some(item) = self.make_call_hierarchy_item(caller_idx) {
            return Some(item);
        }
        let caller_node = self.arena.get(caller_idx)?;
        if caller_node.is_class_like() {
            let class_decl = self.arena.get_class(caller_node)?;
            let class_name = self.get_identifier_text(class_decl.name)?;
            return self.make_call_hierarchy_item_for_declaration(caller_idx, &class_name);
        }
        None
    }

    pub(super) fn prepare_item_from_reference(
        &self,
        node_idx: NodeIndex,
    ) -> Option<CallHierarchyItem> {
        let ident_idx = self.reference_identifier_at_or_above(node_idx)?;
        let (_sym, decl_idx, name) = self.resolve_callee_symbol(ident_idx)?;
        self.make_call_hierarchy_item_for_declaration(decl_idx, &name)
    }

    pub(super) fn resolve_reference_callable(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let ident_idx = self.reference_identifier_at_or_above(node_idx)?;
        let (_sym, decl_idx, _name) = self.resolve_callee_symbol(ident_idx)?;
        self.callable_from_declaration(decl_idx)
    }

    fn callable_from_declaration(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(decl_idx)?;
        if self.is_call_hierarchy_callable_node(decl_idx) {
            return Some(decl_idx);
        }

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.arena.get_variable_declaration(node)?;
            if var_decl.initializer.is_some() {
                let init_node = self.arena.get(var_decl.initializer)?;
                if init_node.is_function_like() {
                    return Some(var_decl.initializer);
                }
            }
        }

        if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let prop_decl = self.arena.get_property_decl(node)?;
            if prop_decl.initializer.is_some() {
                let init_node = self.arena.get(prop_decl.initializer)?;
                if init_node.is_function_like() {
                    return Some(prop_decl.initializer);
                }
            }
        }

        if (node.is_class_like())
            && let Some(ctor_idx) = self.class_constructor_node(decl_idx)
        {
            return Some(ctor_idx);
        }

        None
    }

    fn reference_identifier_at_or_above(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = node_idx;
        for _ in 0..8 {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16
                && self.is_inside_call_or_decorator_reference(current)
            {
                return Some(current);
            }
            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                || node.kind == syntax_kind_ext::NEW_EXPRESSION
            {
                let call_data = self.arena.get_call_expr(node)?;
                let ident = self.get_callee_identifier(call_data.expression);
                if ident.is_some() {
                    return Some(ident);
                }
            }
            let ext = self.arena.get_extended(current)?;
            current = ext.parent;
            if current.is_none() {
                break;
            }
        }
        None
    }

    /// Get the text of an identifier node.
    pub(super) fn get_identifier_text(&self, node_idx: NodeIndex) -> Option<String> {
        identifier_text(self.arena, node_idx)
    }

    pub(super) fn script_call_hierarchy_item(&self) -> CallHierarchyItem {
        let start_offset = 0u32;
        let end_offset = self.source_text.len() as u32;
        let start = self
            .line_map
            .offset_to_position(start_offset, self.source_text);
        let end = self
            .line_map
            .offset_to_position(end_offset, self.source_text);
        CallHierarchyItem {
            name: self.file_name.clone(),
            kind: SymbolKind::File,
            uri: self.file_name.clone(),
            range: Range::new(start, end),
            selection_range: Range::new(start, start),
            container_name: None,
        }
    }
}
