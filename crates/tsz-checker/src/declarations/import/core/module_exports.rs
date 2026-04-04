//! Module body validation and export assignment checking.

use crate::diagnostics::format_message;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Module Body Validation
    // =========================================================================

    /// Check a module body for statements and function implementations.
    pub(crate) fn check_module_body(&mut self, body_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        tracing::trace!(
            "check_module_body: body_kind={} MODULE_BLOCK={}",
            body_node.kind,
            syntax_kind_ext::MODULE_BLOCK
        );

        let mut is_ambient_external_module = false;
        if let Some(ext) = self.ctx.arena.get_extended(body_idx) {
            let parent_idx = ext.parent;
            if parent_idx.is_some()
                && let Some(parent_node) = self.ctx.arena.get(parent_idx)
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
            {
                is_ambient_external_module = true;
            }
        }

        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                let is_ambient_body = self.ctx.is_ambient_declaration(body_idx);
                for &stmt_idx in &statements.nodes {
                    // TS1063: export assignment cannot be used in a namespace.
                    // Emit the error and skip further checking of the statement
                    // (tsc does not resolve the expression when it's invalid).
                    let is_export_assign = self
                        .ctx
                        .arena
                        .get(stmt_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::EXPORT_ASSIGNMENT);
                    if is_export_assign && !is_ambient_external_module {
                        self.error_at_node(
                            stmt_idx,
                            diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
                            diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
                        );
                        continue;
                    }
                    if is_ambient_body
                        && let Some(stmt_node) = self.ctx.arena.get(stmt_idx)
                        && !stmt_node.is_declaration()
                        && stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT
                    {
                        continue;
                    }
                    self.check_statement(stmt_idx);
                }
                self.check_function_implementations(&statements.nodes);
                // Check for duplicate export assignments (TS2300) and conflicts (TS2309)
                // Filter out export assignments in namespace bodies since they're already
                // flagged with TS1063 and shouldn't trigger TS2304/TS2309 follow-up errors.
                // However, they ARE checked in ambient external modules.
                let non_export_assign: Vec<NodeIndex> = if is_ambient_external_module {
                    statements.nodes.clone()
                } else {
                    statements
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_none_or(|n| n.kind != syntax_kind_ext::EXPORT_ASSIGNMENT)
                        })
                        .collect()
                };
                self.check_export_assignment(&non_export_assign);
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            self.check_statement(body_idx);
        }
    }

    // =========================================================================
    // Export Assignment Validation
    // =========================================================================

    /// Check for export assignment conflicts with other exported elements.
    ///
    /// Validates that:
    /// - `export = X` is not used when there are also other exported elements (TS2309)
    /// - There are not multiple `export = X` statements (TS2300)
    pub(crate) fn check_export_assignment(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let mut export_assignment_indices: Vec<NodeIndex> = Vec::new();
        let mut export_default_indices: Vec<NodeIndex> = Vec::new();
        let mut has_other_exports = false;

        // Check if we're in a declaration file (implicitly ambient)
        let is_declaration_file = self
            .ctx
            .arena
            .source_files
            .first()
            .is_some_and(|sf| sf.is_declaration_file)
            || self.ctx.file_name.contains(".d.");

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            match node.kind {
                syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    export_assignment_indices.push(stmt_idx);

                    if let Some(export_data) = self.ctx.arena.get_export_assignment(node) {
                        // TS1294: erasableSyntaxOnly — export = is not erasable.
                        // Exception: `export = x` is allowed in .cts/.cjs files because
                        // it's the standard CJS export syntax and compiles to `module.exports = x`.
                        let is_cts_file = self.ctx.file_name.ends_with(".cts")
                            || self.ctx.file_name.ends_with(".cjs");
                        if export_data.is_export_equals
                            && self.ctx.compiler_options.erasable_syntax_only
                            && !self.ctx.is_ambient_declaration(stmt_idx)
                            && !is_cts_file
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                diagnostic_messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED
                                    .to_string(),
                                diagnostic_codes::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED,
                            );
                        }

                        // TS1282/TS1283: VMS checks for export = <type>
                        if export_data.is_export_equals
                            && self.ctx.compiler_options.verbatim_module_syntax
                            && !is_declaration_file
                        {
                            self.check_vms_export_equals(export_data.expression);
                        }

                        // TS2714: In ambient context, export assignment expression must be
                        // an identifier or qualified name. This check applies to both
                        // `export = <expr>` and `export default <expr>` in ambient contexts.
                        let is_ambient =
                            is_declaration_file || self.is_ambient_declaration(stmt_idx);
                        if is_ambient
                            && !self.is_identifier_or_qualified_name(export_data.expression)
                        {
                            // Only emit TS2714 when the expression is NOT an identifier
                            // or qualified name. Valid forms like `export = X` or
                            // `export default Y` (where X/Y are identifiers) should not
                            // trigger this error.
                            self.error_at_node(
                                export_data.expression,
                                "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                            );
                        } else if export_data.is_export_equals
                            && let Some(ident) = self
                                .ctx
                                .arena
                                .get(export_data.expression)
                                .and_then(|node| self.ctx.arena.get_identifier(node))
                            && let Some(report_node) = self
                                .global_augmentation_namespace_export_cycle_report_node(
                                    statements,
                                    ident.escaped_text.as_str(),
                                )
                        {
                            self.error_at_node(
                                report_node,
                                &format_message(
                                    diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                                    &[ident.escaped_text.as_str()],
                                ),
                                diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                            );
                        } else if let Some(expected_type) =
                            self.jsdoc_type_annotation_for_node(stmt_idx)
                        {
                            let request =
                                crate::context::TypingRequest::with_contextual_type(expected_type);
                            let actual_type = self
                                .get_type_of_node_with_request(export_data.expression, &request);
                            self.check_assignable_or_report(
                                actual_type,
                                expected_type,
                                export_data.expression,
                            );
                            if let Some(expr_node) = self.ctx.arena.get(export_data.expression)
                                && expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            {
                                self.check_object_literal_excess_properties(
                                    actual_type,
                                    expected_type,
                                    export_data.expression,
                                );
                            }
                        } else {
                            self.get_type_of_node(export_data.expression);
                        }
                    }
                }
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_data) = self.ctx.arena.get_export_decl(node) {
                        let has_named_default_export =
                            self.export_decl_has_named_default_export(stmt_idx);
                        if export_data.is_default_export || has_named_default_export {
                            export_default_indices.push(stmt_idx);

                            // TS2714: In ambient context, export default expression must be
                            // an identifier or qualified name. Skip for declarations
                            // (class, function, interface, enum) which are always valid.
                            // Only applies to `export default <expr>`, NOT to
                            // `export { x as default }` re-exports.
                            if export_data.is_default_export {
                                let is_ambient =
                                    is_declaration_file || self.is_ambient_declaration(stmt_idx);
                                let is_declaration =
                                    self.ctx.arena.get(export_data.export_clause).is_some_and(
                                        |n| {
                                            matches!(
                                                n.kind,
                                                k if k == syntax_kind_ext::CLASS_DECLARATION
                                                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                                                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                                                    || k == syntax_kind_ext::ENUM_DECLARATION
                                                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                            )
                                        },
                                    );
                                if is_ambient
                                    && !is_declaration
                                    && export_data.export_clause.is_some()
                                    && !self
                                        .is_identifier_or_qualified_name(export_data.export_clause)
                                {
                                    self.error_at_node(
                                        export_data.export_clause,
                                        "The expression of an export assignment must be an identifier or qualified name in an ambient context.",
                                        diagnostic_codes::THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_I,
                                    );
                                }
                            }
                        } else {
                            has_other_exports = true;
                        }
                    } else {
                        has_other_exports = true;
                    }
                }
                _ => {
                    if self.has_export_modifier(stmt_idx) {
                        has_other_exports = true;
                    }
                }
            }
        }

        // TS1203: Check for export assignment when targeting ES modules
        // This must be checked first before TS2300/TS2309
        // Declaration files (.d.ts, .d.mts, .d.cts) are exempt: they describe
        // the shape of CJS modules and `export = X` is valid in declarations.
        // Ambient module declarations (`declare module "M" { export = X; }`) are
        // also exempt — they describe external module shapes.
        // JS files (.js, .jsx, .mjs, .cjs) are exempt — they get TS8003 instead.
        // CJS-extension files (.cts) are explicitly CommonJS — export= is valid.
        let is_cjs_extension = self.ctx.file_name.ends_with(".cts");

        let is_system_module = matches!(
            self.ctx.compiler_options.module,
            tsz_common::common::ModuleKind::System
        );
        let is_es_module = self.ctx.compiler_options.module.is_es_module();
        // `module: preserve` allows both CJS (`export =`) and ESM (`export default`)
        // syntax — it preserves the module format as-written. TS1203 should not fire.
        let is_preserve = matches!(
            self.ctx.compiler_options.module,
            tsz_common::common::ModuleKind::Preserve
        );
        // For node module modes (node16/node18/node20/nodenext), the module format
        // is per-file: .mts → ESM, .cts → CJS, .ts → depends on nearest package.json
        // "type" field. Use `file_is_esm` from the driver to determine this.
        let is_node_esm_file =
            self.ctx.compiler_options.module.is_node_module() && self.ctx.file_is_esm == Some(true);

        let mut emitted_ts1203 = false;
        if (is_es_module || is_system_module || is_node_esm_file)
            && !is_preserve
            && !is_declaration_file
            && !self.is_js_file()
            && !is_cjs_extension
            && !self.ctx.has_syntax_parse_errors
        {
            for &export_idx in &export_assignment_indices {
                if !self.is_ambient_declaration(export_idx) {
                    emitted_ts1203 = true;
                    if is_system_module {
                        self.error_at_node(
                            export_idx,
                            "Export assignment is not supported when '--module' flag is 'system'.",
                            diagnostic_codes::EXPORT_ASSIGNMENT_IS_NOT_SUPPORTED_WHEN_MODULE_FLAG_IS_SYSTEM,
                        );
                    } else {
                        self.error_at_node(
                            export_idx,
                            "Export assignment cannot be used when targeting ECMAScript modules. Consider using 'export default' or another module format instead.",
                            diagnostic_codes::EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
                        );
                    }
                }
            }
        }

        // TS2300: Check for duplicate export assignments
        // TypeScript emits TS2300 on ALL export assignments if there are 2+
        // tsc points the error at the expression (e.g., `x` in `export = x;`),
        // not at the `export` keyword.
        // Skip in ambient declarations - they describe external module shapes, not
        // actual conflicting runtime exports.
        if export_assignment_indices.len() > 1 {
            for &export_idx in &export_assignment_indices {
                // Skip ambient declarations
                if self.is_ambient_declaration(export_idx) {
                    continue;
                }
                let error_node = self
                    .ctx
                    .arena
                    .get(export_idx)
                    .and_then(|node| self.ctx.arena.get_export_assignment(node))
                    .map(|data| data.expression)
                    .filter(|idx| idx.is_some())
                    .unwrap_or(export_idx);
                self.error_at_node(
                    error_node,
                    "Duplicate identifier 'export='.",
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }

        // TS2309: Check for export assignment with other exports
        // Skip in `preserve` mode — it allows mixing CJS (`export =`) and ESM syntax.
        // When TS1203 already flags `export =` as invalid, tsc suppresses TS2309 for
        // ESNext/Node module modes. For ES2015 targets, tsc emits both TS1203 and TS2309.
        let suppress_ts2309 = emitted_ts1203
            && !matches!(
                self.ctx.compiler_options.module,
                tsz_common::common::ModuleKind::ES2015
            );
        if let Some(&export_idx) = export_assignment_indices.first()
            && has_other_exports
            && export_assignment_indices.len() == 1
            && !is_preserve
            && !suppress_ts2309
        {
            self.check_export_assignment_target_member_duplicates(statements, export_idx);
            self.error_at_node(
                export_idx,
                "An export assignment cannot be used in a module with other exported elements.",
                diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_MODULE_WITH_OTHER_EXPORTED_ELEMENTS,
            );
        }

        // TS2528: Check for multiple default exports
        // tsc allows declaration merging of default exports:
        // - Interface + value (function/class) can coexist
        // - Function overloads (multiple `export default function foo(...)`) are one symbol
        // Only emit TS2528 when there are truly conflicting default exports.
        // Special case: function + class default exports emit TS2323 + TS2813 + TS2814 instead.
        let bridged_default_exports =
            self.default_export_interface_merge_bridge_indices(&export_default_indices);
        let effective_default_indices: Vec<NodeIndex> = export_default_indices
            .iter()
            .copied()
            .filter(|idx| !bridged_default_exports.contains(idx))
            .collect();

        if effective_default_indices.len() > 1 {
            // Classify each default export
            let mut has_interface = false;
            let mut has_class = false;
            let mut has_function = false;
            let mut has_named_default_export = false;
            let mut value_count = 0;
            let mut function_value_count = 0;
            let mut function_name: Option<String> = None;

            for &export_idx in &effective_default_indices {
                if self.export_decl_has_named_default_export(export_idx) {
                    has_named_default_export = true;
                }
                let wrapped_kind = self
                    .ctx
                    .arena
                    .get_export_decl_at(export_idx)
                    .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                    .map(|n| n.kind);

                match wrapped_kind {
                    Some(k) if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                        has_interface = true;
                    }
                    Some(k) if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        has_function = true;
                        // Check if all function defaults share the same name (overloads)
                        let name = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .and_then(|n| self.ctx.arena.get_function(n))
                            .map(|f| self.node_text(f.name).unwrap_or_default());
                        match (&function_name, name) {
                            (None, Some(n)) if !n.is_empty() => {
                                function_name = Some(n);
                                value_count += 1;
                                function_value_count += 1;
                            }
                            (Some(existing), Some(n)) if !n.is_empty() && *existing == n => {
                                // Same non-empty function name: overload, don't count again
                            }
                            _ => {
                                value_count += 1;
                                function_value_count += 1;
                            }
                        }
                    }
                    Some(k) if k == syntax_kind_ext::CLASS_DECLARATION => {
                        has_class = true;
                        value_count += 1;
                    }
                    _ => {
                        value_count += 1;
                    }
                }
            }

            // Emit TS2528 for any multiple default exports that are not
            // function overloads. tsc allows interface + value (function/class)
            // to coexist as a declaration merge, so interface+function is NOT a
            // conflict. However, interface + type re-export IS a conflict.
            // The merge is only valid when: (1) one default is an interface
            // declaration, AND (2) the other is a function or class (value).
            let interface_can_merge =
                has_interface && (has_function || has_class) && value_count == 1;
            let is_conflict =
                value_count > 1 || (effective_default_indices.len() > 1 && !interface_can_merge);
            if is_conflict {
                if has_function && has_class {
                    // When function + class both export as default, tsc emits
                    // TS2323 + TS2813 + TS2814 (merge conflict diagnostics).
                    self.emit_function_class_default_merge_errors(&effective_default_indices);
                } else if has_class && value_count > 1 && {
                    // tsc emits TS2323 only when a named variable reference
                    // (export default foo) accompanies a class, not for anonymous
                    // expressions (export default {...}). multipleExportDefault3/4
                    // have class + object literal and expect TS2528.
                    // Also, if the identifier refers to a type-only binding (e.g.,
                    // a type alias), tsc uses TS2528 instead of TS2323.
                    effective_default_indices.iter().any(|&idx| {
                        self.ctx
                            .arena
                            .get_export_decl_at(idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .is_some_and(|c| {
                                if c.kind != SyntaxKind::Identifier as u16 {
                                    return false;
                                }
                                // Check if identifier refers to a value (not type-only).
                                // tsc uses TS2528 for type-only default exports (e.g.,
                                // `type Bar = {}; export default Bar`).
                                let ed = self.ctx.arena.get_export_decl_at(idx);
                                let clause_idx = ed.map(|ed| ed.export_clause).unwrap_or(idx);
                                if let Some(name) = self.node_text(clause_idx)
                                    && let Some(sym_id) =
                                        self.resolve_name_at_node(&name, clause_idx)
                                    && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                                {
                                    // Only treat as TS2323 if the symbol has value flags
                                    return sym.has_any_flags(symbol_flags::VALUE);
                                }
                                // If we can't resolve, treat as value (conservative)
                                true
                            })
                    })
                } {
                    // Classify each default export as value or type-only.
                    // tsc emits TS2323 for value exports (identifiers resolving to
                    // values, or class/interface declarations). When there are also
                    // type-only exports (e.g., type aliases), tsc additionally emits
                    // TS2528 for ALL default exports. When ALL exports are
                    // values/classes/interfaces, only TS2323 is emitted (no TS2528).
                    let mut has_type_only_export = false;
                    let mut per_export: Vec<(NodeIndex, NodeIndex, bool, bool, bool)> = Vec::new();

                    for &export_idx in &effective_default_indices {
                        let default_anchor = self.get_default_export_anchor(export_idx);
                        let clause_idx = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .map(|ed| ed.export_clause)
                            .unwrap_or(NodeIndex::NONE);
                        let clause_kind = self.ctx.arena.get(clause_idx).map(|c| c.kind);
                        let is_ident = clause_kind == Some(SyntaxKind::Identifier as u16);
                        let is_class_decl = clause_kind == Some(syntax_kind_ext::CLASS_DECLARATION);
                        let is_interface_decl =
                            clause_kind == Some(syntax_kind_ext::INTERFACE_DECLARATION);

                        let is_type_only_ident = is_ident
                            && self
                                .resolve_identifier_symbol(clause_idx)
                                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                .is_some_and(|sym| {
                                    use tsz_binder::symbols::symbol_flags;
                                    let value_flags = symbol_flags::FUNCTION
                                        | symbol_flags::VARIABLE
                                        | symbol_flags::CLASS
                                        | symbol_flags::ENUM
                                        | symbol_flags::ENUM_MEMBER;
                                    (sym.flags & value_flags) == 0
                                        && (sym.flags & symbol_flags::TYPE) != 0
                                });

                        if is_type_only_ident {
                            has_type_only_export = true;
                        }

                        let is_value =
                            (is_ident && !is_type_only_ident) || is_class_decl || is_interface_decl;
                        per_export.push((
                            export_idx,
                            default_anchor,
                            is_ident,
                            is_value,
                            is_class_decl,
                        ));
                    }

                    for &(export_idx, default_anchor, is_ident, is_value, _is_class_decl) in
                        &per_export
                    {
                        // TS2323 for value exports (identifiers resolving to values,
                        // class declarations, interface declarations).
                        if is_value {
                            if is_ident {
                                self.error_at_node(
                                    export_idx,
                                    "Cannot redeclare exported variable 'default'.",
                                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                                );
                            } else {
                                self.error_at_default_export_anchor(
                                    export_idx,
                                    "Cannot redeclare exported variable 'default'.",
                                    diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                                );
                            }
                        }
                        // TS2528 only when type-only exports are present in the mix.
                        if has_type_only_export {
                            self.error_at_node(
                                default_anchor,
                                diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                                diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            );
                        }
                    }
                } else if has_interface
                    && has_function
                    && !has_class
                    && value_count == function_value_count
                {
                    // Interface + function default exports (all values are functions):
                    // TS2323 for all declarations. Note: a single function + interface
                    // is allowed (declaration merging), but that case is excluded by
                    // is_conflict requiring value_count > 1.
                    // When additional non-function value exports exist (e.g., identifier
                    // references to classes), tsc uses TS2528 instead, so we fall through
                    // to the else.
                    for &export_idx in &effective_default_indices {
                        self.error_at_default_export_anchor(
                            export_idx,
                            "Cannot redeclare exported variable 'default'.",
                            diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                        );
                    }
                } else if has_named_default_export
                    && effective_default_indices.iter().any(|&idx| {
                        let Some(clause_idx) = self
                            .ctx
                            .arena
                            .get_export_decl_at(idx)
                            .map(|ed| ed.export_clause)
                        else {
                            return false;
                        };
                        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                            return false;
                        };
                        match clause_node.kind {
                            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                || k == syntax_kind_ext::CLASS_DECLARATION
                                || k == syntax_kind_ext::INTERFACE_DECLARATION =>
                            {
                                true
                            }
                            k if k == SyntaxKind::Identifier as u16 => self
                                .resolve_identifier_symbol(clause_idx)
                                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                .is_some_and(|sym| sym.has_any_flags(symbol_flags::VALUE)),
                            _ => false,
                        }
                    })
                {
                    for &export_idx in &effective_default_indices {
                        let anchor = self.get_default_export_anchor(export_idx);
                        let clause_idx = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .map(|ed| ed.export_clause)
                            .unwrap_or(NodeIndex::NONE);
                        let is_value_default =
                            self.ctx.arena.get(clause_idx).is_some_and(|clause_node| {
                                match clause_node.kind {
                                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                        || k == syntax_kind_ext::CLASS_DECLARATION
                                        || k == syntax_kind_ext::INTERFACE_DECLARATION =>
                                    {
                                        true
                                    }
                                    k if k == SyntaxKind::Identifier as u16 => self
                                        .resolve_identifier_symbol(clause_idx)
                                        .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                        .is_some_and(|sym| sym.has_any_flags(symbol_flags::VALUE)),
                                    _ if self.export_decl_has_direct_named_default_export(
                                        export_idx,
                                    ) =>
                                    {
                                        true
                                    }
                                    _ => false,
                                }
                            });
                        if is_value_default {
                            self.error_at_default_export_anchor(
                                export_idx,
                                "Cannot redeclare exported variable 'default'.",
                                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                            );
                        }
                        self.error_at_node(
                            anchor,
                            diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        );
                    }
                } else {
                    // Fallback: TS2528 "A module cannot have multiple default exports"
                    // tsc skips interface declarations when emitting TS2528 (interfaces
                    // merge with values and don't count as conflicting defaults).
                    for &export_idx in &effective_default_indices {
                        let is_interface = self
                            .ctx
                            .arena
                            .get_export_decl_at(export_idx)
                            .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                            .is_some_and(|n| n.kind == syntax_kind_ext::INTERFACE_DECLARATION);
                        if is_interface {
                            continue;
                        }
                        let anchor = self.get_default_export_anchor(export_idx);
                        self.error_at_node(
                            anchor,
                            diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                            diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        );
                    }
                }

                // TS2393: Duplicate function implementation.
                // When multiple `export default function` declarations have bodies,
                // tsc emits TS2393 on each, regardless of whether they are named or anonymous.
                if has_function {
                    let func_impls: Vec<NodeIndex> = effective_default_indices
                        .iter()
                        .filter_map(|&idx| {
                            let ed = self.ctx.arena.get_export_decl_at(idx)?;
                            let clause_node = self.ctx.arena.get(ed.export_clause)?;
                            if clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                                return None;
                            }
                            let func = self.ctx.arena.get_function(clause_node)?;
                            if func.body.is_some() { Some(idx) } else { None }
                        })
                        .collect();

                    if func_impls.len() > 1 {
                        for &impl_idx in &func_impls {
                            self.error_at_node(
                                impl_idx,
                                diagnostic_messages::DUPLICATE_FUNCTION_IMPLEMENTATION,
                                diagnostic_codes::DUPLICATE_FUNCTION_IMPLEMENTATION,
                            );
                        }
                    }
                }
            } else if has_interface && !(has_function && value_count == 1) {
                // Multiple default exports with at least one interface but not a valid
                // interface + function merge. E.g.:
                //   export default interface A {}
                //   export default B;  // B is an interface
                // TSC reports TS2528 because these can't merge.
                for &export_idx in &effective_default_indices {
                    let anchor = self.get_default_export_anchor(export_idx);
                    self.error_at_node(
                        anchor,
                        diagnostic_messages::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                        diagnostic_codes::A_MODULE_CANNOT_HAVE_MULTIPLE_DEFAULT_EXPORTS,
                    );
                }
            }
        }
    }

    /// Get the best anchor node for a default export diagnostic (declaration name or the
    /// export statement itself).
    fn get_default_export_anchor(&self, export_idx: NodeIndex) -> NodeIndex {
        self.ctx
            .arena
            .get_export_decl_at(export_idx)
            .and_then(|ed| {
                let clause = self.ctx.arena.get(ed.export_clause)?;
                if clause.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                    self.ctx.arena.get_function(clause).and_then(|f| {
                        let n = self.ctx.arena.get(f.name)?;
                        if n.kind == SyntaxKind::Identifier as u16 {
                            Some(f.name)
                        } else {
                            None
                        }
                    })
                } else if clause.kind == syntax_kind_ext::CLASS_DECLARATION {
                    self.ctx.arena.get_class(clause).and_then(|c| {
                        let n = self.ctx.arena.get(c.name)?;
                        if n.kind == SyntaxKind::Identifier as u16 {
                            Some(c.name)
                        } else {
                            None
                        }
                    })
                } else if clause.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                    self.ctx.arena.get_interface(clause).and_then(|i| {
                        let n = self.ctx.arena.get(i.name)?;
                        if n.kind == SyntaxKind::Identifier as u16 {
                            Some(i.name)
                        } else {
                            None
                        }
                    })
                } else if let Some(named_exports) = self.ctx.arena.get_named_imports(clause) {
                    named_exports
                        .elements
                        .nodes
                        .iter()
                        .find_map(|&specifier_idx| {
                            let specifier_node = self.ctx.arena.get(specifier_idx)?;
                            let specifier = self.ctx.arena.get_specifier(specifier_node)?;
                            if specifier.is_type_only {
                                return None;
                            }
                            let exported_name =
                                self.get_identifier_text_from_idx(specifier.name)?;
                            (exported_name == "default").then_some(specifier.name)
                        })
                } else if clause.kind == SyntaxKind::Identifier as u16 {
                    Some(ed.export_clause)
                } else {
                    None
                }
            })
            .unwrap_or(export_idx)
    }

    fn error_at_default_export_anchor(&mut self, export_idx: NodeIndex, message: &str, code: u32) {
        let anchor = self.get_default_export_anchor(export_idx);
        self.error_at_node(anchor, message, code);
    }

    fn export_decl_has_named_default_export(&self, export_idx: NodeIndex) -> bool {
        let Some(clause_idx) = self
            .ctx
            .arena
            .get_export_decl_at(export_idx)
            .map(|ed| ed.export_clause)
        else {
            return false;
        };
        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return false;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return false;
        };

        named_exports.elements.nodes.iter().any(|&specifier_idx| {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                return false;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                return false;
            };
            !specifier.is_type_only
                && self
                    .get_identifier_text_from_idx(specifier.name)
                    .is_some_and(|name| name == "default")
        })
    }

    fn export_decl_has_direct_named_default_export(&self, export_idx: NodeIndex) -> bool {
        let Some(clause_idx) = self
            .ctx
            .arena
            .get_export_decl_at(export_idx)
            .map(|ed| ed.export_clause)
        else {
            return false;
        };
        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return false;
        };
        let Some(named_exports) = self.ctx.arena.get_named_imports(clause_node) else {
            return false;
        };

        named_exports.elements.nodes.iter().any(|&specifier_idx| {
            let Some(specifier_node) = self.ctx.arena.get(specifier_idx) else {
                return false;
            };
            let Some(specifier) = self.ctx.arena.get_specifier(specifier_node) else {
                return false;
            };
            if specifier.is_type_only
                || self
                    .get_identifier_text_from_idx(specifier.name)
                    .is_none_or(|name| name != "default")
            {
                return false;
            }

            specifier.property_name.is_none()
                || self
                    .get_identifier_text_from_idx(specifier.property_name)
                    .is_some_and(|name| name == "default")
        })
    }

    fn default_export_interface_merge_bridge_indices(
        &mut self,
        export_default_indices: &[NodeIndex],
    ) -> FxHashSet<NodeIndex> {
        let interface_default_names: FxHashSet<String> = export_default_indices
            .iter()
            .filter_map(|&export_idx| {
                let clause_idx = self.ctx.arena.get_export_decl_at(export_idx)?.export_clause;
                let clause = self.ctx.arena.get(clause_idx)?;
                if clause.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                    return None;
                }
                let interface_decl = self.ctx.arena.get_interface(clause)?;
                self.get_identifier_text_from_idx(interface_decl.name)
            })
            .collect();

        if interface_default_names.is_empty() {
            return FxHashSet::default();
        }

        export_default_indices
            .iter()
            .filter_map(|&export_idx| {
                let clause_idx = self.ctx.arena.get_export_decl_at(export_idx)?.export_clause;
                let clause_node = self.ctx.arena.get(clause_idx)?;
                let named_exports = self.ctx.arena.get_named_imports(clause_node)?;

                let bridges_interface_merge =
                    named_exports.elements.nodes.iter().any(|&specifier_idx| {
                        let specifier_node = match self.ctx.arena.get(specifier_idx) {
                            Some(node) => node,
                            None => return false,
                        };
                        let specifier = match self.ctx.arena.get_specifier(specifier_node) {
                            Some(specifier) if !specifier.is_type_only => specifier,
                            _ => return false,
                        };
                        let exported_name = match self.get_identifier_text_from_idx(specifier.name)
                        {
                            Some(name) if name == "default" => name,
                            _ => return false,
                        };
                        let _ = exported_name;
                        let mut candidate_name_indices = Vec::with_capacity(2);
                        if specifier.property_name.is_some() {
                            candidate_name_indices.push(specifier.property_name);
                        }
                        if specifier.name.is_some() {
                            candidate_name_indices.push(specifier.name);
                        }

                        candidate_name_indices.into_iter().any(|candidate_idx| {
                            self.get_identifier_text_from_idx(candidate_idx)
                                .is_some_and(|local_name| {
                                    local_name != "default"
                                        && interface_default_names.contains(&local_name)
                                })
                        })
                    });

                bridges_interface_merge.then_some(export_idx)
            })
            .collect()
    }

    /// Emit TS2323 + TS2813 + TS2814 for function + class default export merge conflicts.
    /// tsc treats `export default function` + `export default class` as a declaration merge
    /// conflict rather than a "multiple default exports" (TS2528) scenario.
    fn emit_function_class_default_merge_errors(&mut self, export_default_indices: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // TS2323: "Cannot redeclare exported variable 'default'." on every declaration
        for &export_idx in export_default_indices {
            let message = format_message(
                diagnostic_messages::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                &["default"],
            );
            self.error_at_default_export_anchor(
                export_idx,
                &message,
                diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
            );
        }

        // TS2813: "Class declaration cannot implement overload list for 'default'." on class
        // TS2814: "Function with bodies can only merge with classes that are ambient." on function
        for &export_idx in export_default_indices {
            let wrapped_kind = self
                .ctx
                .arena
                .get_export_decl_at(export_idx)
                .and_then(|ed| self.ctx.arena.get(ed.export_clause))
                .map(|n| n.kind);

            let anchor = self.get_default_export_anchor(export_idx);

            match wrapped_kind {
                Some(k) if k == syntax_kind_ext::CLASS_DECLARATION => {
                    let message = format_message(
                        diagnostic_messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                        &["default"],
                    );
                    self.error_at_node(
                        anchor,
                        &message,
                        diagnostic_codes::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR,
                    );
                }
                Some(k) if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.error_at_node(
                        anchor,
                        diagnostic_messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                        diagnostic_codes::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                    );
                }
                _ => {}
            }
        }
    }

    /// Check if a node is an identifier or qualified name (e.g., `X` or `X.Y.Z`).
    /// Used for TS2714 validation of export assignment expressions in ambient contexts.
    fn is_identifier_or_qualified_name(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        node.kind == SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::QUALIFIED_NAME
            || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
    }
}
