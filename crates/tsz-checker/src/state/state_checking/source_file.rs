//! Source file checking entry point.
//!
//! Contains `check_source_file` (the main per-file entry point) and
//! reserved-await identifier checks (TS1262).

use crate::context::{TypingRequest, is_declaration_file_name};
use crate::state::CheckerState;
use crate::statements::StatementChecker;
use rustc_hash::FxHashSet;
use tracing::{Level, span};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check if the file contains property/element access expressions that need
    /// boxed type registration. Uses the binder's pre-computed flag when available,
    /// avoiding an O(N) AST scan.
    const fn needs_boxed_type_registration(&self) -> bool {
        // PERF: The binder already walks every node during binding. We check its
        // has_property_access flag first (O(1)). If the binder doesn't track this
        // yet, fall back to a conservative `true` — almost all non-trivial files
        // have property access, so the only cost is registering boxed types
        // unnecessarily for very small files (a few microseconds).
        true
    }

    fn prepare_source_file_for_checking(&mut self, root_idx: NodeIndex) -> Option<NodeIndex> {
        // Reset per-file flags
        self.ctx.is_in_ambient_declaration_file = false;

        let node = self.ctx.arena.get(root_idx)?;
        let sf = self.ctx.arena.get_source_file(node)?;
        self.resolve_compiler_options_from_source(&sf.text);
        if self.has_ts_nocheck_pragma(&sf.text) {
            return None;
        }

        // `type_env` is rebuilt per file, so drop per-file symbol-resolution memoization.
        self.ctx.application_symbols_resolved.clear();
        self.ctx.application_symbols_resolution_set.clear();
        // Reset global resolution fuel for the new file.
        crate::state_domain::type_environment::lazy::reset_global_resolution_fuel();

        // Register Function DefIds in the interner BEFORE building the environment.
        // This ensures `T extends Function` constraint checks during type alias
        // processing can identify the Function interface by DefId.
        if self.needs_boxed_type_registration() {
            self.register_function_def_ids_early();
        }

        // Phase 1 DefId-first: warm local caches with stable DefIds.
        //
        // When the checker received a pre-populated shared DefinitionStore
        // from the merge pipeline, we warm local caches in one pass from
        // the store's authoritative symbol→DefId index. This is faster than
        // iterating each binder's semantic_defs and re-converting
        // SemanticDefEntry → DefinitionInfo.
        //
        // When no shared store exists (single-file mode), fall back to the
        // per-binder pre-population path.
        if self.ctx.has_shared_store() {
            self.ctx.warm_local_caches_from_shared_store();
        } else {
            self.ctx.pre_populate_def_ids_from_binder();
            self.ctx.pre_populate_def_ids_from_lib_binders();
        }

        // Phase 1c: resolve cross-batch heritage. Now that all DefIds from both
        // the primary binder and lib binders are registered, resolve heritage_names
        // (e.g., `class MyError extends Error`) to DefId-level extends/implements.
        // Skip when the DefinitionStore was fully populated at merge time
        // (heritage already resolved in from_semantic_defs).
        if !self.ctx.definition_store.is_fully_populated() {
            self.ctx.resolve_cross_batch_heritage();
        }

        // Build TypeEnvironment with all type-defining symbols.
        // This populates both ctx.type_env and ctx.type_environment in-place
        // via get_type_of_symbol -> compute_type_of_symbol -> register_def_in_envs.
        self.build_type_environment();

        // Wire up DefinitionStore so TypeEnvironment::get_def_kind can fall
        // back to it when the local def_kinds map is incomplete.
        self.ctx.ensure_type_env_has_definition_store();

        // Sync type_environment from type_env to ensure FlowAnalyzer has the
        // complete environment (including the DefinitionStore wired above).
        // register_def_in_envs writes to both envs, but some paths may fail
        // try_borrow_mut on type_environment during recursive resolution.
        // A single clone here ensures consistency.
        {
            let env_snapshot = self.ctx.type_env.borrow().clone();
            *self.ctx.type_environment.borrow_mut() = env_snapshot;
        }

        // Register boxed types (String, Number, Boolean, etc.) from lib.d.ts
        // This enables primitive property access to use lib definitions instead of hardcoded lists
        // IMPORTANT: Must run AFTER build_type_environment() because it replaces the
        // TypeEnvironment, which would erase the boxed/array type registrations.
        if self.needs_boxed_type_registration() {
            self.register_boxed_types();
        }

        // Mark that we're now in the checking phase. During build_type_environment,
        // closures may be type-checked without contextual types, which would cause
        // premature TS7006 errors. The checking phase ensures contextual types are available.
        self.ctx.is_checking_statements = true;

        // In .d.ts files, the entire file is an ambient context.
        if self.ctx.is_declaration_file() {
            self.ctx.is_in_ambient_declaration_file = true;
        }

        Some(root_idx)
    }

    fn check_interface_declarations_recursively(
        &mut self,
        statements: &[NodeIndex],
        reset_fuel_between_interfaces: bool,
        interface_filter: Option<&FxHashSet<String>>,
        extension_filter: Option<&FxHashSet<String>>,
    ) {
        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let interface_name = self
                    .ctx
                    .arena
                    .get_interface(stmt_node)
                    .and_then(|iface| self.ctx.arena.get(iface.name))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|ident| ident.escaped_text.as_str());
                if let Some(filter) = interface_filter {
                    let Some(name) = interface_name else {
                        continue;
                    };
                    if !filter.contains(name) {
                        continue;
                    }
                }
                if self.ctx.binder.node_symbols.contains_key(&stmt_idx.0) {
                    if reset_fuel_between_interfaces {
                        self.ctx
                            .type_resolution_fuel
                            .set(crate::state::MAX_TYPE_RESOLUTION_OPS);
                        crate::state_domain::type_environment::lazy::reset_global_resolution_fuel();
                        let check_extension_compatibility = match extension_filter {
                            Some(filter) => {
                                interface_name.is_some_and(|name| filter.contains(name))
                            }
                            None => true,
                        };
                        self.check_lib_interface_declaration_post_merge(
                            stmt_idx,
                            check_extension_compatibility,
                        );
                    } else {
                        self.check_interface_declaration(stmt_idx);
                    }
                }
                continue;
            }

            if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
                continue;
            }

            let Some(module_decl) = self.ctx.arena.get_module(stmt_node) else {
                continue;
            };
            if module_decl.body.is_none() {
                continue;
            }
            let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
                continue;
            };
            if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
                continue;
            }
            let Some(block) = self.ctx.arena.get_module_block(body_node) else {
                continue;
            };
            let Some(inner) = &block.statements else {
                continue;
            };
            self.check_interface_declarations_recursively(
                &inner.nodes,
                reset_fuel_between_interfaces,
                interface_filter,
                extension_filter,
            );
        }
    }

    /// Check only interface declarations in a source file after full environment setup.
    ///
    /// This is used for post-merge standard library validation so interface-specific
    /// diagnostics like TS2344/TS2430 are re-evaluated without running the full lib
    /// statement pipeline.
    pub fn check_source_file_interfaces_only(&mut self, root_idx: NodeIndex) {
        let _span =
            span!(Level::INFO, "check_source_file_interfaces_only", idx = ?root_idx).entered();

        let Some(root_idx) = self.prepare_source_file_for_checking(root_idx) else {
            return;
        };

        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        self.check_interface_declarations_recursively(&sf.statements.nodes, false, None, None);
    }

    /// Check only interface declarations, refreshing type-resolution fuel between declarations.
    ///
    /// This is reserved for post-merge standard library validation, where a synthetic
    /// lib file may contain many independent affected interfaces and an early DOM
    /// interface must not exhaust the budget for later diagnostics.
    pub fn check_source_file_interfaces_only_with_fresh_interface_fuel(
        &mut self,
        root_idx: NodeIndex,
    ) {
        let _span = span!(
            Level::INFO,
            "check_source_file_interfaces_only_with_fresh_interface_fuel",
            idx = ?root_idx
        )
        .entered();

        let Some(root_idx) = self.prepare_source_file_for_checking(root_idx) else {
            return;
        };

        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        self.check_interface_declarations_recursively(&sf.statements.nodes, true, None, None);
    }

    /// Check selected interfaces with the minimal post-merge lib validation path.
    pub fn check_source_file_interfaces_only_filtered_post_merge(
        &mut self,
        root_idx: NodeIndex,
        interface_filter: &FxHashSet<String>,
        extension_filter: &FxHashSet<String>,
    ) {
        let _span = span!(
            Level::INFO,
            "check_source_file_interfaces_only_filtered_post_merge",
            idx = ?root_idx
        )
        .entered();

        let Some(root_idx) = self.prepare_source_file_for_checking(root_idx) else {
            return;
        };

        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        self.check_interface_declarations_recursively(
            &sf.statements.nodes,
            true,
            Some(interface_filter),
            Some(extension_filter),
        );
    }

    /// Check a source file and populate diagnostics (main entry point).
    ///
    /// This is the primary entry point for type checking after parsing and binding.
    /// It traverses the entire AST and performs all type checking operations.
    pub fn check_source_file(&mut self, root_idx: NodeIndex) {
        let _span = span!(Level::INFO, "check_source_file", idx = ?root_idx).entered();
        let Some(root_idx) = self.prepare_source_file_for_checking(root_idx) else {
            return;
        };
        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        // In .d.ts files, emit TS1036 for non-declaration top-level statements.
        // The entire file is an ambient context, so statements like break, continue,
        // return, debugger, if, while, for, etc. are not allowed.
        let is_dts = self.ctx.is_declaration_file();

        // TS2563: In tsc, this is emitted when flow analysis recursion depth
        // exceeds 2000 during getTypeAtFlowNode, NOT as a pre-check on total
        // binder flow node count. tsz creates more flow nodes per expression
        // (optional chains create multiple branch/join nodes). The old threshold
        // of 2000 caused false TS2563 on files that tsc compiles fine.
        //
        // Heuristic: check both total flow nodes AND top-level statement count.
        // Files with many top-level sequential statements (like
        // largeControlFlowGraph.ts: 10,003 assignments) have deep antecedent
        // chains that overwhelm flow analysis. Files with many functions but
        // few top-level statements (like deep50.ts: 50 functions, 37,502 total
        // flow nodes) have flow nodes distributed across independent graphs.
        // The long-term fix: implement tsc's runtime depth check in narrowing.
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            const MAX_TOP_LEVEL_STATEMENTS: usize = 5_000;
            let top_level_stmt_count = sf.statements.nodes.len();
            if top_level_stmt_count > MAX_TOP_LEVEL_STATEMENTS
                && let Some(&first_stmt) = sf.statements.nodes.first()
                && let Some(first_node) = self.ctx.arena.get(first_stmt)
            {
                self.ctx.error(
                    first_node.pos,
                    0,
                    diagnostic_messages::THE_CONTAINING_FUNCTION_OR_MODULE_BODY_IS_TOO_LARGE_FOR_CONTROL_FLOW_ANALYSIS.to_string(),
                    diagnostic_codes::THE_CONTAINING_FUNCTION_OR_MODULE_BODY_IS_TOO_LARGE_FOR_CONTROL_FLOW_ANALYSIS,
                );
            }
        }

        let prev_unreachable = self.ctx.is_unreachable;
        let prev_reported = self.ctx.has_reported_unreachable;
        let suppress_grammar = self.has_syntax_parse_errors();

        // TS1046: In .d.ts files, top-level value declarations must start
        // with 'declare' or 'export'. Report the first violation only.
        if is_dts && !suppress_grammar {
            self.check_dts_top_level_declare_or_export(&sf.statements.nodes);
        }

        let mut seen_dts_ambient_violation = false;
        for &stmt_idx in &sf.statements.nodes {
            if is_dts && !suppress_grammar && !seen_dts_ambient_violation {
                seen_dts_ambient_violation = self.check_dts_statement_in_ambient_context(stmt_idx);
            }
            self.check_statement(stmt_idx);
            if !self.statement_falls_through(stmt_idx) {
                self.ctx.is_unreachable = true;
            }
        }
        self.ctx.is_unreachable = prev_unreachable;
        self.ctx.has_reported_unreachable = prev_reported;

        if self.is_js_file() && self.ctx.should_resolve_jsdoc() {
            self.recheck_checked_js_import_diagnostics(&sf.statements.nodes);
        }

        // Re-check closures that deferred TS7006 during type env building.
        // These closures had skip_implicit_any=true because is_checking_statements
        // was false. Now that all statements have been checked (giving closures a
        // chance to be re-processed with contextual types), any remaining unchecked
        // closures truly have no contextual type and need TS7006 emitted.
        self.recheck_deferred_implicit_any_closures();

        self.check_isolated_declarations(&sf.statements.nodes);
        self.check_isolated_decl_class_expressions(&sf.statements.nodes);
        self.check_isolated_decl_augmentations(&sf.statements.nodes);
        self.check_reserved_await_identifier_in_module(root_idx);
        // Check for function overload implementations (2389, 2391)
        self.check_function_implementations(&sf.statements.nodes);

        // Check for export assignment with other exports (2309)
        self.check_export_assignment(&sf.statements.nodes);

        // TS4094: exported `export default <call-returning-anonymous-class>` patterns.
        if self.ctx.emit_declarations() && !self.ctx.is_declaration_file() {
            self.check_ts4094_in_export_assignments(&sf.statements.nodes);
        }

        // Check for wildcard re-export collisions (2308)
        self.check_wildcard_reexport_collisions(&sf.statements.nodes);

        // Check for circular import aliases (2303)
        self.check_circular_import_aliases();

        // Check for circular CommonJS export aliases (2303)
        // e.g., `exports.blah = exports.someProp` in JS files
        if self.ctx.is_js_file() {
            self.check_commonjs_circular_aliases(&sf.statements.nodes);
        }

        // Check for cross-file circular type aliases (TS2456).
        // This runs AFTER all statements have been checked so that
        // cross-file symbol delegations have populated the DefinitionStore
        // with type alias bodies.  The inline TS2456 check in
        // compute_type_of_symbol handles same-file cycles, but cross-file
        // cycles can only be detected post-hoc because the DefinitionStore
        // bodies aren't available during the initial build_type_environment pass.
        self.check_cross_file_circular_type_aliases();
        self.recheck_static_member_class_type_param_refs_in_source_file(&sf.statements.nodes);

        // Check for TS1148: module none errors
        if matches!(
            self.ctx.compiler_options.module,
            tsz_common::common::ModuleKind::None
        ) && !is_dts
            && !self.ctx.compiler_options.target.supports_es2015()
        {
            self.check_module_none_statements(&sf.statements.nodes);
        }

        // Check for duplicate identifiers (2300)
        self.check_duplicate_identifiers();
        self.check_lib_merged_interface_duplicate_index_signatures();
        self.check_commonjs_export_property_redeclarations();

        // Check for constructor parameter property vs explicit property conflicts (2300/2687)
        self.check_constructor_parameter_property_conflicts();

        // Check for built-in global identifier conflicts (2397)
        self.check_built_in_global_identifier_conflicts();

        // Check for missing global types (2318)
        // Emits errors at file start for essential types when libs are not loaded
        self.check_missing_global_types();

        // Check triple-slash reference directives (TS6053).
        // tsc suppresses TS6053 when the file has syntax errors (TS1011),
        // so only check when there are no parse errors.
        if !self.ctx.compiler_options.no_resolve && !self.ctx.has_parse_errors {
            self.check_triple_slash_references(&sf.file_name, &sf.text);
        }

        // Check for duplicate AMD module name assignments (TS2458)
        self.check_amd_module_names(&sf.text);

        // Check for unused declarations (TS6133/TS6196)
        if self.ctx.no_unused_locals() || self.ctx.no_unused_parameters() {
            self.check_unused_declarations();
        }
        // JS grammar checks: emit TS8xxx errors for TypeScript-only syntax in JS files
        if self.is_js_file() {
            self.check_js_grammar_statements(&sf.statements.nodes);

            // TS8022: Check for orphaned @extends/@augments tags not attached to a class
            self.check_orphaned_extends_tags(&sf.statements.nodes);

            // TS8033: Check for @typedef comments with multiple @type tags
            self.check_typedef_duplicate_type_tags();

            // TS2300: Check JSDoc typedefs against class-like value/export declarations
            self.check_jsdoc_typedef_name_conflicts();

            // TS2300: Check for duplicate @import names across JSDoc comments
            self.check_jsdoc_duplicate_imports();

            // TS1003: Check @param tags for malformed `*` names
            self.check_jsdoc_param_invalid_names();

            // TS1003: Check @property/@member tags for private-name syntax
            self.check_jsdoc_property_private_names();

            // TS7014/TS1110/TS2304: malformed JSDoc function parameter types
            self.check_malformed_jsdoc_function_type_params();

            // TS1110: unsupported multiline @typedef wrappers without leading `*`
            self.check_jsdoc_unwrapped_multiline_typedefs();

            // TS8021: Check for @typedef without type or @property tags
            self.check_typedef_missing_type();

            // TS8039: Check for @template tags after @typedef/@callback/@overload
            self.check_template_after_typedef_callback();

            // TS1273/TS1277: Check for invalid modifiers on @template type parameters
            self.check_jsdoc_template_modifiers();

            // TS2304: Check for @typedef base types that can't be resolved
            self.check_jsdoc_typedef_base_types();
        }

        // Emit deferred TS2875 (JSX import source not found) if set.
        // This is deferred because the check runs inside JSX element type
        // resolution which may be inside a speculative call-checker context.
        if let Some((node_idx, runtime_path)) = self.ctx.deferred_jsx_import_source_error.take() {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                node_idx,
                diagnostic_codes::THIS_JSX_TAG_REQUIRES_THE_MODULE_PATH_TO_EXIST_BUT_NONE_COULD_BE_FOUND_MAKE_SURE,
                &[&runtime_path],
            );
        }

        // Re-emit TS2454 diagnostics that were lost to speculative rollback.
        // check_flow_usage runs during type computation, which can happen
        // inside speculative call-checker contexts that truncate diagnostics
        // on rollback. The deferred buffer survives rollback. We only re-emit
        // if the diagnostic is not already present (dedup by error_at_node).
        let deferred_ts2454 = std::mem::take(&mut self.ctx.deferred_ts2454_errors);
        for (node_idx, sym_id) in deferred_ts2454 {
            let name = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
            // error_at_node -> error() has built-in dedup by (start, code).
            // If the diagnostic survived speculation, this is a no-op.
            // If it was lost, this re-emits it.
            self.error_at_node(
                node_idx,
                &format!("Variable '{name}' is used before being assigned."),
                2454,
            );
        }

        // Flush deferred TS2872/TS2873 truthiness diagnostics.
        // These are purely syntactic facts emitted during binary expression
        // evaluation but lost when call-resolution speculation rolls back
        // the main diagnostics vector. The deferred buffer survives rollback.
        // error() has built-in dedup by (start, code): if the diagnostic
        // survived speculation, this is a no-op.
        let deferred_truthiness = std::mem::take(&mut self.ctx.deferred_truthiness_diagnostics);
        for diag in deferred_truthiness {
            self.ctx
                .error(diag.start, diag.length, diag.message_text, diag.code);
        }

        self.ctx.diagnostics.retain(|diag| {
            diag.code != tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || !is_nested_same_wrapper_assignability_message(&diag.message_text)
        });
    }

    fn has_ts_nocheck_pragma(&self, source: &str) -> bool {
        source
            .lines()
            .take(20)
            .any(|line| line.contains("@ts-nocheck"))
    }

    // =========================================================================
    // Reserved Await Identifier Checking (TS1262)
    // =========================================================================

    fn check_reserved_await_identifier_in_module(&mut self, source_file_idx: NodeIndex) {
        let Some(source_file_node) = self.ctx.arena.get(source_file_idx) else {
            return;
        };
        let Some(source_file) = self.ctx.arena.get_source_file(source_file_node) else {
            return;
        };

        let is_declaration_file = source_file.is_declaration_file
            || is_declaration_file_name(&source_file.file_name)
            || self.ctx.is_declaration_file();

        if is_declaration_file {
            return;
        }

        let is_external_module = if let Some(ref map) = self.ctx.is_external_module_by_file {
            map.get(&self.ctx.file_name).copied().unwrap_or(false)
        } else {
            self.ctx.binder.is_external_module()
        };

        let has_module_indicator = self.source_file_has_module_indicator(source_file);
        let force_js_module_check = self.is_js_file() && has_module_indicator;

        if !is_external_module && !force_js_module_check {
            return;
        }

        let Some(await_sym_id) = self.ctx.binder.file_locals.get("await") else {
            return;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(await_sym_id) else {
            return;
        };

        let mut candidate_decls = symbol.all_declarations();
        candidate_decls.sort_unstable_by_key(|node| node.0);

        for decl_idx in candidate_decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let is_disallowed_top_level_await_decl = matches!(
                node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::BINDING_ELEMENT
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::IMPORT_CLAUSE
                    | syntax_kind_ext::IMPORT_SPECIFIER
                    | syntax_kind_ext::NAMESPACE_IMPORT
            );
            if !is_disallowed_top_level_await_decl {
                continue;
            }

            let is_plain_await_identifier = self
                .await_identifier_name_node_for_decl(decl_idx)
                .is_some_and(|name_idx| self.is_plain_await_identifier(source_file, name_idx));

            if !is_plain_await_identifier {
                continue;
            }

            let mut current = decl_idx;
            let mut is_top_level = false;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if parent == source_file_idx {
                    is_top_level = true;
                    break;
                }
                current = parent;
            }

            if !is_top_level {
                continue;
            }

            let report_idx = self
                .await_identifier_name_node_for_decl(decl_idx)
                .unwrap_or(decl_idx);
            self.error_at_node(
                report_idx,
                "Identifier expected. 'await' is a reserved word at the top-level of a module.",
                crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
            );
            break;
        }

        self.emit_top_level_await_text_fallback(source_file);
    }

    fn await_identifier_name_node_for_decl(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(decl_idx)?;
        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => self
                .ctx
                .arena
                .get_variable_declaration(node)
                .map(|decl| decl.name),
            syntax_kind_ext::BINDING_ELEMENT => self
                .ctx
                .arena
                .get_binding_element(node)
                .map(|decl| decl.name),
            syntax_kind_ext::FUNCTION_DECLARATION => {
                self.ctx.arena.get_function(node).map(|f| f.name)
            }
            syntax_kind_ext::CLASS_DECLARATION => self.ctx.arena.get_class(node).map(|c| c.name),
            syntax_kind_ext::IMPORT_CLAUSE => self
                .ctx
                .arena
                .get_import_clause(node)
                .map(|clause| clause.name),
            syntax_kind_ext::IMPORT_SPECIFIER => self
                .ctx
                .arena
                .get_specifier(node)
                .map(|specifier| specifier.name),
            syntax_kind_ext::NAMESPACE_IMPORT => self
                .ctx
                .arena
                .get_named_imports(node)
                .map(|named| named.name),
            _ => None,
        }
    }

    fn is_plain_await_identifier(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some((start, end)) = self.get_node_span(node_idx) else {
            return false;
        };

        source_file
            .text
            .get(start as usize..end as usize)
            .is_some_and(|text| text == "await")
    }

    fn source_file_has_module_indicator(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                return false;
            };

            matches!(
                stmt_node.kind,
                syntax_kind_ext::EXPORT_DECLARATION
                    | syntax_kind_ext::EXPORT_ASSIGNMENT
                    | syntax_kind_ext::IMPORT_DECLARATION
            )
        })
    }

    fn emit_ts1262_at_first_await(&mut self, statement_start: u32, statement_text: &str) -> bool {
        let Some(offset) = statement_text.find("await") else {
            return false;
        };

        self.error_at_position(
            statement_start + offset as u32,
            5,
            "Identifier expected. 'await' is a reserved word at the top-level of a module.",
            crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
        );
        true
    }

    fn statement_contains_any(text: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|pattern| text.contains(pattern))
    }

    fn emit_top_level_await_text_fallback(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        let ts1262_code =
            crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE;
        if self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == ts1262_code)
        {
            return;
        }

        let has_module_indicator = self.source_file_has_module_indicator(source_file);
        let is_js_like_file = self.is_js_file();

        let import_patterns = [
            "import await from",
            "import * as await from",
            "import { await } from",
            "import { await as await } from",
        ];
        let binding_pattern_patterns = ["var {await}", "var [await]"];
        let js_variable_patterns = ["const await", "let await", "var await"];

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            let Some((start, end)) = self.get_node_span(stmt_idx) else {
                continue;
            };
            let Some(stmt_text) = source_file.text.get(start as usize..end as usize) else {
                continue;
            };

            match stmt_node.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                    if Self::statement_contains_any(stmt_text, &import_patterns)
                        && self.emit_ts1262_at_first_await(start, stmt_text) =>
                {
                    return;
                }
                syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    let has_await_import_equals = stmt_text.contains("import await =");
                    let is_require_form = stmt_text.contains("require(");
                    if has_await_import_equals
                        && (is_require_form || has_module_indicator)
                        && self.emit_ts1262_at_first_await(start, stmt_text)
                    {
                        return;
                    }
                }
                syntax_kind_ext::VARIABLE_STATEMENT => {
                    let has_binding_pattern_await =
                        Self::statement_contains_any(stmt_text, &binding_pattern_patterns);
                    let has_js_var_await = is_js_like_file
                        && Self::statement_contains_any(stmt_text, &js_variable_patterns);
                    if (has_binding_pattern_await || has_js_var_await)
                        && self.emit_ts1262_at_first_await(start, stmt_text)
                    {
                        return;
                    }
                }
                _ => {}
            }
        }

        if has_module_indicator && let Some(offset) = source_file.text.find("const await") {
            self.error_at_position(
                offset as u32 + 6,
                5,
                "Identifier expected. 'await' is a reserved word at the top-level of a module.",
                ts1262_code,
            );
        }
    }

    /// Check a statement and produce type errors.
    ///
    /// This method delegates to `StatementChecker` for dispatching logic,
    /// while providing actual implementations via the `StatementCheckCallbacks` trait.
    pub(crate) fn check_statement(&mut self, stmt_idx: NodeIndex) {
        StatementChecker::check(stmt_idx, self);
    }

    pub(crate) fn check_statement_with_request(
        &mut self,
        stmt_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        StatementChecker::check_with_request(stmt_idx, self, request);
    }

    /// TS4094: For each `export default <expr>` statement, check whether the
    /// expression's type is an anonymous class constructor.  If so, report TS4094 for
    /// each private/protected member of its instance type.
    ///
    /// This covers patterns like `export default mix(AnonymousClass)` where the call
    /// returns the same anonymous class constructor type that was passed in.
    fn check_ts4094_in_export_assignments(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            // TSZ represents `export default <expr>` as an EXPORT_DECLARATION node with
            // `is_default_export: true`. The TypeScript AST uses ExportAssignment for this,
            // but TSZ's parser collapses both into EXPORT_DECLARATION.
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.ctx.arena.get_export_decl(node).cloned() else {
                continue;
            };
            // Only care about `export default <expr>`, not `export { ... }` or re-exports.
            if !export_decl.is_default_export {
                continue;
            }
            let expr_idx = export_decl.export_clause;
            if expr_idx == tsz_parser::parser::NodeIndex::NONE {
                continue;
            }
            // Skip class/function declarations — they are handled by the class/function
            // checker paths which already emit TS4094 for anonymous class members.
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
            {
                continue;
            }
            // Resolve the expression to an instance type. For patterns like
            // `export default mix(DisposableMixin)` where mix<T>(x:T):T returns the
            // constructor as-is, this yields the anonymous class's instance type.
            let Some(instance_type) = self.base_instance_type_from_expression(expr_idx, None)
            else {
                continue;
            };
            if self.instance_type_is_from_anonymous_class(instance_type) {
                self.report_instance_type_private_members_as_ts4094(stmt_idx, instance_type);
            }
        }
    }

    fn recheck_checked_js_import_diagnostics(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import) = self.ctx.arena.get_import_decl(node).cloned() else {
                continue;
            };
            let Some(spec_node) = self.ctx.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
                continue;
            };
            if !self.module_exists_cross_file(&literal.text) {
                continue;
            }

            self.check_imported_members(&import, &literal.text);
        }
    }
}

