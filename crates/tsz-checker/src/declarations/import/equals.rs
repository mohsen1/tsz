//! Import-equals declaration validation: `import X = require("y")` and
//! `import X = Namespace.Member` forms, plus import alias duplicate checking.

use crate::state::CheckerState;
use tsz_common::ModuleKind;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Whether a type-only reference came from `import type` or `export type`.
#[derive(Debug)]
enum TypeOnlyKind {
    ImportType,
    ExportType,
}

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
                    let symbol = self
                        .ctx
                        .binder
                        .symbols
                        .get(sym_id)
                        .expect("sym_id resolved from resolve_identifier_symbol");
                    tracing::trace!("Symbol flags: {:?}", symbol.flags);
                    if self.symbol_is_value_only(sym_id, Some(&alias_name)) {
                        self.report_wrong_meaning_diagnostic(
                            &alias_name,
                            import_decl.import_clause,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        );
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
    ///
    /// Check if the target of an import-equals alias includes a type meaning.
    ///
    /// For `import string = ns.Foo`, this resolves `ns.Foo` and checks if the
    /// target symbol has TYPE flags (interface, type alias, class, enum).
    fn import_alias_target_has_type(&self, target_node: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;

        let Some(node) = self.ctx.arena.get(target_node) else {
            return false;
        };

        let target_sym_id = if node.kind == SyntaxKind::Identifier as u16 {
            self.resolve_identifier_symbol(target_node)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            self.resolve_qualified_symbol(target_node)
        } else {
            None
        };

        if let Some(sym_id) = target_sym_id
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            // TYPE includes: Interface, TypeLiteral, TypeParameter, TypeAlias, Class, Enum, EnumMember
            return (symbol.flags & symbol_flags::TYPE) != 0;
        }
        false
    }

    pub(crate) fn check_import_equals_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use crate::state_checking::is_strict_mode_reserved_name;
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

        if let Some(name_node) = self.ctx.arena.get(import.import_clause)
            && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            && is_strict_mode_reserved_name(&name_ident.escaped_text)
        {
            self.emit_module_strict_mode_reserved_word_error(
                import.import_clause,
                &name_ident.escaped_text,
            );
        }

        // TS2438: Import name cannot be a reserved type name (string, number, etc.).
        // TSC checks this for namespace alias imports like `import string = ns.Foo`
        // but ONLY when the target includes a type meaning (interface, type alias, class, etc.).
        // A pure namespace import (no type) is allowed since it doesn't clobber the type name.
        if let Some(name_node) = self.ctx.arena.get(import.import_clause)
            && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = name_ident.escaped_text.as_str();
            if matches!(
                name,
                "string"
                    | "number"
                    | "boolean"
                    | "symbol"
                    | "void"
                    | "any"
                    | "never"
                    | "unknown"
                    | "undefined"
                    | "bigint"
                    | "object"
            ) {
                // Resolve the import target to check if it includes a type meaning
                let target_has_type = self.import_alias_target_has_type(import.module_specifier);
                if target_has_type {
                    self.error_at_node(
                        import.import_clause,
                        &format_message(diagnostic_messages::IMPORT_NAME_CANNOT_BE, &[name]),
                        diagnostic_codes::IMPORT_NAME_CANNOT_BE,
                    );
                }
            }
        }

        // TS1147/TS2439/TS2303 checks for import = require("...") forms.
        // Use get_require_module_specifier so both StringLiteral and recovered require-call
        // representations are handled consistently.
        let require_module_specifier = self.get_require_module_specifier(import.module_specifier);

        // TS1392: An import alias cannot use 'import type'.
        // Only applies to namespace alias forms like `import type Foo = ns.Foo`.
        // `import type X = require("...")` is valid since TS 3.8.
        // Suppress when parse errors exist — malformed imports like
        // `import type defer * as ns from "./a"` already have parser errors,
        // and TS1392 is a cascading false positive on the recovered AST.
        if import.is_type_only && require_module_specifier.is_none() && !self.ctx.has_parse_errors {
            self.error_at_node(
                stmt_idx,
                "An import alias cannot use 'import type'",
                diagnostic_codes::AN_IMPORT_ALIAS_CANNOT_USE_IMPORT_TYPE,
            );
        }
        let mut force_module_not_found = false;
        let mut force_module_not_found_as_2307 = false;
        let mut inside_namespace = false;
        // When in a wrong context (inside block/function), skip module resolution
        // errors. The grammar error (TS1232) is the primary diagnostic.
        let in_wrong_context = self.is_in_non_module_element_context(stmt_idx);
        if require_module_specifier.is_some()
            && self.ctx.arena.get(import.module_specifier).is_some()
        {
            // This is an external module reference (require("..."))
            // Check if we're inside a MODULE_DECLARATION (namespace/module)
            let mut current = stmt_idx;
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

            // TS1147: Import declarations in a namespace cannot reference a module.
            // tsc emits only TS1147 in this case (no TS2307), even when the
            // imported module cannot be resolved. Skip module resolution entirely.
            if inside_namespace {
                self.error_at_node(
                    import.module_specifier,
                    diagnostic_messages::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                    diagnostic_codes::IMPORT_DECLARATIONS_IN_A_NAMESPACE_CANNOT_REFERENCE_A_MODULE,
                );
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

            // TS2303 cycle detection for require() imports is handled eagerly
            // by check_circular_import_aliases() in module_checker.rs, which
            // correctly handles both same-file and cross-file cycles with
            // proper deduplication. Only check_export_target_is_module here.
            if let Some(imported_module) = require_module_specifier.as_deref() {
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

        // Resolve the import alias target to determine if it has Value semantics.
        // This is needed for both TS2440 and TS2437 checks.
        let mut resolved_flags = 0u32;
        let mut resolved_decls = Vec::new();

        if let Some(import_decl) = self.ctx.arena.get_import_decl(
            self.ctx
                .arena
                .get(stmt_idx)
                .expect("stmt_idx is a valid node index from caller"),
        ) {
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
                if let Some(resolved_id) = self.resolve_alias_symbol(target_sym_id, &mut visited)
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
                    if decl_node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
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

            // TS2440: Check if the import binding name conflicts with a local declaration.
            // tsc only reports TS2440 when the import target has value semantics
            // (i.e., it's an instantiated module/namespace). Non-instantiated namespaces
            // (empty or type-only) don't introduce a value binding and don't conflict.
            if import_has_value
                && let Some(import_sym_id) = import_sym_id
                && let Some(import_sym) = self.ctx.binder.symbols.get(import_sym_id)
            {
                let has_merged_local_value_decl = import_sym.declarations.iter().any(|&decl_idx| {
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
                            let sym_a = self.ctx.binder.scopes.get(a.0 as usize).and_then(|s| {
                                self.ctx.binder.node_symbols.get(&s.container_node.0)
                            });
                            let sym_b = self.ctx.binder.scopes.get(b.0 as usize).and_then(|s| {
                                self.ctx.binder.node_symbols.get(&s.container_node.0)
                            });
                            sym_a.is_some() && sym_a == sym_b
                        }
                        _ => true,
                    };
                    if !in_same_scope {
                        return false;
                    }
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        return false;
                    };
                    if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                        return false;
                    }
                    if decl_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && self.declaration_is_enclosing_namespace_of_node(decl_idx, stmt_idx)
                    {
                        return false;
                    }
                    self.declaration_introduces_runtime_value(decl_idx)
                });

                if has_merged_local_value_decl {
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

            for &sym_id in all_symbols {
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
                    if is_namespace
                        && sym.declarations.iter().any(|&decl_idx| {
                            self.declaration_is_enclosing_namespace_of_node(decl_idx, stmt_idx)
                        })
                    {
                        continue;
                    }
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
            // TS2437: Module is hidden by a local declaration with the same name.
            // When a local non-namespace declaration shadows a namespace/module,
            // and that namespace has value semantics (is instantiated), emit TS2437.
            {
                let first_ident_idx = self.get_leftmost_identifier_node(module_specifier_idx);
                if let Some(first_idx) = first_ident_idx
                    && let Some(first_node) = self.ctx.arena.get(first_idx)
                    && first_node.kind == SyntaxKind::Identifier as u16
                    && let Some(first_ident) = self.ctx.arena.get_identifier(first_node)
                {
                    let first_name = first_ident.escaped_text.clone();
                    if let Some(sym_id) = self.resolve_identifier_symbol(first_idx) {
                        let lib_binders = self.get_lib_binders();
                        let sym_flags = self
                            .ctx
                            .binder
                            .get_symbol_with_libs(sym_id, &lib_binders)
                            .map_or(0, |s| s.flags);
                        // If the resolved symbol is NOT a namespace and HAS value meaning,
                        // it shadows the module. Check if there's an actual namespace with
                        // this name that has value semantics (is instantiated).
                        // Pure type declarations (interfaces, type aliases) don't shadow
                        // namespaces for import-equals resolution in tsc.
                        if (sym_flags & tsz_binder::symbol_flags::NAMESPACE) == 0
                            && (sym_flags & tsz_binder::symbol_flags::VALUE) != 0
                        {
                            let ns_has_value = self
                                .check_namespace_has_value_in_outer_scope(&first_name, stmt_idx);
                            if ns_has_value {
                                self.error_at_node_msg(
                                    first_idx,
                                    diagnostic_codes::MODULE_IS_HIDDEN_BY_A_LOCAL_DECLARATION_WITH_THE_SAME_NAME,
                                    &[&first_name],
                                );
                                return;
                            }
                        }
                    }
                }
            }
            self.check_namespace_import(stmt_idx, module_specifier_idx);

            // TS1288: An import alias cannot resolve to a type or type-only declaration
            // when 'verbatimModuleSyntax' is enabled.
            // Fires for `import f3 = Foo.T` when T is a pure type (type alias / interface).
            if self.ctx.compiler_options.verbatim_module_syntax
                && !import.is_type_only
                && !self.ctx.is_declaration_file()
            {
                let pure_type_flags =
                    tsz_binder::symbol_flags::TYPE_ALIAS | tsz_binder::symbol_flags::INTERFACE;
                let value_flags = tsz_binder::symbol_flags::VARIABLE
                    | tsz_binder::symbol_flags::FUNCTION
                    | tsz_binder::symbol_flags::CLASS
                    | tsz_binder::symbol_flags::ENUM
                    | tsz_binder::symbol_flags::NAMESPACE;
                if (resolved_flags & pure_type_flags) != 0 && (resolved_flags & value_flags) == 0 {
                    self.error_at_node(
                        stmt_idx,
                        diagnostic_messages::AN_IMPORT_ALIAS_CANNOT_RESOLVE_TO_A_TYPE_OR_TYPE_ONLY_DECLARATION_WHEN_VERBATIMM,
                        diagnostic_codes::AN_IMPORT_ALIAS_CANNOT_RESOLVE_TO_A_TYPE_OR_TYPE_ONLY_DECLARATION_WHEN_VERBATIMM,
                    );
                }
            }

            return;
        }

        // TS1202: Import assignment cannot be used when targeting ECMAScript modules.
        // Emit whenever the resolved module kind is ESM, regardless of whether the
        // module setting was explicit or derived from the target (e.g. @target: es6
        // implies module=ES2015 which is ESM, and tsc still emits TS1202 there).
        // Exception: `import type X = require(...)` is a type-only form and never emits TS1202.
        // Exception: When the import is inside a namespace, TS1147 takes priority and
        // tsc does not also emit TS1202.
        // Exception: When the import is inside a function body,
        // TS1232 takes priority and tsc does not emit TS1202.
        let is_ambient_context = self.is_ambient_declaration(stmt_idx);
        let in_function = self.is_inside_function_body(stmt_idx);
        if self.ctx.compiler_options.module.is_es_module()
            && self.ctx.compiler_options.module != ModuleKind::Preserve
            && !is_ambient_context
            && !import.is_type_only
            && !inside_namespace
            && !in_function
        {
            self.error_at_node(
                stmt_idx,
                "Import assignment cannot be used when targeting ECMAScript modules. Consider using 'import * as ns from \"mod\"', 'import {a} from \"mod\"', 'import d from \"mod\"', or another module format instead.",
                diagnostic_codes::IMPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES_CONSIDER_USIN,
            );
        }

        // TS1484: import X = require("...") where X is a type under VMS.
        // If the target module only exports a type, the import must use `import type`.
        if self.ctx.compiler_options.verbatim_module_syntax
            && !import.is_type_only
            && !self.ctx.is_declaration_file()
            && !is_ambient_context
            && let Some(ref name) = import_name
            && let Some(module_spec) = require_module_specifier.as_deref()
            && self.is_import_specifier_type_only(module_spec, name)
        {
            let msg = format_message(
                crate::diagnostics::diagnostic_messages::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
                &[name],
            );
            self.error_at_node(
                stmt_idx,
                &msg,
                crate::diagnostics::diagnostic_codes::IS_A_TYPE_AND_MUST_BE_IMPORTED_USING_A_TYPE_ONLY_IMPORT_WHEN_VERBATIMMODULESYNTA,
            );
        }

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let Some(module_name) = require_module_specifier.as_deref() else {
            return;
        };

        // When the import-equals is inside a function body (not just a block),
        // skip module resolution errors. tsc doesn't resolve require() inside functions.
        if in_wrong_context && self.is_inside_function_body(stmt_idx) {
            return;
        }

        // When the import is inside a namespace, TS1147 was already emitted above.
        // tsc does not also emit TS2307, so skip module resolution entirely.
        if inside_namespace {
            return;
        }

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
            self.error_at_node(module_specifier_idx, &message, code);
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
    /// Emits TS1380 "An import alias cannot reference a declaration that was imported using 'import type'."
    /// Emits TS1379 "An import alias cannot reference a declaration that was exported using 'export type'."
    fn check_namespace_import(&mut self, stmt_idx: NodeIndex, module_ref: NodeIndex) {
        let Some(ref_node) = self.ctx.arena.get(module_ref) else {
            return;
        };

        // Handle simple identifier: import x = Namespace
        if ref_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(ref_node) {
                let name = &ident.escaped_text;
                // Skip if identifier is empty (parse error created a placeholder)
                // or if it's a reserved word that should be handled by TS1359
                if name.is_empty() || name == "null" || name == "globalThis" {
                    return;
                }
                // Try to resolve the identifier as a namespace/module
                let resolved = self.resolve_identifier_symbol(module_ref);
                if resolved.is_none() {
                    self.error_cannot_find_namespace_with_suggestion(name, module_ref);
                    return;
                }
                // TS1380/TS1379: Check if the referenced declaration is type-only
                if let Some(sym_id) = resolved {
                    self.check_import_alias_type_only_reference(sym_id, module_ref);
                }
            }
            return;
        }

        // Handle qualified name: import x = Namespace.Member
        if ref_node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.ctx.arena.get_qualified_name(ref_node)
        {
            let export_parent = self.ctx.arena.get_extended(stmt_idx).and_then(|ext| {
                let parent = ext.parent;
                self.ctx.arena.get(parent).and_then(|parent_node| {
                    (parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION).then_some(parent)
                })
            });
            let file_has_export_equals = self.ctx.arena.source_files.first().is_some_and(|sf| {
                sf.statements.nodes.iter().any(|&file_stmt_idx| {
                    self.ctx
                        .arena
                        .get(file_stmt_idx)
                        .and_then(|file_stmt_node| {
                            self.ctx.arena.get_export_assignment(file_stmt_node)
                        })
                        .is_some_and(|export_assignment| export_assignment.is_export_equals)
                })
            });
            let emits_export_import_ts2708 = export_parent.is_some()
                && file_has_export_equals
                && !self.is_ambient_declaration(stmt_idx)
                && !export_parent.is_some_and(|parent| self.is_ambient_declaration(parent));

            // Check the leftmost part first - this is what determines TS2503 vs TS2694
            let left_name = self.get_leftmost_identifier_name(qn.left);
            if let Some(name) = left_name {
                // Try to resolve the left identifier
                let left_resolved = self.resolve_leftmost_qualified_name(qn.left);
                if left_resolved.is_none() {
                    self.error_cannot_find_namespace_with_suggestion(&name, qn.left);
                    return; // Don't check for TS2694 if left doesn't exist
                }

                // Only check for missing member (TS2694) if the resolved left symbol
                // actually has namespace/module meaning. If the resolved symbol is a
                // pure type (e.g. a local interface shadowing an outer namespace),
                // the import-equals should resolve to the outer namespace instead,
                // so don't emit a misleading TS2694.
                let (left_is_namespace, left_has_value) = if let Some(sym_id) = left_resolved {
                    let lib_binders = self.get_lib_binders();
                    if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    {
                        let is_namespace =
                            (symbol.flags & tsz_binder::symbol_flags::NAMESPACE) != 0;
                        let mut has_value = (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0;
                        if has_value
                            && (symbol.flags & tsz_binder::symbol_flags::VALUE_MODULE) != 0
                            && (symbol.flags
                                & (tsz_binder::symbol_flags::VALUE
                                    & !tsz_binder::symbol_flags::VALUE_MODULE))
                                == 0
                        {
                            has_value = symbol.declarations.iter().any(|&decl_idx| {
                                self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                                    decl_node.kind
                                        != tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                                        || self.is_namespace_declaration_instantiated(decl_idx)
                                })
                            });
                        }
                        (is_namespace, has_value)
                    } else {
                        (false, false)
                    }
                } else {
                    (false, false)
                };

                if left_is_namespace {
                    // TS2708: Also require that the qualified member cannot be
                    // resolved. When the member IS found (e.g., exported interface),
                    // the import just creates a type alias — no TS2708.
                    let member_resolves = self.resolve_qualified_symbol(module_ref).is_some();
                    if !left_has_value && emits_export_import_ts2708 && !member_resolves {
                        self.error_namespace_used_as_value_at(&name, qn.left);
                    }
                    // If left is resolved, check if right member exists (TS2694)
                    // Use the existing report_type_query_missing_member which handles this correctly
                    self.report_type_query_missing_member(module_ref);
                }

                // TS1380/TS1379: Check if any symbol in the qualified name chain
                // references a type-only import or export.
                self.check_qualified_name_type_only(module_ref);
            }
        }
    }

    /// Check if any symbol in a qualified name chain references a type-only declaration.
    /// Emits TS1380 (imported using 'import type') or TS1379 (exported using 'export type').
    fn check_qualified_name_type_only(&mut self, module_ref: NodeIndex) {
        if let Some(type_only_kind) = self.find_type_only_in_chain(module_ref) {
            self.emit_import_alias_type_only_error(type_only_kind, module_ref);
        }
    }

    /// Walk a qualified name or identifier and check if any resolved symbol is type-only.
    /// Returns the kind of type-only reference found (import type vs export type).
    /// Unlike `resolve_qualified_symbol` which resolves through all aliases, this checks
    /// each intermediate symbol for type-only status.
    fn find_type_only_in_chain(&self, idx: NodeIndex) -> Option<TypeOnlyKind> {
        self.find_type_only_in_chain_inner(idx, 0)
    }

    fn find_type_only_in_chain_inner(&self, idx: NodeIndex, depth: usize) -> Option<TypeOnlyKind> {
        if depth > 20 {
            return None;
        }
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(idx)?;
            return self.check_symbol_type_only_kind(sym_id);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            // Check left part recursively
            if let Some(kind) = self.find_type_only_in_chain_inner(qn.left, depth + 1) {
                return Some(kind);
            }

            // Resolve the left symbol to get the namespace, then look up the right member
            // WITHOUT following alias resolution — check the raw member symbol.
            let mut visited = Vec::new();
            let left_sym = self.resolve_qualified_symbol_inner(qn.left, &mut visited, 0)?;
            let left_sym_resolved = self
                .resolve_alias_symbol(left_sym, &mut visited)
                .unwrap_or(left_sym);

            let lib_binders = self.get_lib_binders();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym_resolved, &lib_binders)?;

            let right_name = self
                .ctx
                .arena
                .get(qn.right)
                .and_then(|n| self.ctx.arena.get_identifier(n))
                .map(|ident| ident.escaped_text.as_str())?;

            // Look up the raw member symbol (before alias resolution)
            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                // Check the raw member symbol for type-only
                if let Some(kind) = self.check_symbol_type_only_kind(member_sym) {
                    return Some(kind);
                }
                // Also resolve through alias and check the final target
                let resolved = self
                    .resolve_alias_symbol(member_sym, &mut visited)
                    .unwrap_or(member_sym);
                if resolved != member_sym
                    && let Some(kind) = self.check_symbol_type_only_kind(resolved)
                {
                    return Some(kind);
                }
                // Check all visited aliases in the chain
                for &alias_id in &visited {
                    if let Some(kind) = self.check_symbol_type_only_kind(alias_id) {
                        return Some(kind);
                    }
                }
            }

            // Fall back: try cross-file resolution using the ORIGINAL left sym
            // (before alias resolution) which may have import_module set.
            let orig_left_symbol = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders);
            if let Some(orig) = orig_left_symbol
                && let Some(ref module_specifier) = orig.import_module
            {
                let mut reexport_visited = Vec::new();
                if let Some(resolved) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    &mut reexport_visited,
                ) {
                    // Check visited aliases from resolution chain
                    for &alias_id in &reexport_visited {
                        if let Some(kind) = self.check_symbol_type_only_kind(alias_id) {
                            return Some(kind);
                        }
                    }
                    return self.check_symbol_type_only_kind(resolved);
                }
            }

            // Ultimate fallback: use resolve_qualified_symbol and check the result
            // This handles cases where the member is found through complex resolution
            let mut resolve_visited = Vec::new();
            if let Some(resolved) =
                self.resolve_qualified_symbol_inner(idx, &mut resolve_visited, 0)
            {
                if let Some(kind) = self.check_symbol_type_only_kind(resolved) {
                    return Some(kind);
                }
                for &alias_id in &resolve_visited {
                    if let Some(kind) = self.check_symbol_type_only_kind(alias_id) {
                        return Some(kind);
                    }
                }
            }
        }

        None
    }

    /// Check if a symbol is type-only, and if so, determine whether it was
    /// imported using 'import type' or exported using 'export type'.
    fn check_symbol_type_only_kind(&self, sym_id: tsz_binder::SymbolId) -> Option<TypeOnlyKind> {
        self.check_symbol_type_only_kind_inner(sym_id, 0)
    }

    fn check_symbol_type_only_kind_inner(
        &self,
        sym_id: tsz_binder::SymbolId,
        depth: usize,
    ) -> Option<TypeOnlyKind> {
        if depth > 10 {
            return None;
        }
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;

        if symbol.is_type_only {
            // Symbol is directly marked type-only — determine if it was via import or export
            return if symbol.import_module.is_some() {
                Some(TypeOnlyKind::ImportType)
            } else {
                Some(TypeOnlyKind::ExportType)
            };
        }

        // Check alias chain for transitive type-only
        if self.alias_resolves_to_type_only(sym_id) {
            return Some(self.determine_type_only_kind_from_alias(sym_id));
        }

        // For import-equals aliases (`import A = ns.Member`), the binder doesn't set
        // import_module, so alias_resolves_to_type_only can't trace through.
        // Check the declaration's RHS for type-only references.
        // Also follow through export alias → local symbol chains.
        if symbol.flags & tsz_binder::symbol_flags::ALIAS != 0 {
            // Collect all symbols to check: the current symbol plus anything it aliases to
            let mut syms_to_check = vec![sym_id];
            let mut visited_resolve = Vec::new();
            if let Some(resolved) = self.resolve_alias_symbol(sym_id, &mut visited_resolve) {
                syms_to_check.extend(visited_resolve);
                syms_to_check.push(resolved);
            }
            for check_sym_id in syms_to_check {
                // Use lookup_symbol_with_name to get the correct arena for cross-file
                // symbols. When c.ts checks `import AA = b.A`, the resolved symbol
                // lives in b.ts's arena, not c.ts's.
                let (check_sym, sym_arena) = match self.lookup_symbol_with_name(check_sym_id, None)
                {
                    Some(pair) => pair,
                    None => continue,
                };
                // Check directly: is the resolved target type-only?
                if check_sym.is_type_only {
                    return if check_sym.import_module.is_some() {
                        Some(TypeOnlyKind::ImportType)
                    } else {
                        Some(TypeOnlyKind::ExportType)
                    };
                }
                // For import-equals declarations, check the RHS leftmost identifier
                // using a non-recursive helper to avoid stack overflow.
                // For export specifiers, follow through to the local binding.
                for &decl_idx in &check_sym.declarations {
                    if !decl_idx.is_some() {
                        continue;
                    }
                    let Some(decl_node) = sym_arena.get(decl_idx) else {
                        continue;
                    };
                    if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                        if let Some(import_decl) = sym_arena.get_import_decl(decl_node)
                            && let Some(kind) = self.check_import_equals_rhs_type_only_cross_file(
                                import_decl.module_specifier,
                                sym_arena,
                            )
                        {
                            return Some(kind);
                        }
                    } else if decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER {
                        // For `export { A }`, the specifier's name/property_name
                        // identifier references the local binding. Use node_symbols
                        // (merged across all files) to find it, then recursively
                        // check the local symbol for type-only status.
                        if let Some(spec) = sym_arena.get_specifier(decl_node) {
                            let local_ident = if spec.property_name.is_some() {
                                spec.property_name
                            } else {
                                spec.name
                            };
                            if let Some(&local_sym_id) =
                                self.ctx.binder.node_symbols.get(&local_ident.0)
                                && local_sym_id != check_sym_id
                                && let Some(kind) =
                                    self.check_symbol_type_only_kind_inner(local_sym_id, depth + 1)
                            {
                                return Some(kind);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Check the RHS of an import-equals for type-only references, using the
    /// provided arena for AST node access. This handles cross-file cases where
    /// the import-equals declaration is in a different file than the one being
    /// checked (e.g., c.ts checks `b.A` where `import A = a.A` is in b.ts).
    ///
    /// Uses `binder.node_symbols` (merged across all files) instead of
    /// `resolve_identifier_symbol` (which only searches the current file's scope).
    fn check_import_equals_rhs_type_only_cross_file(
        &self,
        rhs_idx: NodeIndex,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> Option<TypeOnlyKind> {
        let node = arena.get(rhs_idx)?;
        let lib_binders = self.get_lib_binders();

        if node.kind == SyntaxKind::Identifier as u16 {
            // Only use resolve_identifier_symbol for same-file cases.
            // For cross-file arenas, node indices overlap between files,
            // so resolve_identifier_symbol would resolve the WRONG identifier.
            let is_same_arena = std::ptr::eq(arena, self.ctx.arena);
            if is_same_arena && let Some(sym_id) = self.resolve_identifier_symbol(rhs_idx) {
                let sym = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
                if sym.is_type_only {
                    return if sym.import_module.is_some() {
                        Some(TypeOnlyKind::ImportType)
                    } else {
                        Some(TypeOnlyKind::ExportType)
                    };
                }
                if self.alias_resolves_to_type_only(sym_id) {
                    return Some(self.determine_type_only_kind_from_alias(sym_id));
                }
                return None;
            }

            // Cross-file fallback: resolve_identifier_symbol only searches the
            // current file's scope. For declarations from other files, look up
            // the identifier by name in the per-file binder's file_locals.
            let ident = arena.get_identifier(node)?;
            let name = &ident.escaped_text;

            // Find which file's binder has this identifier as a local
            // Use the pre-built global index for O(1) lookup by name
            if let Some(entries) = self
                .ctx
                .global_file_locals_index
                .as_ref()
                .and_then(|idx| idx.get(name.as_str()))
            {
                for &(_file_idx, sym_id) in entries {
                    if let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                        if sym.is_type_only {
                            return if sym.import_module.is_some() {
                                Some(TypeOnlyKind::ImportType)
                            } else {
                                Some(TypeOnlyKind::ExportType)
                            };
                        }
                        if self.alias_resolves_to_type_only(sym_id) {
                            return Some(self.determine_type_only_kind_from_alias(sym_id));
                        }
                    }
                }
            } else if let Some(all_binders) = &self.ctx.all_binders {
                // Fallback when global index not available
                for file_binder in all_binders.iter() {
                    if let Some(sym_id) = file_binder.file_locals.get(name)
                        && let Some(sym) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    {
                        if sym.is_type_only {
                            return if sym.import_module.is_some() {
                                Some(TypeOnlyKind::ImportType)
                            } else {
                                Some(TypeOnlyKind::ExportType)
                            };
                        }
                        if self.alias_resolves_to_type_only(sym_id) {
                            return Some(self.determine_type_only_kind_from_alias(sym_id));
                        }
                    }
                }
            }

            return None;
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = arena.get_qualified_name(node)?;
            // Only check the leftmost identifier for type-only
            return self.check_import_equals_rhs_type_only_cross_file(qn.left, arena);
        }

        None
    }

    /// Determine the type-only kind by tracing through alias chains.
    fn determine_type_only_kind_from_alias(&self, sym_id: tsz_binder::SymbolId) -> TypeOnlyKind {
        let lib_binders = self.get_lib_binders();
        let mut visited = Vec::new();
        if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited) {
            // Check the alias chain for type-only symbols
            for &alias_id in &visited {
                if let Some(alias_sym) =
                    self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
                    && alias_sym.is_type_only
                {
                    return if alias_sym.import_module.is_some() {
                        TypeOnlyKind::ImportType
                    } else {
                        TypeOnlyKind::ExportType
                    };
                }
            }
            // Check the final target
            if let Some(target_sym) = self.ctx.binder.get_symbol_with_libs(target, &lib_binders)
                && target_sym.is_type_only
            {
                return if target_sym.import_module.is_some() {
                    TypeOnlyKind::ImportType
                } else {
                    TypeOnlyKind::ExportType
                };
            }
        }
        // Default to import type (more common case)
        TypeOnlyKind::ImportType
    }

    /// Check a single symbol for type-only status and emit TS1380/TS1379.
    fn check_import_alias_type_only_reference(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        error_node: NodeIndex,
    ) {
        if let Some(kind) = self.check_symbol_type_only_kind(sym_id) {
            self.emit_import_alias_type_only_error(kind, error_node);
        }
    }

    /// Emit the appropriate TS1380 or TS1379 error.
    fn emit_import_alias_type_only_error(&mut self, kind: TypeOnlyKind, error_node: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        match kind {
            TypeOnlyKind::ImportType => {
                self.error_at_node(
                    error_node,
                    diagnostic_messages::AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_IMPORTED_USING_IMPORT_TY,
                    diagnostic_codes::AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_IMPORTED_USING_IMPORT_TY,
                );
            }
            TypeOnlyKind::ExportType => {
                self.error_at_node(
                    error_node,
                    diagnostic_messages::AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_EXPORTED_USING_EXPORT_TY,
                    diagnostic_codes::AN_IMPORT_ALIAS_CANNOT_REFERENCE_A_DECLARATION_THAT_WAS_EXPORTED_USING_EXPORT_TY,
                );
            }
        }
    }

    /// Get the leftmost identifier name from a node (handles nested `QualifiedNames`).
    /// Get the leftmost identifier `NodeIndex` from an identifier or qualified name.
    fn get_leftmost_identifier_node(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(idx);
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.get_leftmost_identifier_node(qn.left);
        }
        None
    }

    pub(crate) fn get_leftmost_identifier_name(&self, idx: NodeIndex) -> Option<String> {
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

    /// Check if a namespace with the given name exists in an outer scope and has
    /// value semantics (is instantiated). Used for TS2437 to determine if a local
    /// non-namespace declaration is truly shadowing an instantiated module.
    fn check_namespace_has_value_in_outer_scope(&self, name: &str, node: NodeIndex) -> bool {
        // Walk up from the enclosing scope's parent to find a NAMESPACE symbol
        let Some(scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, node) else {
            return false;
        };
        // Start from the parent of the current scope (skip the scope where the var is)
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return false;
        };
        let mut walk_id = current_scope.parent;
        let lib_binders = self.get_lib_binders();

        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(name) {
                let sym_flags = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or(0, |s| s.flags);
                if (sym_flags & tsz_binder::symbol_flags::NAMESPACE) != 0 {
                    // Found a namespace — check if it has value (is instantiated)
                    let has_value = (sym_flags & tsz_binder::symbol_flags::VALUE) != 0;
                    if has_value
                        && (sym_flags & tsz_binder::symbol_flags::VALUE_MODULE) != 0
                        && (sym_flags
                            & (tsz_binder::symbol_flags::VALUE
                                & !tsz_binder::symbol_flags::VALUE_MODULE))
                            == 0
                    {
                        // Only VALUE_MODULE — check if any declaration is instantiated
                        if let Some(sym) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        {
                            return sym.declarations.iter().any(|&decl_idx| {
                                self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                                    decl_node.kind
                                        != tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                                        || self.is_namespace_declaration_instantiated(decl_idx)
                                })
                            });
                        }
                        return false;
                    }
                    return has_value;
                }
            }
            // Move to parent scope
            if walk_id == scope.parent {
                break; // At root scope
            }
            walk_id = scope.parent;
        }
        false
    }

    /// Whether a declaration introduces a runtime value binding in the current file.
    ///
    /// Used by TS2440 conflict checks to avoid reporting conflicts against purely
    /// type-space declarations (e.g. interfaces/type aliases).
    fn declaration_introduces_runtime_value(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::ENUM_DECLARATION
            | syntax_kind_ext::VARIABLE_DECLARATION
            | syntax_kind_ext::VARIABLE_STATEMENT => true,
            syntax_kind_ext::MODULE_DECLARATION => {
                self.is_namespace_declaration_instantiated(decl_idx)
            }
            _ => false,
        }
    }

    fn declaration_is_enclosing_namespace_of_node(
        &self,
        decl_idx: NodeIndex,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        self.node_has_ancestor(node_idx, decl_idx)
    }

    fn node_has_ancestor(&self, mut node_idx: NodeIndex, ancestor_idx: NodeIndex) -> bool {
        let mut guard = 0u32;
        loop {
            if node_idx == ancestor_idx {
                return true;
            }
            guard += 1;
            if guard > 4096 {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            node_idx = ext.parent;
        }
    }
}
