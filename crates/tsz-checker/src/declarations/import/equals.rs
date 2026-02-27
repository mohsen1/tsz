//! Import-equals declaration validation: `import X = require("y")` and
//! `import X = Namespace.Member` forms, plus import alias duplicate checking.

use crate::state::CheckerState;
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

        // In JS files, `import x = require(...)` is TS-only syntax (TS8002).
        // tsc skips semantic analysis for such statements, so we should too.
        if self.ctx.is_js_file() {
            return;
        }

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

        // TS1392: An import alias cannot use 'import type'.
        // Only applies to namespace alias forms like `import type Foo = ns.Foo`.
        // `import type X = require("...")` is valid since TS 3.8.
        if import.is_type_only && require_module_specifier.is_none() {
            self.error_at_node(
                stmt_idx,
                "An import alias cannot use 'import type'",
                diagnostic_codes::AN_IMPORT_ALIAS_CANNOT_USE_IMPORT_TYPE,
            );
        }
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

            let mut resolved_flags = 0u32;
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
                        // For qualified names like A.B.C, we need to resolve the whole chain
                        self.resolve_qualified_symbol(target_node)
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

            // TS2440: Check if the import binding name conflicts with a local declaration.
            // tsc only reports TS2440 when the import target has value semantics
            // (i.e., it's an instantiated module/namespace). Non-instantiated namespaces
            // (empty or type-only) don't introduce a value binding and don't conflict.
            if import_has_value
                && let Some(import_sym_id) = import_sym_id
                && let Some(import_sym) = self.ctx.binder.symbols.get(import_sym_id)
            {
                let has_merged_local_non_import_decl =
                    import_sym.declarations.iter().any(|&decl_idx| {
                        if decl_idx == stmt_idx {
                            return false;
                        }
                        if !self.ctx.binder.node_symbols.contains_key(&decl_idx.0) {
                            return false;
                        }
                        // Scope check: merged namespace blocks have separate ScopeIds
                        // but share the same container symbol. Compare container
                        // symbols to allow merged namespaces while excluding
                        // module augmentations and other unrelated scopes.
                        let decl_scope = self.ctx.arena.get_extended(decl_idx).and_then(|ext| {
                            let parent = ext.parent;
                            if parent.is_some() {
                                self.ctx.binder.find_enclosing_scope(self.ctx.arena, parent)
                            } else {
                                self.ctx
                                    .binder
                                    .find_enclosing_scope(self.ctx.arena, decl_idx)
                            }
                        });
                        let in_same_scope = match (import_scope, decl_scope) {
                            (Some(a), Some(b)) if a == b => true,
                            (Some(a), Some(b)) => {
                                let sym_a =
                                    self.ctx.binder.scopes.get(a.0 as usize).and_then(|s| {
                                        self.ctx.binder.node_symbols.get(&s.container_node.0)
                                    });
                                let sym_b =
                                    self.ctx.binder.scopes.get(b.0 as usize).and_then(|s| {
                                        self.ctx.binder.node_symbols.get(&s.container_node.0)
                                    });
                                sym_a.is_some() && sym_a == sym_b
                            }
                            _ => true,
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
                    // Use the parent EXPORT_DECLARATION node if this import-equals
                    // is wrapped in one (e.g. `export import q = M1.s`), so the
                    // error span starts at `export` matching tsc behaviour.
                    let error_node = self
                        .ctx
                        .arena
                        .get_extended(stmt_idx)
                        .and_then(|ext| {
                            let p = ext.parent;
                            self.ctx.arena.get(p).and_then(|pn| {
                                if pn.kind == syntax_kind_ext::EXPORT_DECLARATION {
                                    Some(p)
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(stmt_idx);
                    self.error_at_node(
                        error_node,
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
                        self.ctx.binder.node_symbols.get(&decl_idx.0) == Some(&sym_id)
                    });

                    if import_has_value && is_value && has_local_declaration {
                        let message = format_message(
                            diagnostic_messages::IMPORT_DECLARATION_CONFLICTS_WITH_LOCAL_DECLARATION_OF,
                            &[name],
                        );
                        let error_node = self
                            .ctx
                            .arena
                            .get_extended(stmt_idx)
                            .and_then(|ext| {
                                let p = ext.parent;
                                self.ctx.arena.get(p).and_then(|pn| {
                                    if pn.kind == syntax_kind_ext::EXPORT_DECLARATION {
                                        Some(p)
                                    } else {
                                        None
                                    }
                                })
                            })
                            .unwrap_or(stmt_idx);
                        self.error_at_node(
                            error_node,
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
        // Emit whenever the resolved module kind is ESM, regardless of whether the
        // module setting was explicit or derived from the target (e.g. @target: es6
        // implies module=ES2015 which is ESM, and tsc still emits TS1202 there).
        // Exception: `import type X = require(...)` is a type-only form and never emits TS1202.
        let is_ambient_context = self.is_ambient_declaration(stmt_idx);
        if self.ctx.compiler_options.module.is_es_module()
            && !is_ambient_context
            && !import.is_type_only
        {
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
}
