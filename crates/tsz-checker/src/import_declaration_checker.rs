//! Import alias duplicate checking, import equals declaration validation,
//! namespace import resolution, and re-export chain cycle detection.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check for duplicate import alias declarations within a scope.
    ///
    /// TS2300: Emitted when multiple `import X = ...` declarations have the same name
    /// within the same scope (namespace, module, or file).
    pub(crate) fn check_import_alias_duplicates(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        // Map from import alias name to list of declaration indices
        let mut alias_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };

            // Get the import alias name from import_clause (e.g., 'M' in 'import M = Z.I')
            let Some(alias_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(alias_id) = self.ctx.arena.get_identifier(alias_node) else {
                continue;
            };
            let alias_name = alias_id.escaped_text.to_string();

            alias_map.entry(alias_name).or_default().push(stmt_idx);
        }

        // TS2300: Emit for all declarations with duplicate names
        for (alias_name, indices) in alias_map {
            if indices.len() > 1 {
                for &import_idx in &indices {
                    let Some(import_node) = self.ctx.arena.get(import_idx) else {
                        continue;
                    };
                    let Some(import_decl) = self.ctx.arena.get_import_decl(import_node) else {
                        continue;
                    };

                    // Report error on the alias name (import_clause)
                    let alias_node = import_decl.import_clause;
                    let Some(sym_id) = self.resolve_identifier_symbol(alias_node) else {
                        tracing::trace!("Could not resolve identifier symbol");
                        continue;
                    };
                    let symbol = self.ctx.binder.symbols.get(sym_id).unwrap();
                    tracing::trace!("Symbol flags: {:?}", symbol.flags);
                    if self.symbol_is_value_only(sym_id, Some(&alias_name)) {
                        self.error_value_only_type_at(&alias_name, import_decl.import_clause);
                    } else {
                        self.error_at_node(
                            import_decl.import_clause,
                            &format!("Duplicate identifier '{alias_name}'."),
                            diagnostic_codes::DUPLICATE_IDENTIFIER,
                        );
                    }
                }
            }
        }
    }

    // =========================================================================
    // Import Equals Declaration Validation
    // =========================================================================

    /// Check an import equals declaration for ESM compatibility, unresolved modules,
    /// and conflicts with local declarations.
    ///
    /// Validates `import x = require()` and `import x = Namespace` style imports:
    /// - TS1202 when import assignment is used in ES modules
    /// - TS2307 when the module cannot be found
    /// - TS2440 when import conflicts with a local declaration
    pub(crate) fn check_import_equals_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // TS1147/TS2439/TS2303 checks for import = require("...") forms.
        // Use get_require_module_specifier so both StringLiteral and recovered require-call
        // representations are handled consistently.
        let require_module_specifier = self.get_require_module_specifier(import.module_specifier);
        let mut force_module_not_found = false;
        let mut force_module_not_found_as_2307 = false;
        if require_module_specifier.is_some()
            && self.ctx.arena.get(import.module_specifier).is_some()
        {
            // This is an external module reference (require("..."))
            // Check if we're inside a MODULE_DECLARATION (namespace/module)
            let mut current = stmt_idx;
            let mut inside_namespace = false;
            let mut namespace_is_exported = false;
            let mut containing_module_name: Option<String> = None;

            while current.is_some() {
                if let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        // Check if this is an ambient module (declare module "...") or namespace
                        if let Some(module_decl) = self.ctx.arena.get_module(node)
                            && let Some(name_node) = self.ctx.arena.get(module_decl.name)
                        {
                            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                                // This is an ambient module: declare module "foo"
                                if let Some(name_literal) = self.ctx.arena.get_literal(name_node) {
                                    containing_module_name = Some(name_literal.text.clone());
                                }
                            } else {
                                // This is a namespace: namespace Foo
                                inside_namespace = true;
                                // Check if this namespace is exported
                                namespace_is_exported = self.has_export_modifier(current);
                            }
                        }
                        break;
                    }
                    // Move to parent
                    if let Some(ext) = self.ctx.arena.get_extended(current) {
                        current = ext.parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // TS1147: Only emit for namespaces (not ambient modules)
            if inside_namespace {
                self.error_at_node(
                        import.module_specifier,
                        diagnostic_messages::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                        diagnostic_codes::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                    );
                // Only return early for non-exported namespaces
                // TypeScript emits both TS1147 and TS2307 for exported namespaces
                if !namespace_is_exported {
                    return;
                }
                force_module_not_found = true;
            }

            // TS2439: Ambient modules cannot use relative imports
            if containing_module_name.is_some()
                && let Some(imported_module) = require_module_specifier.as_deref()
            {
                // Check if this is a relative import (starts with ./ or ../)
                if imported_module.starts_with("./") || imported_module.starts_with("../") {
                    self.error_at_node(
                                stmt_idx,
                                diagnostic_messages::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
                                diagnostic_codes::IMPORT_OR_EXPORT_DECLARATION_IN_AN_AMBIENT_MODULE_DECLARATION_CANNOT_REFERENCE_M,
                            );
                    // Keep TS2439 and also force TS2307 in this ambient-relative import case.
                    force_module_not_found = true;
                    force_module_not_found_as_2307 = true;
                }
            }

            // TS2303: Check for circular import aliases
            if let Some(imported_module) = require_module_specifier.as_deref() {
                let mut visited = rustc_hash::FxHashSet::default();
                if let Some(ref current) = containing_module_name {
                    visited.insert(current.clone());
                } else {
                    visited.insert(self.ctx.file_name.clone());
                }

                let mut current_module = imported_module.to_string();
                let mut has_cycle = false;

                // Max depth to prevent infinite loops in malformed graphs
                for _ in 0..100 {
                    // Try to resolve current_module to an actual file name
                    let resolved_module =
                        if let Some(target_idx) = self.ctx.resolve_import_target(&current_module) {
                            let arena = self.ctx.get_arena_for_file(target_idx as u32);
                            if let Some(source_file) = arena.source_files.first() {
                                source_file.file_name.clone()
                            } else {
                                current_module.clone()
                            }
                        } else {
                            current_module.clone()
                        };

                    if visited.contains(&resolved_module) || visited.contains(&current_module) {
                        has_cycle = true;
                        break;
                    }
                    visited.insert(resolved_module.clone());
                    visited.insert(current_module.clone());

                    let mut next_module = None;
                    if let Some(exports) = self.resolve_effective_module_exports(&current_module)
                        && let Some(export_equals_sym) = exports.get("export=")
                        && let Some(sym) = self.get_symbol_globally(export_equals_sym)
                        && (sym.flags & tsz_binder::symbol_flags::ALIAS) != 0
                        && let Some(ref import_mod) = sym.import_module
                    {
                        next_module = Some(import_mod.clone());
                    }

                    if let Some(next) = next_module {
                        current_module = next;
                    } else {
                        break;
                    }
                }

                if has_cycle {
                    // Emit TS2303: Circular definition of import alias
                    if let Some(import_name) = self
                        .ctx
                        .arena
                        .get(import.import_clause)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.clone())
                    {
                        let message = format_message(
                            diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                            &[&import_name],
                        );
                        self.error_at_node(
                            import.import_clause,
                            &message,
                            diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                        );
                        return;
                    }
                }
                self.check_export_target_is_module(import.module_specifier, imported_module);
            }
        }

        // Get the import alias name (e.g., 'a' in 'import a = M')
        let import_name = self
            .ctx
            .arena
            .get(import.import_clause)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.clone());

        // Check for TS2440: Import declaration conflicts with local declaration
        // This error is specific to ImportEqualsDeclaration (not ES6 imports).
        // It occurs when:
        // 1. The import introduces a name that already has a value declaration
        // 2. The value declaration is in the same file (local)
        //
        // Note: The binder does NOT merge import equals declarations - it creates
        // a new symbol and overwrites the scope. So we need to find ALL symbols
        // with the same name and check if any non-import has VALUE flags.
        if let Some(ref name) = import_name {
            // Get the symbol for this import
            let import_sym_id = self.ctx.binder.node_symbols.get(&stmt_idx.0).copied();
            // Find the enclosing scope of the import statement
            let import_scope = self
                .ctx
                .binder
                .find_enclosing_scope(self.ctx.arena, stmt_idx);

            // TS2440: Import declaration conflicts with local declaration.
            // The binder can merge non-mergeable declarations into the import symbol,
            // so detect conflicts directly on the import symbol's declarations first.

            let mut resolved_flags = 0;
            let mut resolved_decls = Vec::new();

            if let Some(import_decl) = self
                .ctx
                .arena
                .get_import_decl(self.ctx.arena.get(stmt_idx).unwrap())
            {
                let target_node = import_decl.module_specifier;
                let target_sym_id_opt = if let Some(node) = self.ctx.arena.get(target_node) {
                    if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                        self.resolve_identifier_symbol(target_node)
                    } else if node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
                        // For qualified names like A.B.C, we need to resolve the whole thing
                        self.resolve_identifier_symbol(target_node)
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(target_sym_id) = target_sym_id_opt {
                    let mut visited = Vec::new();
                    if let Some(resolved_id) =
                        self.resolve_alias_symbol(target_sym_id, &mut visited)
                        && let Some(resolved_sym) = self
                            .ctx
                            .binder
                            .get_symbol_with_libs(resolved_id, &self.get_lib_binders())
                    {
                        resolved_flags = resolved_sym.flags;
                        resolved_decls = resolved_sym.declarations.clone();
                    }
                }
            }

            let mut has_value = (resolved_flags & tsz_binder::symbol_flags::VALUE) != 0;
            if has_value
                && (resolved_flags & tsz_binder::symbol_flags::VALUE_MODULE) != 0
                && (resolved_flags
                    & (tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE))
                    == 0
            {
                let mut any_instantiated = false;
                for decl_idx in &resolved_decls {
                    if let Some(decl_node) = self.ctx.arena.get(*decl_idx) {
                        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                        {
                            if self.is_namespace_declaration_instantiated(*decl_idx) {
                                any_instantiated = true;
                                break;
                            }
                        } else {
                            any_instantiated = true;
                            break;
                        }
                    }
                }
                has_value = any_instantiated;
            }
            let import_has_value = has_value;

            if import_has_value
                && let Some(import_sym_id) = import_sym_id
                && let Some(import_sym) = self.ctx.binder.symbols.get(import_sym_id)
            {
                let has_merged_local_non_import_decl =
                    import_sym.declarations.iter().any(|&decl_idx| {
                        if decl_idx == stmt_idx {
                            return false;
                        }
                        let in_same_scope = if let Some(import_scope_id) = import_scope {
                            self.ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, decl_idx)
                                == Some(import_scope_id)
                        } else {
                            true
                        };
                        if !in_same_scope {
                            return false;
                        }

                        self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                            decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        })
                    });

                if has_merged_local_non_import_decl {
                    let message = format_message(
                        diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                        &[name],
                    );
                    self.error_at_node(
                        stmt_idx,
                        &message,
                        diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                    );
                    return;
                }
            }

            // Find all symbols with this name (there may be multiple due to shadowing)
            let all_symbols = self.ctx.binder.symbols.find_all_by_name(name);

            for sym_id in all_symbols {
                // Skip the import's own symbol
                if Some(sym_id) == import_sym_id {
                    continue;
                }

                if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                    // Check if this symbol has value semantics
                    let is_value = (sym.flags & symbol_flags::VALUE) != 0;
                    let _is_alias = (sym.flags & symbol_flags::ALIAS) != 0;
                    let is_namespace = (sym.flags & symbol_flags::NAMESPACE_MODULE) != 0;

                    // TS2300: duplicate `import =` aliases with the same name in the same scope.
                    // TypeScript reports this as duplicate identifier (not TS2440).
                    // Special case: If this is a namespace module, check if it's the enclosing scope
                    // itself. In TypeScript, `namespace A.M { import M = Z.M; }` is allowed - the
                    // import alias `M` shadows the namespace container name `M`.
                    if is_namespace && let Some(import_scope_id) = import_scope {
                        // Get the scope that contains the import
                        if let Some(scope) = self.ctx.binder.scopes.get(import_scope_id.0 as usize)
                        {
                            // Check if any of this namespace's declarations match the container node
                            // of the import's enclosing scope
                            let is_enclosing_namespace =
                                sym.declarations.contains(&scope.container_node);
                            if is_enclosing_namespace {
                                // This namespace is the enclosing context, not a conflicting declaration
                                continue;
                            }
                        }
                    }

                    // Only check for conflicts within the same scope.
                    // A symbol in a different namespace/module should not conflict.
                    if let Some(import_scope_id) = import_scope {
                        let decl_in_same_scope = sym.declarations.iter().any(|&decl_idx| {
                            self.ctx
                                .binder
                                .find_enclosing_scope(self.ctx.arena, decl_idx)
                                == Some(import_scope_id)
                        });
                        if !decl_in_same_scope {
                            continue;
                        }
                    }

                    // Check if this symbol has any declaration in the CURRENT file
                    // A declaration is in the current file if it's in node_symbols
                    let has_local_declaration = sym.declarations.iter().any(|&decl_idx| {
                        // The declaration is local if its node_symbols entry points to this symbol
                        self.ctx.binder.node_symbols.get(&decl_idx.0) == Some(&sym_id)
                    });

                    if import_has_value && is_value && has_local_declaration {
                        let message = format_message(
                            diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            &[name],
                        );
                        self.error_at_node(
                            stmt_idx,
                            &message,
                            diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                        );
                        return; // Don't emit further errors for this import
                    }
                }
            }
        }

        let module_specifier_idx = import.module_specifier;
        let Some(ref_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = ref_node.pos;
        let spec_length = ref_node.end.saturating_sub(ref_node.pos);

        // Handle namespace imports: import x = Namespace or import x = Namespace.Member
        // These need to emit TS2503 ("Cannot find namespace") if not found
        if require_module_specifier.is_none() {
            self.check_namespace_import(stmt_idx, module_specifier_idx);
            return;
        }

        // TS1202: Import assignment cannot be used when targeting ECMAScript modules.
        // Check if module kind is explicitly ESM (CommonJS modules support import = require)
        let is_ambient_context =
            self.ctx.file_name.ends_with(".d.ts") || self.is_ambient_declaration(stmt_idx);
        if self.ctx.compiler_options.module.is_es_module() && !is_ambient_context {
            self.error_at_node(
                stmt_idx,
                "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
                diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
            );
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(module_name) = require_module_specifier.as_deref() else {
            return;
        };

        if force_module_not_found {
            let (message, code) = self.module_not_found_diagnostic(module_name);
            let (message, code) = if force_module_not_found_as_2307 {
                (
                    crate::diagnostics::format_message(
                        crate::diagnostics::diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                        &[module_name],
                    ),
                    crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                )
            } else {
                (message, code)
            };
            self.ctx
                .push_diagnostic(crate::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    spec_start,
                    spec_length,
                    message,
                    code,
                ));
            return;
        }

        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            return;
        }

        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return;
        }

        if self.ctx.binder.declared_modules.contains(module_name) {
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            {
                let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                error_code = fallback_code;
                error_message = fallback_message;
            }
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            return;
        }

        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        let module_key = module_name.to_string();
        if self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            return;
        }

        // Use TS2792 when module resolution is "classic" (system/amd/umd modules),
        // suggesting the user switch to nodenext or configure paths.
        let (message, code) = self.module_not_found_diagnostic(module_name);
        self.ctx.modules_with_ts2307_emitted.insert(module_key);
        self.error_at_position(spec_start, spec_length, &message, code);
    }

    // =========================================================================
    // Namespace Import Validation (TS2503)
    // =========================================================================

    /// Check a namespace import (import x = Namespace or import x = Namespace.Member).
    /// Emits TS2503 "Cannot find namespace" if the namespace cannot be resolved.
    /// Emits TS2708 "Cannot use namespace as a value" if exporting a type-only member.
    fn check_namespace_import(&mut self, _stmt_idx: NodeIndex, module_ref: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(ref_node) = self.ctx.arena.get(module_ref) else {
            return;
        };

        // Handle simple identifier: import x = Namespace
        if ref_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(ref_node) {
                let name = &ident.escaped_text;
                // Skip if identifier is empty (parse error created a placeholder)
                // or if it's a reserved word that should be handled by TS1359
                if name.is_empty() || name == "null" {
                    return;
                }
                // Try to resolve the identifier as a namespace/module
                if self.resolve_identifier_symbol(module_ref).is_none() {
                    self.error_at_node_msg(
                        module_ref,
                        diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        &[name],
                    );
                }
            }
            return;
        }

        // Handle qualified name: import x = Namespace.Member
        if ref_node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.ctx.arena.get_qualified_name(ref_node)
        {
            // Check the leftmost part first - this is what determines TS2503 vs TS2694
            let left_name = self.get_leftmost_identifier_name(qn.left);
            if let Some(name) = left_name {
                // Try to resolve the left identifier
                let left_resolved = self.resolve_leftmost_qualified_name(qn.left);
                if left_resolved.is_none() {
                    self.error_at_node_msg(
                        qn.left,
                        diagnostic_codes::CANNOT_FIND_NAMESPACE,
                        &[&name],
                    );
                    return; // Don't check for TS2694 if left doesn't exist
                }

                // If left is resolved, check if right member exists (TS2694)
                // Use the existing report_type_query_missing_member which handles this correctly
                self.report_type_query_missing_member(module_ref);

                // Note: TS2708 for namespace-as-value is NOT emitted here for import equals
                // declarations. `export import X = NS.TypeMember` is valid even when the
                // member is type-only. TS2708 is emitted at usage sites (property access,
                // call expressions, extends clauses) when a namespace is used as a value.
            }
        }
    }

    /// Get the leftmost identifier name from a node (handles nested `QualifiedNames`).
    fn get_leftmost_identifier_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            return Some(ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.get_leftmost_identifier_name(qn.left);
        }
        None
    }

    /// Resolve the leftmost identifier in a potentially nested `QualifiedName`.
    fn resolve_leftmost_qualified_name(&self, idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.resolve_leftmost_qualified_name(qn.left);
        }
        None
    }

    // =========================================================================
    // Import Declaration Validation
    // =========================================================================

    /// TS1214: Check import binding names for strict-mode reserved words.
    /// Import declarations make the file a module (always strict mode), so TS1214 applies.
    fn check_import_binding_reserved_words(&mut self, import_clause_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use crate::state_checking::is_strict_mode_reserved_name;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(import_clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        // Check default import name: `import package from "./mod"`
        if clause.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(clause.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && is_strict_mode_reserved_name(&ident.escaped_text)
        {
            let message = format_message(
                            diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                            &[&ident.escaped_text],
                        );
            self.error_at_node(
                            clause.name,
                            &message,
                            diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                        );
        }

        // Check named bindings (namespace import or named imports)
        if clause.named_bindings.is_none() {
            return;
        }
        let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
            return;
        };

        if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
            // `import * as package from "./mod"` — check the alias name
            if let Some(ns_data) = self.ctx.arena.get_named_imports(bindings_node)
                && ns_data.name.is_some()
                && let Some(name_node) = self.ctx.arena.get(ns_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && is_strict_mode_reserved_name(&ident.escaped_text)
            {
                let message = format_message(
                                    diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                                    &[&ident.escaped_text],
                                );
                self.error_at_node(
                                    ns_data.name,
                                    &message,
                                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                                );
            }
        } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS {
            // `import { foo as package } from "./mod"` — check each specifier's local name
            if let Some(named_data) = self.ctx.arena.get_named_imports(bindings_node) {
                let elements: Vec<_> = named_data.elements.nodes.to_vec();
                for elem_idx in elements {
                    let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(spec) = self.ctx.arena.get_specifier(elem_node) else {
                        continue;
                    };
                    // The local binding name is `spec.name`
                    let name_to_check = spec.name;
                    if let Some(name_node) = self.ctx.arena.get(name_to_check)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && is_strict_mode_reserved_name(&ident.escaped_text)
                    {
                        let message = format_message(
                                    diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                                    &[&ident.escaped_text],
                                );
                        self.error_at_node(
                                    name_to_check,
                                    &message,
                                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
                                );
                    }
                }
            }
        }
    }

    /// TS2823: Check that import attributes are only used with supported module options.
    pub(crate) fn check_import_attributes_module_option(&mut self, attributes_idx: NodeIndex) {
        use tsz_common::common::ModuleKind;

        if attributes_idx.is_none() {
            return;
        }

        let supported = matches!(
            self.ctx.compiler_options.module,
            ModuleKind::ESNext | ModuleKind::Node16 | ModuleKind::NodeNext | ModuleKind::Preserve
        );

        if !supported && let Some(attr_node) = self.ctx.arena.get(attributes_idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_position(
                attr_node.pos,
                attr_node.end.saturating_sub(attr_node.pos),
                diagnostic_messages::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
                diagnostic_codes::IMPORT_ATTRIBUTES_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_OPTION_IS_SET_TO_ESNEXT_NOD,
            );
        }
    }

    /// Check an import declaration for unresolved modules and missing exports.
    pub(crate) fn check_import_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(import) = self.ctx.arena.get_import_decl(node) else {
            return;
        };

        // TS2823: Import attributes require specific module options
        self.check_import_attributes_module_option(import.attributes);

        // TS1214/TS1212: Check import binding names for strict mode reserved words.
        // Import declarations make the file a module, so it's always strict mode → TS1214.
        self.check_import_binding_reserved_words(import.import_clause);

        if import.import_clause.is_some() {
            self.check_import_declaration_conflicts(stmt_idx, import.import_clause);
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        // Extract module specifier data eagerly to avoid borrow issues later
        let module_specifier_idx = import.module_specifier;
        let import_clause_idx = import.import_clause;

        let Some(spec_node) = self.ctx.arena.get(module_specifier_idx) else {
            return;
        };
        let spec_start = spec_node.pos;
        let spec_length = spec_node.end.saturating_sub(spec_node.pos);

        let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
            return;
        };

        let module_name = &literal.text;
        let has_import_clause = self.ctx.arena.get(import_clause_idx).is_some();
        let is_side_effect_import = !has_import_clause;
        if is_side_effect_import && !self.ctx.compiler_options.no_unchecked_side_effect_imports {
            return;
        }
        let is_type_only_import = self
            .ctx
            .arena
            .get(import_clause_idx)
            .and_then(|clause_node| self.ctx.arena.get_import_clause(clause_node))
            .is_some_and(|clause| clause.is_type_only);
        let mut emitted_dts_import_error = false;
        if module_name.ends_with(".d.ts") && !is_type_only_import {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let suggested = module_name.trim_end_matches(".d.ts");
            let message = format_message(
                diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                &[suggested],
            );
            self.error_at_position(
                spec_start,
                spec_length,
                &message,
                diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
            );
            emitted_dts_import_error = true;
        }

        if let Some(binders) = &self.ctx.all_binders
            && binders.iter().any(|binder| {
                binder.declared_modules.contains(module_name)
                    || binder.shorthand_ambient_modules.contains(module_name)
            })
        {
            tracing::trace!(%module_name, "check_import_declaration: found in declared/shorthand modules, returning");
            return;
        }

        if self.would_create_cycle(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: cycle detected");
            let cycle_path: Vec<&str> = self
                .ctx
                .import_resolution_stack
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name.as_str()))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!("Circular import detected: {cycle_str}");

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_position(
                    spec_start,
                    spec_length,
                    &message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        self.ctx.import_resolution_stack.push(module_name.clone());

        // Check ambient modules BEFORE resolution errors.
        // `declare module "x"` in .d.ts files should suppress TS2307 even when
        // file-based resolution fails (matching check_import_equals_declaration).
        if self.is_ambient_module_match(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: ambient module match, returning");
            self.ctx.import_resolution_stack.pop();
            return;
        }

        // Check for specific resolution error from driver (TS2834, TS2835, TS2792, etc.)
        // This must be checked before resolved_modules to catch extensionless import errors
        let module_key = module_name.to_string();
        if let Some(error) = self.ctx.get_resolution_error(module_name) {
            // Extract error values before mutable borrow
            let mut error_code = error.code;
            let mut error_message = error.message.clone();
            if error_code
                == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || error_code == crate::diagnostics::diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
            {
                // Side-effect imports use TS2882 instead of TS2307/TS2792
                if is_side_effect_import {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                    error_code = diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF;
                    error_message = format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                        &[module_name],
                    );
                } else {
                    let (fallback_message, fallback_code) = self.module_not_found_diagnostic(module_name);
                    error_code = fallback_code;
                    error_message = fallback_message;
                }
            }
            tracing::trace!(%module_name, error_code, "check_import_declaration: resolution error found");
            // Check if we've already emitted an error for this module (prevents duplicate emissions)
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx
                    .modules_with_ts2307_emitted
                    .insert(module_key.clone());
                self.error_at_position(spec_start, spec_length, &error_message, error_code);
            }
            if error_code
                != crate::diagnostics::diagnostic_codes::MODULE_WAS_RESOLVED_TO_BUT_JSX_IS_NOT_SET
            {
                self.ctx.import_resolution_stack.pop();
                return;
            }
        }

        // Check if module was successfully resolved
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
                let mut skip_export_checks = false;
                // Extract data we need before any mutable borrows
                let (is_declaration_file_flag, file_info) = {
                    let arena = self.ctx.get_arena_for_file(target_idx as u32);
                    if let Some(source_file) = arena.source_files.first() {
                        let file_name = source_file.file_name.as_str();
                        let is_js_like = file_name.ends_with(".js")
                            || file_name.ends_with(".jsx")
                            || file_name.ends_with(".mjs")
                            || file_name.ends_with(".cjs");
                        let skip_exports = is_js_like && !source_file.is_declaration_file;
                        let target_is_esm =
                            file_name.ends_with(".mjs") || file_name.ends_with(".mts");
                        let is_dts = source_file.is_declaration_file;
                        (is_dts, Some((skip_exports, target_is_esm)))
                    } else {
                        (false, None)
                    }
                };

                if let Some((should_skip_exports, target_is_esm)) = file_info {
                    if should_skip_exports {
                        skip_export_checks = true;
                    }

                    // TS1479: Check if CommonJS file is importing an ES module
                    // This error occurs when the current file will emit require() calls
                    // but the target file is an ES module (which cannot be required)
                    let current_is_commonjs = {
                        let current_file = &self.ctx.file_name;
                        // .cts files are always CommonJS
                        let is_commonjs_file = current_file.ends_with(".cts");
                        // .mts files are always ESM
                        let is_esm_file = current_file.ends_with(".mts");
                        // For other files, check if module system will emit require() calls
                        is_commonjs_file
                            || (!is_esm_file && !self.ctx.compiler_options.module.is_es_module())
                    };

                    if current_is_commonjs && target_is_esm && !is_type_only_import {
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                            &[module_name],
                        );
                        self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::THE_CURRENT_FILE_IS_A_COMMONJS_MODULE_WHOSE_IMPORTS_WILL_PRODUCE_REQUIRE_CALLS_H,
                        );
                    }
                }

                if is_declaration_file_flag && !is_type_only_import && !emitted_dts_import_error {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let suggested = if module_name.ends_with(".d.ts") {
                        module_name.trim_end_matches(".d.ts")
                    } else {
                        module_name.as_str()
                    };
                    let message = format_message(
                            diagnostic_messages::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                            &[suggested],
                        );
                    self.error_at_position(
                            spec_start,
                            spec_length,
                            &message,
                            diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT,
                        );
                }
                if let Some(binder) = self.ctx.get_binder_for_file(target_idx) {
                    let normalized_module_name = module_name.trim_matches('"').trim_matches('\'');
                    if !binder.is_external_module
                        && !self.is_ambient_module_match(module_name)
                        && !binder.declared_modules.contains(normalized_module_name)
                    {
                        let arena = self.ctx.get_arena_for_file(target_idx as u32);
                        if let Some(source_file) = arena.source_files.first()
                            && !source_file.is_declaration_file
                        {
                            let file_name = source_file.file_name.as_str();
                            let is_js_like = file_name.ends_with(".js")
                                || file_name.ends_with(".jsx")
                                || file_name.ends_with(".mjs")
                                || file_name.ends_with(".cjs");
                            if !is_js_like {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::FILE_IS_NOT_A_MODULE,
                                    &[&source_file.file_name],
                                );
                                self.error_at_position(
                                    spec_start,
                                    spec_length,
                                    &message,
                                    diagnostic_codes::FILE_IS_NOT_A_MODULE,
                                );
                                self.ctx.import_resolution_stack.pop();
                                return;
                            }
                        }
                    }
                }
                if !skip_export_checks {
                    self.check_imported_members(import, module_name);
                }
            } else {
                self.check_imported_members(import, module_name);
            }

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        if self.ctx.binder.module_exports.contains_key(module_name) {
            tracing::trace!(%module_name, "check_import_declaration: found in module_exports, checking members");
            self.check_imported_members(import, module_name);

            if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
                let mut visited = FxHashSet::default();
                for source_module in source_modules {
                    self.check_reexport_chain_for_cycles(source_module, &mut visited);
                }
            }

            self.ctx.import_resolution_stack.pop();
            return;
        }

        tracing::trace!(%module_name, "check_import_declaration: fallback - emitting module-not-found error");
        // Fallback: Emit module-not-found error if no specific error was found
        // Check if we've already emitted for this module (prevents duplicate emissions)
        if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
            self.ctx.modules_with_ts2307_emitted.insert(module_key);
            // Side-effect imports (bare `import "module"`) use TS2882 instead of TS2307
            let (message, code) = if is_side_effect_import {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                (
                    format_message(
                        diagnostic_messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                        &[module_name],
                    ),
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF,
                )
            } else {
                self.module_not_found_diagnostic(module_name)
            };
            // Use pre-extracted position instead of error_at_node to avoid
            // silent failures when get_node_span returns None
            self.error_at_position(spec_start, spec_length, &message, code);
        }

        self.ctx.import_resolution_stack.pop();
    }

    // =========================================================================
    // Re-export Cycle Detection
    // =========================================================================

    /// Check re-export chains for circular dependencies.
    pub(crate) fn check_reexport_chain_for_cycles(
        &mut self,
        module_name: &str,
        visited: &mut FxHashSet<String>,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if visited.contains(module_name) {
            let cycle_path: Vec<&str> = visited
                .iter()
                .map(std::string::String::as_str)
                .chain(std::iter::once(module_name))
                .collect();
            let cycle_str = cycle_path.join(" -> ");
            let message = format!(
                "{}: {}",
                diagnostic_messages::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                cycle_str
            );

            // Check if we've already emitted TS2307 for this module (prevents duplicate emissions)
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error(
                    0,
                    0,
                    message,
                    diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS,
                );
            }
            return;
        }

        visited.insert(module_name.to_string());

        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_name) {
            for source_module in source_modules {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        if let Some(reexports) = self.ctx.binder.reexports.get(module_name) {
            for (source_module, _) in reexports.values() {
                self.check_reexport_chain_for_cycles(source_module, visited);
            }
        }

        visited.remove(module_name);
    }

    /// Check if adding a module to the resolution path would create a cycle.
    pub(crate) fn would_create_cycle(&self, module: &str) -> bool {
        self.ctx
            .import_resolution_stack
            .contains(&module.to_string())
    }

    // =========================================================================
    // Re-export Resolution Helpers
    // =========================================================================

    /// Try to resolve an import through the target module's binder re-export chains.
    /// Traverses across binder boundaries by resolving each re-export source
    /// to its target file and checking that file's binder.
    pub(crate) fn resolve_import_via_target_binder(
        &self,
        module_name: &str,
        import_name: &str,
    ) -> bool {
        if let Some(target_idx) = self.ctx.resolve_import_target(module_name) {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_in_file(target_idx, import_name, &mut visited);
        }
        false
    }

    /// Try to resolve an import by searching all binders' re-export chains.
    pub(crate) fn resolve_import_via_all_binders(
        &self,
        module_name: &str,
        normalized: &str,
        import_name: &str,
    ) -> bool {
        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .resolve_import_if_needed_public(module_name, import_name)
                    .is_some()
                    || binder
                        .resolve_import_if_needed_public(normalized, import_name)
                        .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Resolve an import by checking a specific file's exports and following
    /// re-export chains across binder boundaries. Each file has its own binder
    /// in multi-file mode, so we traverse wildcard/named re-exports by resolving
    /// each source specifier to its target file and checking that file's binder.
    fn resolve_import_in_file(
        &self,
        file_idx: usize,
        import_name: &str,
        visited: &mut rustc_hash::FxHashSet<usize>,
    ) -> bool {
        if !visited.insert(file_idx) {
            return false; // Cycle detection
        }

        let Some(target_binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };

        let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(target_file_name) = target_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
        else {
            return false;
        };

        // Check direct exports
        if let Some(exports) = target_binder.module_exports.get(&target_file_name)
            && exports.has(import_name)
        {
            return true;
        }

        // Check named re-exports
        if let Some(reexports) = target_binder.reexports.get(&target_file_name)
            && let Some((source_module, original_name)) = reexports.get(import_name)
        {
            let name = original_name.as_deref().unwrap_or(import_name);
            if let Some(source_idx) = self
                .ctx
                .resolve_import_target_from_file(file_idx, source_module)
                && self.resolve_import_in_file(source_idx, name, visited)
            {
                return true;
            }
        }

        // Check wildcard re-exports
        if let Some(source_modules) = target_binder.wildcard_reexports.get(&target_file_name) {
            let source_modules = source_modules.clone();
            for source_module in &source_modules {
                if let Some(source_idx) = self
                    .ctx
                    .resolve_import_target_from_file(file_idx, source_module)
                    && self.resolve_import_in_file(source_idx, import_name, visited)
                {
                    return true;
                }
            }
        }

        false
    }

    fn check_import_declaration_conflicts(&mut self, stmt_idx: NodeIndex, clause_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        let mut bindings_to_check = Vec::new();

        if clause.name.is_some() {
            bindings_to_check.push((clause_idx, clause.name));
        }

        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
        {
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                {
                    bindings_to_check.push((clause.named_bindings, ns.name));
                }
            } else if bindings_node.kind == syntax_kind_ext::NAMED_IMPORTS
                && let Some(named) = self.ctx.arena.get_named_imports(bindings_node)
            {
                for &spec_idx in &named.elements.nodes {
                    if let Some(spec_node) = self.ctx.arena.get(spec_idx)
                        && let Some(spec) = self.ctx.arena.get_specifier(spec_node)
                    {
                        let name_idx = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if name_idx.is_some() {
                            bindings_to_check.push((spec_idx, name_idx));
                        }
                    }
                }
            }
        }

        for (binding_node_idx, name_idx) in bindings_to_check {
            if let Some(name_node) = self.ctx.arena.get(name_idx)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let sym_id_opt = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&binding_node_idx.0)
                    .copied();
                if let Some(sym_id) = sym_id_opt {
                    let mut has_conflict = false;
                    if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                        if sym.is_type_only {
                            continue;
                        }

                        let mut import_has_value = false;
                        let mut visited = Vec::new();
                        if let Some(resolved_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                            && let Some(resolved_sym) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(resolved_id, &self.get_lib_binders())
                        {
                            let mut has_value = (resolved_sym.flags & symbol_flags::VALUE) != 0;
                            if has_value
                                && (resolved_sym.flags & symbol_flags::VALUE_MODULE) != 0
                                && (resolved_sym.flags
                                    & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE))
                                    == 0
                            {
                                let mut any_instantiated = false;
                                for &decl_idx in &resolved_sym.declarations {
                                    if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                        if decl_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
                                                        if self.is_namespace_declaration_instantiated(decl_idx) {
                                                            any_instantiated = true;
                                                            break;
                                                        }
                                                    } else {
                                                        any_instantiated = true;
                                                        break;
                                                    }
                                    }
                                }
                                has_value = any_instantiated;
                            }
                            import_has_value = has_value;
                            if (resolved_sym.flags & symbol_flags::ALIAS) != 0
                                && sym.import_module.is_some()
                                && sym.import_name.is_none()
                            {
                                import_has_value = true;
                            }
                        }
                        if !import_has_value {
                            continue;
                        }

                        let import_scope = self
                            .ctx
                            .binder
                            .find_enclosing_scope(self.ctx.arena, binding_node_idx);

                        has_conflict = sym.declarations.iter().any(|&decl_idx| {
                            if decl_idx == binding_node_idx
                                || decl_idx == clause_idx
                                || decl_idx == stmt_idx
                            {
                                return false;
                            }

                            let in_same_scope = if let Some(import_scope_id) = import_scope {
                                self.ctx
                                    .binder
                                    .find_enclosing_scope(self.ctx.arena, decl_idx)
                                    == Some(import_scope_id)
                            } else {
                                true
                            };
                            if !in_same_scope {
                                return false;
                            }

                            if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                                !matches!(
                                    decl_node.kind,
                                    syntax_kind_ext::IMPORT_CLAUSE
                                        | syntax_kind_ext::NAMESPACE_IMPORT
                                        | syntax_kind_ext::IMPORT_SPECIFIER
                                        | syntax_kind_ext::NAMED_IMPORTS
                                        | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                                        | syntax_kind_ext::IMPORT_DECLARATION
                                )
                            } else {
                                false
                            }
                        });
                    }

                    if has_conflict {
                        let message = format_message(
                                diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                                &[&name],
                            );
                        self.error_at_node(
                                name_idx,
                                &message,
                                diagnostic_codes::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            );
                    }
                }
            }
        }
    }
}
