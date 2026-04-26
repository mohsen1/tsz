//! Declaration emit helpers: type query resolution, symbol accessibility,
//! unique symbol nameability, and module path resolution utilities.

use crate::query_boundaries::common::{collect_referenced_types, lazy_def_id};
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn first_private_value_type_query_name_in_exported_type_annotation(
        &self,
        type_annotation: NodeIndex,
    ) -> Option<(NodeIndex, String)> {
        let mut stack = vec![type_annotation];

        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::TYPE_QUERY
                && let Some(type_query) = self.ctx.arena.get_type_query(node)
                && let Some(root_name) = self.type_query_root_identifier_name(type_query.expr_name)
                && !root_name.is_empty()
            {
                if let Some(sym_id) =
                    self.resolve_type_query_value_symbol_for_emit(type_query.expr_name)
                    && self.value_symbol_is_private_for_exported_type_query(sym_id)
                {
                    return Some((type_query.expr_name, root_name));
                }

                if !self.type_query_value_name_is_accessible(type_query.expr_name)
                    && self.has_inaccessible_current_file_value_name(&root_name)
                {
                    return Some((type_query.expr_name, root_name));
                }
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }

        None
    }

    fn type_query_root_identifier_name(&self, expr_name: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            return self.type_query_root_identifier_name(qualified.left);
        }

        None
    }

    fn type_query_value_name_is_accessible(&self, expr_name: NodeIndex) -> bool {
        self.resolve_type_query_value_symbol_for_emit(expr_name)
            .is_some()
    }

    fn resolve_type_query_value_symbol_for_emit(&self, expr_name: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_name)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol_without_tracking(expr_name);
        }

        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qualified = self.ctx.arena.get_qualified_name(node)?;
        let left_sym_id = self.resolve_type_query_value_symbol_for_emit(qualified.left)?;
        let right_name = self.ctx.arena.get_identifier_text(qualified.right)?;
        let left_symbol = self.get_symbol_from_any_binder(left_sym_id)?;

        left_symbol.exports.as_ref().and_then(|exports| {
            exports.iter().find_map(|(name, sym_id)| {
                if name == right_name {
                    Some(*sym_id)
                } else {
                    None
                }
            })
        })
    }

    fn value_symbol_is_private_for_exported_type_query(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };

        symbol
            .all_declarations()
            .into_iter()
            .any(|decl_idx| self.declaration_is_hidden_from_declaration_emit(decl_idx))
    }

    fn declaration_is_hidden_from_declaration_emit(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent.kind == syntax_kind_ext::SOURCE_FILE
                || parent.kind == syntax_kind_ext::MODULE_BLOCK
            {
                return false;
            }

            if parent.kind == syntax_kind_ext::BLOCK {
                let Some(block_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return true;
                };
                let Some(block_parent) = self.ctx.arena.get(block_ext.parent) else {
                    return true;
                };

                return !matches!(
                    block_parent.kind,
                    syntax_kind_ext::FUNCTION_DECLARATION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::CONSTRUCTOR
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR
                        | syntax_kind_ext::MODULE_DECLARATION
                        | syntax_kind_ext::MODULE_BLOCK
                );
            }

            current = parent_idx;
        }

        false
    }

    fn has_inaccessible_current_file_value_name(&self, name: &str) -> bool {
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(name) {
            let is_accessible_value =
                self.ctx
                    .binder
                    .get_symbol(local_sym_id)
                    .is_some_and(|symbol| {
                        !symbol.is_type_only && self.local_value_name_resolves_to(local_sym_id)
                    });
            if is_accessible_value {
                return false;
            }
        }

        self.ctx.binder.symbols.iter().any(|symbol| {
            !symbol.is_type_only
                && symbol.escaped_name == name
                && (symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32)
        })
    }

    pub(crate) fn first_unnameable_external_unique_symbol_reference(
        &self,
        inferred_type: TypeId,
    ) -> Option<(String, String)> {
        let mut result = None;

        crate::query_boundaries::common::walk_referenced_types(
            self.ctx.types,
            inferred_type,
            |type_id| {
                if result.is_some() {
                    return;
                }

                if let Some(shape) = query::object_shape(self.ctx.types, type_id)
                    && let Some(info) = self.inspect_unique_symbol_properties(&shape.properties)
                {
                    result = Some(info);
                    return;
                }
                if let Some(shape) = query::callable_shape(self.ctx.types, type_id)
                    && let Some(info) = self.inspect_unique_symbol_properties(&shape.properties)
                {
                    result = Some(info);
                }
            },
        );

        result
    }

    pub(crate) fn first_inaccessible_external_unique_symbol_reference(
        &self,
        inferred_type: TypeId,
    ) -> Option<SymbolId> {
        let mut result = None;

        crate::query_boundaries::common::walk_referenced_types(
            self.ctx.types,
            inferred_type,
            |type_id| {
                if result.is_some() {
                    return;
                }

                let Some(sym_ref) =
                    crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, type_id)
                else {
                    return;
                };

                let sym_id = SymbolId(sym_ref.0);
                if self.unique_symbol_type_is_inaccessible(sym_id) {
                    result = Some(sym_id);
                }
            },
        );

        result
    }

    pub(crate) fn first_inaccessible_unique_symbol_reference_from_lazy_defs(
        &self,
        inferred_type: TypeId,
    ) -> Option<SymbolId> {
        let referenced_types = collect_referenced_types(self.ctx.types, inferred_type);

        for &type_id in &referenced_types {
            let Some(def_id) = lazy_def_id(self.ctx.types, type_id) else {
                continue;
            };
            let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id) else {
                continue;
            };

            let mut visited = FxHashSet::default();
            if self.symbol_references_inaccessible_unique_symbol_type(sym_id, &mut visited) {
                return Some(sym_id);
            }
        }

        None
    }

    pub(crate) fn first_non_portable_type_reference(
        &self,
        inferred_type: TypeId,
    ) -> Option<(String, String)> {
        let referenced_types = collect_referenced_types(self.ctx.types, inferred_type);
        for &type_id in &referenced_types {
            if let Some(def_id) = lazy_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }

            if let Some(shape) = query::object_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }

            if let Some(shape) = query::callable_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }
        }

        None
    }

    pub(crate) fn first_private_name_from_external_module_reference(
        &mut self,
        inferred_type: TypeId,
    ) -> Option<(String, String)> {
        let referenced_types = collect_referenced_types(self.ctx.types, inferred_type);

        for &type_id in &referenced_types {
            if let Some(def_id) = lazy_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
            {
                let owner_file_hint = self
                    .ctx
                    .definition_store
                    .get(def_id)
                    .and_then(|def| def.file_id);
                if let Some(info) =
                    self.private_external_module_nameability_info(sym_id, owner_file_hint)
                {
                    return Some(info);
                }
            }

            if let Some(shape) = query::object_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.private_external_module_nameability_info(sym_id, None)
            {
                return Some(info);
            }

            if let Some(shape) = query::callable_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.private_external_module_nameability_info(sym_id, None)
            {
                return Some(info);
            }
        }

        None
    }

    fn private_external_module_nameability_info(
        &mut self,
        sym_id: SymbolId,
        owner_file_hint: Option<u32>,
    ) -> Option<(String, String)> {
        use tsz_binder::symbol_flags;

        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            .unwrap_or(sym_id);
        let symbol = self
            .get_symbol_globally(resolved_sym_id)
            .or_else(|| self.get_cross_file_symbol(resolved_sym_id))
            .or_else(|| self.get_symbol_from_any_binder(resolved_sym_id))?;
        let referenced_name = symbol.escaped_name.clone();

        if referenced_name.is_empty() || referenced_name.starts_with("__") {
            return None;
        }
        // Top-level JSDoc typedef aliases in the current checked-JS file are
        // declaration-emitted (and can be lifted to exported aliases when needed),
        // so they are nameable and must not trigger TS9006.
        if self.file_has_jsdoc_typedef_named(self.ctx.current_file_idx, &referenced_name) {
            return None;
        }

        let file_idx = owner_file_hint
            .filter(|idx| *idx != u32::MAX)
            .or_else(|| {
                self.ctx
                    .resolve_symbol_file_index(resolved_sym_id)
                    .map(|idx| idx as u32)
            })
            .or_else(|| {
                self.symbol_decl_file_idx(resolved_sym_id)
                    .filter(|&idx| idx != u32::MAX)
            })
            .or_else(|| {
                self.ctx.all_binders.as_ref().and_then(|binders| {
                    binders.iter().enumerate().find_map(|(idx, binder)| {
                        binder.get_symbol(resolved_sym_id).and_then(|candidate| {
                            (candidate.escaped_name == referenced_name
                                && (candidate.flags & tsz_binder::symbol_flags::TYPE) != 0)
                                .then_some(idx as u32)
                        })
                    })
                })
            })
            .or_else(|| self.find_external_private_symbol_owner_file(&referenced_name))?;
        if file_idx == self.ctx.current_file_idx as u32 {
            return None;
        }

        let target_binder = self.ctx.get_binder_for_file(file_idx as usize)?;
        let target_sym_id = if target_binder
            .get_symbol(resolved_sym_id)
            .is_some_and(|sym| sym.escaped_name == referenced_name)
        {
            resolved_sym_id
        } else if let Some(sym_id) = target_binder.file_locals.get(&referenced_name) {
            sym_id
        } else {
            target_binder
                .get_symbols()
                .find_all_by_name(&referenced_name)
                .iter()
                .find_map(|candidate_id| {
                    target_binder
                        .get_symbol(*candidate_id)
                        .is_some_and(|sym| sym.escaped_name == referenced_name)
                        .then_some(*candidate_id)
                })?
        };
        if !target_binder.is_external_module() {
            return None;
        }

        let target_file_name = self
            .ctx
            .get_arena_for_file(file_idx)
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())?;
        let target_file_stem = target_file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(target_file_name.as_str())
            .to_string();
        let target_basename = target_file_stem
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or(target_file_stem.as_str())
            .to_string();
        let normalized_file_name = target_file_name.replace('\\', "/");
        let normalized_stem = target_file_stem.replace('\\', "/");
        let normalized_without_dot = normalized_file_name
            .strip_prefix("./")
            .unwrap_or(normalized_file_name.as_str())
            .to_string();
        let stem_without_dot = normalized_stem
            .strip_prefix("./")
            .unwrap_or(normalized_stem.as_str())
            .to_string();

        let mut export_keys = vec![
            target_file_name,
            normalized_file_name,
            normalized_without_dot,
            target_file_stem,
            normalized_stem,
            stem_without_dot,
            target_basename,
        ];
        export_keys.sort();
        export_keys.dedup();

        let is_exported_from_target = export_keys.iter().any(|key| {
            target_binder
                .module_exports
                .get(key)
                .is_some_and(|exports| {
                    exports.iter().any(|(_, &export_sym_id)| {
                        export_sym_id == target_sym_id
                            || export_sym_id == resolved_sym_id
                            || target_binder.resolve_import_symbol(export_sym_id)
                                == Some(target_sym_id)
                            || target_binder.resolve_import_symbol(export_sym_id)
                                == Some(resolved_sym_id)
                            || self.ctx.binder.resolve_import_symbol(export_sym_id)
                                == Some(resolved_sym_id)
                            || self
                                .resolve_alias_symbol(export_sym_id, &mut AliasCycleTracker::new())
                                == Some(resolved_sym_id)
                    })
                })
        });
        if is_exported_from_target {
            return None;
        }

        // `export = C` modules can expose additional type members through the export=
        // symbol's namespace surface (e.g. `import("./mod").Member`). Those members are
        // nameable from consumers and must not trigger TS9006 private-name diagnostics.
        let is_exported_via_export_equals_namespace = export_keys.iter().any(|key| {
            let Some(exports) = self.ctx.module_exports_for_module(target_binder, key) else {
                return false;
            };
            let Some(export_equals_sym_id) = exports.get("export=") else {
                return false;
            };

            let matches_resolved_symbol = |candidate_sym_id| {
                candidate_sym_id == resolved_sym_id
                    || target_binder.resolve_import_symbol(candidate_sym_id)
                        == Some(resolved_sym_id)
                    || self.ctx.binder.resolve_import_symbol(candidate_sym_id)
                        == Some(resolved_sym_id)
                    || self.resolve_alias_symbol(candidate_sym_id, &mut AliasCycleTracker::new())
                        == Some(resolved_sym_id)
            };

            if matches_resolved_symbol(export_equals_sym_id) {
                return true;
            }

            let mut candidate_member_ids = Vec::new();
            if let Some(export_equals_symbol) = target_binder.get_symbol(export_equals_sym_id) {
                if let Some(ns_exports) = &export_equals_symbol.exports {
                    candidate_member_ids.extend(ns_exports.iter().map(|(_, &sym_id)| sym_id));
                }
                if let Some(ns_members) = &export_equals_symbol.members {
                    candidate_member_ids.extend(ns_members.iter().map(|(_, &sym_id)| sym_id));
                }
            }

            candidate_member_ids
                .into_iter()
                .any(matches_resolved_symbol)
        });
        if is_exported_via_export_equals_namespace {
            return None;
        }

        let is_named_commonjs_export = self
            .resolve_js_export_surface(file_idx as usize)
            .named_exports
            .iter()
            .any(|prop| self.ctx.types.resolve_atom(prop.name) == referenced_name);
        if is_named_commonjs_export {
            return None;
        }

        let locally_nameable = self
            .ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(local_symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                let is_from_current_file = local_symbol.decl_file_idx == u32::MAX
                    || local_symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = local_symbol.has_any_flags(symbol_flags::ALIAS);
                if !is_from_current_file && !is_import {
                    return false;
                }

                local_sym_id == target_sym_id
                    || self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            });
        if locally_nameable {
            return None;
        }

        // If the current file references this target file via a JSDoc
        // `typeof import(<spec>)` whose `<spec>` is rejected by tsc as
        // unresolvable (e.g. an absolute `/...` path that has no matching
        // ambient module), the program-level error is the resolution failure
        // (TS2307/TS2792). Stacking a TS9006 about a "private name from
        // <module>" on top of that just contradicts the prior error — tsc
        // does not emit it. Detect this case by re-running the same
        // unresolvable-specifier predicate the JSDoc diagnostic check uses.
        if self.current_file_jsdoc_typeof_import_unresolvable_for_target(file_idx) {
            return None;
        }

        let module_specifier = self.module_specifier_for_file(file_idx)?;
        Some((referenced_name, module_specifier))
    }

    /// Returns true if the current file contains a JSDoc `@type {typeof
    /// import("<spec>")}` whose `<spec>` is rejected by tsc as unresolvable
    /// (rooted/absolute paths with no ambient module fallback) and that
    /// specifier — via our resolver's basename probing — still happens to
    /// land on `target_file_idx`. tsc emits TS2307/TS2792 for such
    /// specifiers and intentionally suppresses follow-on diagnostics that
    /// walk the (technically resolved) target file's private symbols.
    fn current_file_jsdoc_typeof_import_unresolvable_for_target(
        &self,
        target_file_idx: u32,
    ) -> bool {
        let arena = self.ctx.arena;
        let Some(sf) = arena.source_files.first() else {
            return false;
        };
        let source_text = sf.text.as_ref();
        // Cheap text scan first — most files have no JSDoc `import(` at all.
        if !source_text.contains("import(") {
            return false;
        }

        let mut search_start = 0usize;
        while let Some(rel) = source_text[search_start..].find("import(") {
            let abs = search_start + rel + "import(".len();
            // Skip optional whitespace
            let bytes = source_text.as_bytes();
            let mut cursor = abs;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            // Expect a quoted specifier; skip non-string forms (unsupported here).
            if cursor >= bytes.len() {
                break;
            }
            let quote = bytes[cursor];
            if quote != b'"' && quote != b'\'' {
                search_start = abs;
                continue;
            }
            cursor += 1;
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            if cursor >= bytes.len() {
                break;
            }
            let specifier = &source_text[start..cursor];
            search_start = cursor + 1;

            // Match the JSDoc TS2307 check: rooted specifiers (starting with
            // `/`) are unresolvable per tsc unless an ambient module declares
            // them. Plain relative/non-rooted forms are out of scope here —
            // they go through the standard resolution-error pipeline.
            if !specifier.starts_with('/') {
                continue;
            }
            let has_ambient_module = self
                .ctx
                .declared_modules_contains(self.ctx.binder, specifier)
                || self
                    .ctx
                    .binder
                    .shorthand_ambient_modules
                    .contains(specifier);
            if has_ambient_module {
                continue;
            }
            let resolved = self
                .ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, specifier)
                .or_else(|| self.ctx.resolve_import_target(specifier));
            if resolved.map(|idx| idx as u32) == Some(target_file_idx) {
                return true;
            }
        }

        false
    }

    fn find_external_private_symbol_owner_file(&self, symbol_name: &str) -> Option<u32> {
        let all_binders = self.ctx.all_binders.as_ref()?;
        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx || !binder.is_external_module() {
                continue;
            }

            let target_file_name = self
                .ctx
                .get_arena_for_file(file_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
                .unwrap_or_default();

            let candidates = binder.get_symbols().find_all_by_name(symbol_name);
            let has_private_candidate = candidates.iter().any(|candidate_id| {
                binder
                    .get_symbol(*candidate_id)
                    .is_some_and(|sym| sym.escaped_name == symbol_name)
                    && !binder
                        .module_exports
                        .get(&target_file_name)
                        .is_some_and(|exports| exports.iter().any(|(_, &sid)| sid == *candidate_id))
            });
            if has_private_candidate {
                return Some(file_idx as u32);
            }
        }

        None
    }

    fn find_non_portable_symbol_reference(&self, sym_id: SymbolId) -> Option<(String, String)> {
        use std::path::{Component, Path};
        use tsz_binder::symbol_flags;

        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            .unwrap_or(sym_id);

        let symbol = self.get_symbol_from_any_binder(resolved_sym_id)?;
        let type_name = symbol.escaped_name.clone();
        let source_path = self.symbol_source_path(resolved_sym_id)?;

        let components: Vec<_> = Path::new(&source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();

        // Case 1: Import alias with a bare module specifier pointing into
        // nested node_modules.  The "from" path uses the import specifier.
        // The parent package is between the FIRST node_modules and the second.
        if nm_positions.len() >= 2
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(import_module) = &symbol.import_module
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            let first_nm = nm_positions[0];
            let second_nm = nm_positions[1];
            let pkg_start = first_nm + 1;
            let pkg_end = second_nm;

            let parent_parts: Vec<String> = components[pkg_start..pkg_end]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_parts.is_empty() {
                let from_path =
                    format!("{}/node_modules/{}", parent_parts.join("/"), import_module);
                return Some((type_name, from_path));
            }
        }

        // Case 2: Any type whose source file lives inside nested
        // node_modules (2+ segments).  A type from a transitive dependency
        // is non-portable regardless of whether the inner package has a
        // package.json or "exports" field — consumers may resolve a
        // different version of the transitive dep.
        if nm_positions.len() >= 2 {
            let first_nm = nm_positions[0];
            let second_nm = nm_positions[1];

            let nested_start = second_nm + 1;
            let nested_len = if components.get(nested_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let parent_parts: Vec<String> = components[first_nm + 1..second_nm]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            let nested_parts: Vec<String> = components[nested_start..nested_start + nested_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_parts.is_empty() && !nested_parts.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_parts.join("/"),
                    nested_parts.join("/")
                );
                return Some((type_name, from_path));
            }
        }

        // Case 3 (single node_modules, private subpath) requires the
        // declaration emitter's context to determine if the type actually
        // appears in the output. The type-graph walk here is too aggressive:
        // it finds types in conditional branches that may resolve away,
        // leading to false TS2883 emissions that tsc avoids.

        None
    }

    fn symbol_source_path(&self, sym_id: SymbolId) -> Option<String> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) {
            let arena = self.ctx.get_arena_for_file(file_idx as u32);
            if let Some(source_file) = arena.source_files.first() {
                return Some(source_file.file_name.clone());
            }
        }

        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        if symbol.decl_file_idx != u32::MAX {
            let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
            if let Some(source_file) = arena.source_files.first() {
                return Some(source_file.file_name.clone());
            }
        }

        if let Some(arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
            && let Some(source_file) = arena.source_files.first()
        {
            return Some(source_file.file_name.clone());
        }

        for binder in self
            .ctx
            .all_binders
            .as_ref()
            .into_iter()
            .flat_map(|binders| binders.iter())
        {
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(source_file) = arena.source_files.first()
            {
                return Some(source_file.file_name.clone());
            }
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        if let Some(source_file) = arena.source_files.first() {
                            return Some(source_file.file_name.clone());
                        }
                    }
                }
            }
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            let binder = &lib_ctx.binder;
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(source_file) = arena.source_files.first()
            {
                return Some(source_file.file_name.clone());
            }
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        if let Some(source_file) = arena.source_files.first() {
                            return Some(source_file.file_name.clone());
                        }
                    }
                }
            }
        }

        None
    }

    fn inspect_unique_symbol_properties(
        &self,
        properties: &[tsz_solver::PropertyInfo],
    ) -> Option<(String, String)> {
        for prop in properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name);
            let Some(symbol_id) = prop_name.strip_prefix("__unique_") else {
                continue;
            };
            let Ok(symbol_raw) = symbol_id.parse::<u32>() else {
                continue;
            };
            if let Some(info) = self.unique_symbol_emit_nameability_info(SymbolId(symbol_raw)) {
                return Some(info);
            }
        }
        None
    }

    fn unique_symbol_emit_nameability_info(&self, sym_id: SymbolId) -> Option<(String, String)> {
        let (reported_name, root_sym_id, file_idx) = self.unique_symbol_report_target(sym_id)?;
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return None;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return None;
        }

        if self.local_value_name_resolves_to(root_sym_id) {
            return None;
        }

        let module_specifier = self.module_specifier_for_file(file_idx)?;
        Some((reported_name, module_specifier))
    }

    fn unique_symbol_type_is_inaccessible(&self, sym_id: SymbolId) -> bool {
        let Some((_, root_sym_id, file_idx)) = self.unique_symbol_report_target(sym_id) else {
            return false;
        };
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return false;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return false;
        }

        !self.local_value_name_resolves_to(root_sym_id)
    }

    pub(crate) fn exported_variable_initializer_symbol(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol_without_tracking(expr_idx);
        }

        None
    }

    pub(crate) fn symbol_initializer_references_builtin_global_this(
        &self,
        sym_id: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let sym_id = self
            .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            .unwrap_or(sym_id);
        if !visited.insert(sym_id) {
            return false;
        }

        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };

        let decl_candidates = symbol.all_declarations();

        let owner_file_idx = self.symbol_decl_file_idx(sym_id);

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(owner_binder) = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            {
                if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
                }
                if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                    candidate_arenas.push(symbol_arena.as_ref());
                }
            }
            if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if candidate_arenas.is_empty() {
                candidate_arenas.push(self.ctx.arena);
            }

            for arena in candidate_arenas {
                let variable_decl_idx = decl_idx;
                let Some(mut node) = arena.get(variable_decl_idx) else {
                    continue;
                };

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    let mut parent = arena
                        .get_extended(variable_decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    while parent.is_some() {
                        let Some(parent_node) = arena.get(parent) else {
                            break;
                        };
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            node = parent_node;
                            break;
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }
                }

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let Some(var_decl) = arena.get_variable_declaration(node) else {
                    continue;
                };
                let initializer = var_decl.initializer;
                if initializer.is_none() {
                    continue;
                }

                if arena
                    .get(initializer)
                    .and_then(|init_node| arena.get_identifier(init_node))
                    .is_some_and(|ident| ident.escaped_text == "globalThis")
                {
                    let init_sym_id = self
                        .value_symbol_in_arena(arena, initializer)
                        .unwrap_or(SymbolId::NONE);
                    let init_sym_id = self
                        .resolve_alias_symbol(init_sym_id, &mut AliasCycleTracker::new())
                        .unwrap_or(init_sym_id);
                    if init_sym_id.is_some()
                        && self.symbol_decl_file_idx(init_sym_id) != owner_file_idx
                    {
                        return true;
                    }
                }

                if let Some(next_sym_id) = self.value_symbol_in_arena(arena, initializer)
                    && self.symbol_initializer_references_builtin_global_this(next_sym_id, visited)
                {
                    return true;
                }
            }
        }

        false
    }

    fn value_symbol_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.ctx.get_binder_for_arena(arena)?;
        if let Some(sym_id) = binder.get_node_symbol(expr_idx) {
            return Some(sym_id);
        }

        let node = arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = arena.get_identifier(node)?;
        binder.file_locals.get(ident.escaped_text.as_str())
    }

    fn symbol_decl_file_idx(&self, sym_id: SymbolId) -> Option<u32> {
        self.ctx
            .resolve_symbol_file_index(sym_id)
            .map(|idx| idx as u32)
            .or_else(|| {
                self.get_symbol_from_any_binder(sym_id)
                    .map(|symbol| symbol.decl_file_idx)
            })
    }

    pub(crate) fn symbol_references_inaccessible_unique_symbol_type(
        &self,
        sym_id: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let sym_id = self
            .resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            .unwrap_or(sym_id);
        if !visited.insert(sym_id) {
            return false;
        }

        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };

        let decl_candidates = symbol.all_declarations();

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(owner_binder) = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            {
                if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
                }
                if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                    candidate_arenas.push(symbol_arena.as_ref());
                }
            }
            if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            candidate_arenas.push(self.ctx.arena);

            for arena in candidate_arenas {
                if self.node_references_inaccessible_unique_symbol_type(
                    arena, decl_idx, sym_id, visited,
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn node_references_inaccessible_unique_symbol_type(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
        owner_sym_id: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_OPERATOR
            && arena.get_type_operator(node).is_some_and(|op| {
                op.operator == SyntaxKind::UniqueKeyword as u16
                    && self.node_is_symbol_type_reference(arena, op.type_node)
            })
        {
            return self.type_symbol_is_inaccessible(owner_sym_id);
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(type_sym_id) =
                self.type_reference_symbol_in_arena(arena, type_ref.type_name)
            && self.symbol_references_inaccessible_unique_symbol_type(type_sym_id, visited)
        {
            return true;
        }

        arena.get_children(node_idx).into_iter().any(|child| {
            self.node_references_inaccessible_unique_symbol_type(
                arena,
                child,
                owner_sym_id,
                visited,
            )
        })
    }

    fn node_is_symbol_type_reference(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };

        arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    fn type_reference_symbol_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_name_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.ctx.get_binder_for_arena(arena)?;
        if let Some(sym_id) = binder.get_node_symbol(type_name_idx) {
            return Some(sym_id);
        }

        let node = arena.get(type_name_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = arena.get_identifier(node)?;
        binder.file_locals.get(ident.escaped_text.as_str())
    }

    fn type_symbol_is_inaccessible(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };
        let file_idx = symbol.decl_file_idx;
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return false;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return false;
        }

        !self.local_name_resolves_to(sym_id)
    }

    fn local_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
                    return false;
                }

                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    fn unique_symbol_report_target(&self, sym_id: SymbolId) -> Option<(String, SymbolId, u32)> {
        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut namespace_names = Vec::new();
        let mut root_namespace_sym = SymbolId::NONE;
        let mut parent_sym_id = symbol.parent;
        while parent_sym_id.is_some() {
            let Some(parent_symbol) = self.get_symbol_from_any_binder(parent_sym_id) else {
                break;
            };
            if (parent_symbol.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                break;
            }
            namespace_names.push(parent_symbol.escaped_name.clone());
            root_namespace_sym = parent_sym_id;
            parent_sym_id = parent_symbol.parent;
        }
        if !namespace_names.is_empty() {
            namespace_names.reverse();
            return Some((namespace_names.join("."), root_namespace_sym, file_idx));
        }

        let matches_symbol = |candidate_sym_id: SymbolId| {
            if candidate_sym_id == sym_id {
                return true;
            }
            let Some(candidate_symbol) = owner_binder.get_symbol(candidate_sym_id) else {
                return false;
            };
            candidate_symbol.escaped_name == symbol.escaped_name
                && (candidate_symbol.value_declaration_span == symbol.value_declaration_span
                    || candidate_symbol.first_declaration_span == symbol.first_declaration_span)
        };

        for candidate in owner_binder.symbols.iter() {
            if (candidate.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                continue;
            }
            let Some(exports) = candidate.exports.as_ref() else {
                continue;
            };
            if !exports
                .iter()
                .any(|(_, exported_sym_id)| matches_symbol(*exported_sym_id))
            {
                continue;
            }
            return Some((candidate.escaped_name.clone(), candidate.id, file_idx));
        }

        let decl_candidates = symbol.all_declarations();

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if std::ptr::eq(owner_binder, self.ctx.binder) {
                candidate_arenas.push(self.ctx.arena);
            }

            for arena in candidate_arenas {
                let variable_decl_idx = decl_idx;
                let Some(mut node) = arena.get(variable_decl_idx) else {
                    continue;
                };

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    let mut parent = arena
                        .get_extended(variable_decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    while parent.is_some() {
                        let Some(parent_node) = arena.get(parent) else {
                            break;
                        };
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            node = parent_node;
                            break;
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }
                }

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let mut namespace_names = Vec::new();
                let mut namespace_nodes = Vec::new();
                let mut parent = arena
                    .get_extended(variable_decl_idx)
                    .map_or(NodeIndex::NONE, |info| info.parent);
                while parent.is_some() {
                    let Some(parent_node) = arena.get(parent) else {
                        break;
                    };
                    if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module) = arena.get_module(parent_node)
                        && let Some(name_node) = arena.get(module.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_ident) = arena.get_identifier(name_node)
                    {
                        namespace_names.push(name_ident.escaped_text.clone());
                        namespace_nodes.push(parent);
                    }
                    parent = arena
                        .get_extended(parent)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                }

                if !namespace_names.is_empty() {
                    namespace_names.reverse();
                    let display_name = namespace_names.join(".");
                    let root_namespace_idx = *namespace_nodes.last().unwrap_or(&NodeIndex::NONE);
                    let root_sym_id = self
                        .ctx
                        .get_binder_for_arena(arena)
                        .and_then(|binder| binder.get_node_symbol(root_namespace_idx))
                        .unwrap_or(sym_id);
                    return Some((display_name, root_sym_id, file_idx));
                }

                return Some((symbol.escaped_name.clone(), sym_id, file_idx));
            }
        }

        Some((symbol.escaped_name.clone(), sym_id, file_idx))
    }

    fn exports_has_explicit_subpaths(exports: &serde_json::Value) -> bool {
        match exports {
            serde_json::Value::Object(map) => map.keys().any(|k| k.starts_with("./") || k == "."),
            _ => false,
        }
    }

    fn declaration_runtime_relative_path(&self, relative_path: &str) -> Option<String> {
        let relative_path = relative_path.replace('\\', "/");

        for (decl_ext, runtime_ext) in [
            (".d.ts", ".js"),
            (".d.tsx", ".jsx"),
            (".d.mts", ".mjs"),
            (".d.cts", ".cjs"),
            (".ts", ".js"),
            (".tsx", ".jsx"),
            (".mts", ".mjs"),
            (".cts", ".cjs"),
        ] {
            if let Some(prefix) = relative_path.strip_suffix(decl_ext) {
                return Some(format!("{prefix}{runtime_ext}"));
            }
        }

        Some(relative_path)
    }

    fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);
        let current_dir = current_path.parent().unwrap_or(current_path);

        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let ups = current_components.len() - common_len;
        let mut result = String::new();
        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_str()?),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        result
    }

    fn reverse_export_specifier_for_runtime_path(
        &self,
        package_root: &std::path::Path,
        runtime_relative_path: &str,
    ) -> Option<String> {
        let package_json_path = package_root.join("package.json");
        let package_json = std::fs::read_to_string(package_json_path).ok()?;
        let package_json: serde_json::Value = serde_json::from_str(&package_json).ok()?;
        let exports = package_json.get("exports")?;
        let runtime_relative_path = format!("./{}", runtime_relative_path.trim_start_matches("./"));
        self.reverse_match_exports_subpath(exports, &runtime_relative_path)
    }

    fn reverse_match_exports_subpath(
        &self,
        exports: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match exports {
            serde_json::Value::String(target) => {
                self.match_export_target(".", target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .find_map(|entry| self.reverse_match_exports_subpath(entry, runtime_path)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if let Some(specifier) =
                            self.reverse_match_export_entry(key, value, runtime_path)
                        {
                            return Some(specifier);
                        }
                        continue;
                    }

                    if let Some(specifier) = self.reverse_match_exports_subpath(value, runtime_path)
                    {
                        return Some(specifier);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn reverse_match_export_entry(
        &self,
        subpath_key: &str,
        value: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match value {
            serde_json::Value::String(target) => {
                self.match_export_target(subpath_key, target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries.iter().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            serde_json::Value::Object(map) => map.values().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            _ => None,
        }
    }

    fn match_export_target(
        &self,
        subpath_key: &str,
        target: &str,
        runtime_path: &str,
    ) -> Option<String> {
        let target = target.trim();
        let runtime_path = runtime_path.trim();

        if target.contains('*') {
            let wildcard = self.match_export_wildcard(target, runtime_path)?;
            return Some(self.apply_export_wildcard(subpath_key, &wildcard));
        }

        if target.ends_with('/') && subpath_key.ends_with('/') {
            let remainder = runtime_path.strip_prefix(target)?;
            return Some(format!(
                "{}{}",
                subpath_key.trim_start_matches("./"),
                remainder
            ));
        }

        if target != runtime_path {
            return None;
        }

        if subpath_key == "." {
            return Some(String::new());
        }

        Some(subpath_key.trim_start_matches("./").to_string())
    }

    fn match_export_wildcard(&self, pattern: &str, value: &str) -> Option<String> {
        let star_idx = pattern.find('*')?;
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some(middle.to_string())
    }

    fn apply_export_wildcard(&self, pattern: &str, wildcard: &str) -> String {
        pattern
            .replace('*', wildcard)
            .trim_start_matches("./")
            .to_string()
    }

    fn strip_ts_extensions(&self, path: &str) -> String {
        for ext in [
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js",
            ".mjs", ".cjs",
        ] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }

        path.to_string()
    }

    pub(crate) fn get_symbol_from_any_binder(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                // O(1) fast-path via resolve_symbol_file_index
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
                self.ctx
                    .all_binders
                    .as_ref()
                    .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
            })
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.get_symbol(sym_id))
            })
    }

    pub(crate) fn local_value_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                if symbol.is_type_only {
                    return false;
                }
                // Skip symbols that came from other files via globals merge.
                // In the merged program, file_locals includes globals from all files.
                // For TS4023 "cannot be named" checks, only symbols that are actually
                // declared in or imported into the current file count as accessible.
                // A symbol from another file that ended up in globals is NOT nameable
                // in the current file's declaration emit unless it's explicitly imported.
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
                    return false;
                }
                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    pub(crate) fn module_specifier_for_file(&self, file_idx: u32) -> Option<String> {
        if let Some(specifier) = self.ctx.module_specifiers.get(&file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }
}