fn is_nested_same_wrapper_assignability_message(message: &str) -> bool {
    fn generic_head(display: &str) -> Option<&str> {
        display.split_once('<').map(|(head, _)| head.trim())
    }

    let Some(source_rest) = message.strip_prefix("Type '") else {
        return false;
    };
    let Some(source_end) = source_rest.find('\'') else {
        return false;
    };
    let source = &source_rest[..source_end];
    let Some(target_start) = message.find("' is not assignable to type '") else {
        return false;
    };
    let target_rest = &message[target_start + "' is not assignable to type '".len()..];
    let Some(target_end) = target_rest.find('\'') else {
        return false;
    };
    let target = &target_rest[..target_end];

    let Some(source_head) = generic_head(source) else {
        return false;
    };
    if generic_head(target) != Some(source_head) {
        return false;
    }
    let Some((_, source_args)) = source.split_once('<') else {
        return false;
    };
    let prefix = format!("{source_head}<");
    // Source must be Wrapper<Wrapper<...>> (source arg starts with the same head)
    if !source_args.trim_start().starts_with(&prefix) {
        return false;
    }
    // Only suppress when the target arg does NOT also start with the same wrapper.
    // e.g., PromiseLike<PromiseLike<T>> vs PromiseLike<T> → suppress (target arg = T)
    // but Box<Box<number>> vs Box<Box<string>> → keep (target arg starts with Box<)
    let target_args = target.split_once('<').map(|(_, rest)| rest).unwrap_or("");
    !target_args.trim_start().starts_with(&prefix)
}
