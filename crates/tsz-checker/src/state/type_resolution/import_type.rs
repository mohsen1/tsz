//! Import type resolution helpers (`import("./module").Foo`).

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn resolve_ts_import_type_member_symbol(
        &self,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let from_file = self.ctx.current_file_idx;

        if let Some(source_binder) = self.ctx.get_binder_for_file(from_file)
            && let Some((sym_id, _)) =
                source_binder.resolve_import_with_reexports_type_only(module_specifier, member_name)
        {
            if let Some(target_idx) = self
                .ctx
                .resolve_import_target_from_file(from_file, module_specifier)
            {
                self.ctx.register_symbol_file_target(sym_id, target_idx);
            }
            return Some(sym_id);
        }

        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(from_file, module_specifier)
            .or_else(|| self.ctx.resolve_import_target(module_specifier))?;

        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();

        let record_and_return = |sym_id: tsz_binder::SymbolId| -> Option<tsz_binder::SymbolId> {
            self.ctx
                .register_symbol_file_target(sym_id, target_file_idx);
            Some(sym_id)
        };

        if let Some((sym_id, _)) =
            target_binder.resolve_import_with_reexports_type_only(&target_file_name, member_name)
        {
            return record_and_return(sym_id);
        }

        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && let Some(sym_id) = exports.get(member_name)
            && target_binder.get_symbol(sym_id).is_some()
        {
            return record_and_return(sym_id);
        }

        if let Some(exports) = target_binder.module_exports.get(module_specifier)
            && let Some(sym_id) = exports.get(member_name)
            && target_binder.get_symbol(sym_id).is_some()
        {
            return record_and_return(sym_id);
        }

        if let Some(sym_id) = target_binder.file_locals.get(member_name)
            && let Some(symbol) = target_binder.get_symbol(sym_id)
        {
            let pure_type_flags = tsz_binder::symbol_flags::TYPE_ALIAS
                | tsz_binder::symbol_flags::INTERFACE
                | tsz_binder::symbol_flags::TYPE_PARAMETER;
            let is_pure_type = symbol.is_type_only
                || ((symbol.flags & pure_type_flags) != 0
                    && (symbol.flags & tsz_binder::symbol_flags::VALUE) == 0);
            if is_pure_type {
                return record_and_return(sym_id);
            }
        }

        if let Some(sym_id) = self.resolve_jsdoc_import_member(module_specifier, member_name)
            && let Some(symbol) = self
                .get_cross_file_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
        {
            let pure_type_flags = tsz_binder::symbol_flags::TYPE_ALIAS
                | tsz_binder::symbol_flags::INTERFACE
                | tsz_binder::symbol_flags::TYPE_PARAMETER;
            let is_pure_type = symbol.is_type_only
                || ((symbol.flags & pure_type_flags) != 0
                    && (symbol.flags & tsz_binder::symbol_flags::VALUE) == 0);
            if is_pure_type {
                return Some(sym_id);
            }
        }

        None
    }

    fn import_type_namespace_name(&self, module_specifier: &str) -> String {
        let stripped = module_specifier
            .strip_prefix("./")
            .or_else(|| module_specifier.strip_prefix("../"))
            .unwrap_or(module_specifier);
        let display_name = stripped
            .strip_suffix(".d.ts")
            .or_else(|| stripped.strip_suffix(".d.tsx"))
            .or_else(|| stripped.strip_suffix(".d.mts"))
            .or_else(|| stripped.strip_suffix(".d.cts"))
            .or_else(|| stripped.strip_suffix(".ts"))
            .or_else(|| stripped.strip_suffix(".tsx"))
            .or_else(|| stripped.strip_suffix(".mts"))
            .or_else(|| stripped.strip_suffix(".cts"))
            .or_else(|| stripped.strip_suffix(".js"))
            .or_else(|| stripped.strip_suffix(".jsx"))
            .or_else(|| stripped.strip_suffix(".mjs"))
            .or_else(|| stripped.strip_suffix(".cjs"))
            .unwrap_or(stripped);
        format!("\"{display_name}\".export=")
    }

    fn import_type_missing_member_node(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return idx;
            };
            if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                return current;
            }
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return current;
            };
            let Some(left_node) = self.ctx.arena.get(qn.left) else {
                return qn.right;
            };
            if left_node.kind == syntax_kind_ext::CALL_EXPRESSION {
                return qn.right;
            }
            current = qn.left;
        }
    }

    fn import_type_member_segments(&self, idx: NodeIndex) -> Option<Vec<String>> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == syntax_kind_ext::CALL_EXPRESSION {
            let call = self.ctx.arena.get_call_expr(node)?;
            let expr_node = self.ctx.arena.get(call.expression)?;
            return (expr_node.kind == SyntaxKind::ImportKeyword as u16).then(Vec::new);
        }
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qn = self.ctx.arena.get_qualified_name(node)?;
        let mut segments = self.import_type_member_segments(qn.left)?;
        let right_node = self.ctx.arena.get(qn.right)?;
        let right_ident = self.ctx.arena.get_identifier(right_node)?;
        segments.push(right_ident.escaped_text.clone());
        Some(segments)
    }

    fn resolve_import_type_reference(
        &mut self,
        module_name: &str,
        type_name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let segments = self.import_type_member_segments(type_name_idx)?;
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1
            && let Some(jsdoc_typedef_type) =
                self.resolve_import_type_jsdoc_typedef(module_name, &segments[0])
        {
            return Some(jsdoc_typedef_type);
        }

        let mut current_sym =
            self.resolve_ts_import_type_member_symbol(module_name, &segments[0])?;
        for segment in segments.iter().skip(1) {
            let symbol = self
                .get_cross_file_symbol(current_sym)
                .or_else(|| self.ctx.binder.get_symbol(current_sym))?;
            current_sym = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        let symbol_flags = self
            .get_cross_file_symbol(current_sym)
            .or_else(|| self.ctx.binder.get_symbol(current_sym))
            .map(|symbol| symbol.flags)?;

        let resolved = if (symbol_flags
            & (symbol_flags::TYPE_ALIAS
                | symbol_flags::CLASS
                | symbol_flags::INTERFACE
                | symbol_flags::ENUM
                | symbol_flags::TYPE_PARAMETER))
            != 0
        {
            self.type_reference_symbol_type(current_sym)
        } else if (symbol_flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
            != 0
        {
            self.get_type_of_symbol(current_sym)
        } else {
            TypeId::ERROR
        };
        (resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN).then_some(resolved)
    }

    fn resolve_import_type_jsdoc_typedef(
        &mut self,
        module_name: &str,
        typedef_name: &str,
    ) -> Option<TypeId> {
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
            .or_else(|| self.ctx.resolve_import_target(module_name))?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?.clone();

        for source_file in &target_arena.source_files {
            let comments = source_file.comments.clone();
            let source_text = source_file.text.to_string();
            let mut checker = Box::new(CheckerState::with_parent_cache(
                &target_arena,
                &target_binder,
                self.ctx.types,
                source_file.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self,
            ));
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            checker.ctx.current_file_idx = target_file_idx;
            self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

            if let Some((ty, _)) =
                checker.resolve_jsdoc_typedef_info(typedef_name, &comments, &source_text)
                && ty != TypeId::ERROR
                && ty != TypeId::UNKNOWN
            {
                self.ctx.merge_symbol_file_targets_from(&checker.ctx);
                return Some(ty);
            }
        }

        None
    }

    /// Format a generic type name with its type parameter names for TS2314 messages.
    /// e.g., "Foo" + [T, U] → "Foo<T, U>"
    pub(crate) fn format_generic_display_name_with_interner(
        name: &str,
        type_params: &[tsz_solver::TypeParamInfo],
        types: &dyn tsz_solver::QueryDatabase,
    ) -> String {
        if type_params.is_empty() {
            return name.to_string();
        }
        let param_names: Vec<String> = type_params
            .iter()
            .map(|p| types.resolve_atom(p.name))
            .collect();
        format!("{}<{}>", name, param_names.join(", "))
    }

    /// Walk the left chain of nested qualified names to find a root import call.
    /// For `import("./m").A.B`, the AST is:
    ///   QualifiedName(left: QualifiedName(left: CallExpr(import("./m")), right: A), right: B)
    /// Returns the `CALL_EXPRESSION` `NodeIndex` if the leftmost node is an `import()` call.
    pub(crate) fn find_leftmost_import_call(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        const MAX_DEPTH: usize = 64;
        for _ in 0..MAX_DEPTH {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = self.ctx.arena.get_qualified_name(node)?;
                idx = qn.left;
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Check if it's import(...)
                let call = self.ctx.arena.get_call_expr(node)?;
                let expr_node = self.ctx.arena.get(call.expression)?;
                if expr_node.kind == SyntaxKind::ImportKeyword as u16 {
                    return Some(idx);
                }
                return None;
            } else {
                return None;
            }
        }
        None
    }

    /// Extract the module specifier string from an `import()` call expression.
    pub(crate) fn get_import_type_module_specifier(
        &self,
        call_idx: NodeIndex,
    ) -> Option<(String, NodeIndex)> {
        let node = self.ctx.arena.get(call_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let args = call.arguments.as_ref()?;
        let &first_arg = args.nodes.first()?;
        let arg_node = self.ctx.arena.get(first_arg)?;
        let literal = self.ctx.arena.get_literal(arg_node)?;
        Some((literal.text.clone(), first_arg))
    }

    /// Check an import type expression for module resolution and emit TS2307 if needed.
    /// Returns the resolved type or `TypeId::ERROR`.
    pub(crate) fn check_import_type_and_resolve(
        &mut self,
        call_idx: NodeIndex,
        type_name_idx: NodeIndex,
        _type_ref_idx: NodeIndex,
    ) -> TypeId {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        // TS2880: Check for deprecated `assert` keyword in import type options
        self.check_import_type_deprecated_assert(call_idx);

        let Some((module_name, specifier_node)) = self.get_import_type_module_specifier(call_idx)
        else {
            return TypeId::ERROR;
        };
        let is_bare_import_type = call_idx == type_name_idx;
        let has_import_type_options = self.ctx.arena.get(call_idx).is_some_and(|call_node| {
            self.ctx
                .arena
                .get_call_expr(call_node)
                .and_then(|call| call.arguments.as_ref())
                .is_some_and(|args| args.nodes.len() > 1)
        });
        let has_call_parse_diagnostic = self.ctx.arena.get(call_idx).is_some_and(|call_node| {
            self.ctx
                .diagnostics
                .iter()
                .any(|diag| diag.start >= call_node.pos && diag.start < call_node.end)
        });
        let suppress_bare_import_type_error = has_call_parse_diagnostic || has_import_type_options;
        let bare_import_type_refers_to_type = if is_bare_import_type {
            use tsz_binder::symbol_flags;

            const PURE_TYPE: u32 = symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS;
            const VALUE: u32 = symbol_flags::VARIABLE
                | symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::ENUM
                | symbol_flags::ENUM_MEMBER
                | symbol_flags::VALUE_MODULE;

            let lib_binders = self.get_lib_binders();
            let ambient_export_equals_sym = self
                .ctx
                .binder
                .module_exports
                .get(&module_name)
                .and_then(|exports| exports.get("export="))
                .or_else(|| {
                    self.ctx
                        .global_module_exports_index
                        .as_ref()
                        .and_then(|idx| idx.get(&module_name))
                        .and_then(|inner| inner.get("export="))
                        .and_then(|entries| entries.first().map(|&(_file_idx, sym_id)| sym_id))
                });
            let file_export_equals = self
                .ctx
                .resolve_import_target(&module_name)
                .and_then(|target_idx| {
                    self.ctx
                        .get_binder_for_file(target_idx)
                        .map(|binder| (target_idx, binder))
                })
                .and_then(|(target_idx, binder)| {
                    let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                    let file_name = target_arena.source_files.first()?.file_name.as_str();
                    binder
                        .module_exports
                        .get(file_name)
                        .and_then(|exports| exports.get("export="))
                });
            let has_export_equals =
                ambient_export_equals_sym.is_some() || file_export_equals.is_some();

            has_export_equals
                || self.is_module_export_equals_type_only(&module_name)
                || ambient_export_equals_sym.is_some_and(|sym_id| {
                    let symbol_is_type = |checker: &Self, sym_id: tsz_binder::SymbolId| {
                        checker
                            .ctx
                            .binder
                            .get_symbol_with_libs(sym_id, &lib_binders)
                            .is_some_and(|sym| {
                                sym.is_type_only
                                    || ((sym.flags & PURE_TYPE) != 0 && (sym.flags & VALUE) == 0)
                            })
                    };

                    if symbol_is_type(self, sym_id) {
                        return true;
                    }

                    let mut visited = Vec::new();
                    self.resolve_alias_symbol(sym_id, &mut visited)
                        .is_some_and(|resolved| symbol_is_type(self, resolved))
                })
        } else {
            false
        };
        let bare_import_type_error = |checker: &mut Self| {
            let message = format_message(
                diagnostic_messages::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
                &[&module_name],
            );
            checker.error_at_node(
                type_name_idx,
                &message,
                diagnostic_codes::MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_I,
            );
            TypeId::ERROR
        };
        let report_unresolved_imports = self.ctx.report_unresolved_imports;
        let member_segments = self.import_type_member_segments(type_name_idx);

        if let Some(resolved) = self.resolve_import_type_reference(&module_name, type_name_idx) {
            return resolved;
        }

        // Check if the module resolves through any of the known resolution paths.
        // Import type specifiers may not have been collected by the CLI driver's
        // module specifier scanner (which only scans import/export declarations),
        // so resolved_modules may not have an entry. We check multiple sources:

        // 1. Driver-resolved modules (from import/export declarations)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(&module_name)
        {
            if let Some(member_segments) = member_segments.as_ref()
                && let Some(first_segment) = member_segments.first()
            {
                let namespace_name = self.import_type_namespace_name(&module_name);
                let member_idx = self.import_type_missing_member_node(type_name_idx);
                self.error_namespace_no_export(&namespace_name, first_segment, member_idx);
                return TypeId::ERROR;
            }
            return if is_bare_import_type
                && !bare_import_type_refers_to_type
                && !suppress_bare_import_type_error
            {
                bare_import_type_error(self)
            } else {
                TypeId::ERROR
            };
        }

        // 2. Binder module_exports (cross-file)
        if self.ctx.binder.module_exports.contains_key(&module_name) {
            if let Some(member_segments) = member_segments.as_ref()
                && let Some(first_segment) = member_segments.first()
            {
                let namespace_name = self.import_type_namespace_name(&module_name);
                let member_idx = self.import_type_missing_member_node(type_name_idx);
                self.error_namespace_no_export(&namespace_name, first_segment, member_idx);
                return TypeId::ERROR;
            }
            return if is_bare_import_type
                && !bare_import_type_refers_to_type
                && !suppress_bare_import_type_error
            {
                bare_import_type_error(self)
            } else {
                TypeId::ERROR
            };
        }

        // 3. Shorthand ambient modules (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(&module_name)
        {
            return if is_bare_import_type
                && !bare_import_type_refers_to_type
                && !suppress_bare_import_type_error
            {
                bare_import_type_error(self)
            } else {
                TypeId::ERROR
            };
        }

        // 4. Declared modules (ambient modules with body)
        if self.ctx.binder.declared_modules.contains(&module_name) {
            if let Some(member_segments) = member_segments.as_ref()
                && let Some(first_segment) = member_segments.first()
            {
                let namespace_name = self.import_type_namespace_name(&module_name);
                let member_idx = self.import_type_missing_member_node(type_name_idx);
                self.error_namespace_no_export(&namespace_name, first_segment, member_idx);
                return TypeId::ERROR;
            }
            return if is_bare_import_type
                && !bare_import_type_refers_to_type
                && !suppress_bare_import_type_error
            {
                bare_import_type_error(self)
            } else {
                TypeId::ERROR
            };
        }

        // 5. Check if the driver has a resolution error for this specifier
        //    (positive evidence of failed resolution)
        if let Some(error) = self.ctx.get_resolution_error(&module_name) {
            // For Node.js built-in modules, use TS2591 instead of TS2307
            // (tsc emits "Cannot find name 'X'. Install @types/node" for these)
            if report_unresolved_imports {
                let (error_message, error_code) = {
                    let (msg, code) = self.module_not_found_diagnostic(&module_name);
                    if code != error.code {
                        (msg, code) // module_not_found_diagnostic upgraded to TS2591
                    } else {
                        (error.message.clone(), error.code)
                    }
                };
                let module_key = module_name.to_string();
                if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                    self.ctx.modules_with_ts2307_emitted.insert(module_key);
                    self.error_at_node(specifier_node, &error_message, error_code);
                }
            }
            return TypeId::ERROR;
        }

        // 6. For non-relative specifiers (no ./ or ../ prefix), if not found in
        //    declared/ambient modules, emit TS2307. Non-relative specifiers target
        //    packages or ambient modules — the binder has complete information.
        let is_relative = module_name.starts_with("./") || module_name.starts_with("../");
        if !is_relative {
            if report_unresolved_imports {
                let module_key = module_name.to_string();
                if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                    self.ctx.modules_with_ts2307_emitted.insert(module_key);
                    let (message, code) = self.module_not_found_diagnostic(&module_name);
                    self.error_at_node(specifier_node, &message, code);
                }
            }
            return TypeId::ERROR;
        }

        // 7. For relative specifiers without resolution data, we can't determine
        //    if the module exists (import type specifiers aren't collected by the
        //    driver's module scanner). Check resolved_module_paths for cross-file
        //    resolution.
        if let Some(ref paths) = self.ctx.resolved_module_paths {
            // If there's no entry for this (file_idx, specifier), the specifier
            // was never resolved. Check if any project file matches.
            let key = (self.ctx.current_file_idx, module_name.clone());
            if paths.contains_key(&key) {
                if let Some(member_segments) = member_segments.as_ref()
                    && let Some(first_segment) = member_segments.first()
                {
                    let namespace_name = self.import_type_namespace_name(&module_name);
                    let member_idx = self.import_type_missing_member_node(type_name_idx);
                    self.error_namespace_no_export(&namespace_name, first_segment, member_idx);
                    return TypeId::ERROR;
                }
                return if is_bare_import_type
                    && !bare_import_type_refers_to_type
                    && !suppress_bare_import_type_error
                {
                    bare_import_type_error(self)
                } else {
                    TypeId::ERROR
                };
            }
        }

        // Relative specifier with no resolution data — we can't confirm it doesn't
        // exist. Return ERROR without emitting TS2307 to avoid false positives.
        TypeId::ERROR
    }

    /// TS2880: Check for deprecated `assert` keyword in import type options.
    ///
    /// For `import("./module", { assert: { ... } })` type expressions, the second
    /// argument is an options object literal. If it contains an `assert` property,
    /// emit TS2880 at the attributes value position (matching tsc's `ImportAttributes`
    /// node position for import type nodes).
    fn check_import_type_deprecated_assert(&mut self, call_idx: NodeIndex) {
        // Only emit if deprecation is not suppressed
        if self
            .ctx
            .capabilities
            .check_import_assert_deprecated()
            .is_none()
        {
            return;
        }

        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return;
        };
        let Some(call_data) = self.ctx.arena.get_call_expr(call_node) else {
            return;
        };
        let args = match call_data.arguments.as_ref() {
            Some(a) => a.nodes.as_slice(),
            None => &[],
        };
        if args.len() < 2 {
            return;
        }

        let options_idx = args[1];
        let Some(options_node) = self.ctx.arena.get(options_idx) else {
            return;
        };
        if options_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }

        let children = self.ctx.arena.get_children(options_idx);
        for child_idx in children {
            let Some(child_node) = self.ctx.arena.get(child_idx) else {
                continue;
            };
            if child_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.ctx.arena.get_property_assignment(child_node) else {
                continue;
            };
            let Some(name) = self.get_property_name(prop.name) else {
                continue;
            };
            if name == "assert" {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                // For import type expressions, tsc positions TS2880 at the attributes
                // value object (the inner `{ ... }`) to match the ImportAttributes
                // node position in tsc's AST.
                let Some(val_node) = self.ctx.arena.get(prop.initializer) else {
                    continue;
                };
                self.error_at_position(
                    val_node.pos,
                    6, // length of "assert"
                    diagnostic_messages::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
                    diagnostic_codes::IMPORT_ASSERTIONS_HAVE_BEEN_REPLACED_BY_IMPORT_ATTRIBUTES_USE_WITH_INSTEAD_OF_AS,
                );
            }
        }
    }
}
