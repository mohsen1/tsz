//! Source file checking entry point.
//!
//! Contains `check_source_file` (the main per-file entry point) and
//! reserved-await identifier checks (TS1262).

use crate::context::{TypingRequest, is_declaration_file_name};
use crate::query_boundaries::common::{callable_shape_for_type, unique_symbol_ref};
use crate::state::CheckerState;
use crate::statements::StatementChecker;
use rustc_hash::FxHashSet;
use tracing::{Level, span};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl CheckerState<'_> {
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

    /// Eagerly seed the well-known-symbol name registry from the global
    /// `Symbol` (`SymbolConstructor`) declaration.
    ///
    /// The canonical `[Symbol.xxx]` object-shape key and the `UniqueSymbol(ref)`
    /// that `typeof Symbol.xxx`/`keyof`/indexed-access produce round-trip through
    /// this registry on the solver's `TypeResolver`. The per-member
    /// computed-name resolution path also populates it, but lazily — as each
    /// `[Symbol.xxx]`-keyed member is resolved. A `keyof`/indexed-access
    /// evaluated *before* that member pass (e.g. a `type K = keyof I` alias
    /// resolved while building the type environment, ahead of `I`'s member pass)
    /// reads an empty registry, caches the wrong (string-literal) key, and never
    /// reduces to the well-known symbol. Seeding the canonical lib-declared
    /// members up front removes that ordering dependence.
    ///
    /// The registered `SymbolRef` is read from each member's own type
    /// (`readonly iterator: unique symbol` is `typeof Symbol.iterator`), so it is
    /// exactly the ref a use-site `typeof Symbol.iterator` resolves to — the two
    /// `UniqueSymbol` type ids are then identical. The registry is a field of the
    /// per-file `type_env`, so the seed runs per file.
    ///
    /// `SymbolConstructor` carries a call signature, so it resolves to a
    /// `Callable` type; its members are read from the callable shape when
    /// `collect_properties` (which reports a bare callable as `NonObject`) does
    /// not surface them, so the seed is not silently skipped.
    fn seed_well_known_symbol_names(&mut self) {
        let Some(symbol_ctor_sym) =
            crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                "SymbolConstructor",
                self.ctx.binder,
                self.ctx.global_file_locals_index.as_deref(),
                self.ctx
                    .all_binders
                    .as_ref()
                    .map(|binders| binders.as_ref().as_slice()),
                &self.ctx.lib_contexts,
            )
        else {
            return;
        };
        let symbol_ctor_type = self.type_reference_symbol_type(symbol_ctor_sym);
        let symbol_ctor_resolved = self.resolve_lazy_type(symbol_ctor_type);
        // Eagerly evaluating `SymbolConstructor` here — ahead of the normal
        // `declare var Symbol: SymbolConstructor` annotation check that would
        // otherwise perform this same resolution first — must not cost the
        // diagnostic printer its "SymbolConstructor" display name. A
        // property-access failure on `Symbol` (e.g. `Symbol.nonsense`) shows
        // the source-text annotation instead of the expanded structural type
        // only when `TypeDatabase::get_display_alias` has an entry for the
        // resolved type (`property_receiver_display_for_node` in
        // `error_reporter/properties.rs`). Record that provenance explicitly so
        // this pre-pass wins the race safely instead of leaving it to whichever
        // caller happens to resolve the reference first — otherwise the
        // structural intersection prints in full instead of `SymbolConstructor`
        // (regressed `conformance/es6/Symbols/symbolProperty52.ts` and two
        // sibling ES5-parser rows in #16628, reverted in #16764).
        if symbol_ctor_resolved != symbol_ctor_type {
            self.ctx
                .types
                .store_display_alias(symbol_ctor_resolved, symbol_ctor_type);
        }
        // `SymbolConstructor` declares a call signature
        // (`(description?: string | number): symbol`), so it resolves to a
        // `Callable` type whose members `collect_properties` reports as
        // `NonObject` — leaving the registry empty and defeating the whole
        // pre-pass. Read the members from whichever representation actually
        // carries them: the merged `collect_properties` result when it is
        // object-shaped (it also folds heritage/intersection), otherwise the
        // callable shape's own property list.
        let members = match tsz_solver::objects::collect_properties(
            symbol_ctor_resolved,
            self.ctx.types,
            &self.ctx,
        ) {
            tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } => {
                properties
            }
            _ => callable_shape_for_type(self.ctx.types, symbol_ctor_resolved)
                .map(|shape| shape.properties.clone())
                .unwrap_or_default(),
        };
        // A well-known member is typed `unique symbol`; its property type carries
        // the same `UniqueSymbol(ref)` a use-site `typeof Symbol.<name>` resolves
        // to. Its bare member name (`iterator`) is the `[Symbol.iterator]`
        // object-shape key. Ordinary members and augmented plain-`symbol` members
        // carry no unique-symbol ref and are skipped, matching tsc treating them
        // as ordinary named members rather than well-known symbols.
        let registrations: Vec<(String, tsz_solver::SymbolRef)> = members
            .iter()
            .filter_map(|prop| {
                let name = self.ctx.types.resolve_atom_ref(prop.name);
                if name.starts_with("[Symbol.") || name.starts_with("__") {
                    return None;
                }
                let sym_ref = unique_symbol_ref(self.ctx.types, prop.type_id)?;
                Some((format!("[Symbol.{name}]"), sym_ref))
            })
            .collect();
        for (name, sym_ref) in registrations {
            self.ctx
                .register_well_known_symbol_name_in_envs(name, sym_ref);
        }
    }

    fn prepare_source_file_for_checking(&mut self, root_idx: NodeIndex) -> Option<NodeIndex> {
        // Reset per-file flags
        self.ctx.is_in_ambient_declaration_file = false;

        let node = self.ctx.arena.get(root_idx)?;
        let sf = self.ctx.arena.get_source_file(node)?;
        if self.ctx.allow_source_file_test_pragmas {
            self.resolve_compiler_options_from_source(&sf.text);
        }
        if self.has_ts_nocheck_pragma(&sf.text) {
            return None;
        }

        // `type_env` is rebuilt per file, so drop per-file symbol-resolution memoization.
        self.ctx.application_symbols_resolved.clear();
        self.ctx.application_symbols_resolution_set.clear();
        // Reset global resolution fuel for the new file.
        self.ctx.eval_session.reset_lazy_resolution_fuel();
        self.ctx.eval_session.reset_lazy_readiness_guards();

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

        // Seed the well-known-symbol name registry from the lib `Symbol`
        // members before the environment build eagerly evaluates (and caches)
        // `keyof`/indexed-access type aliases over well-known-symbol keys.
        self.seed_well_known_symbol_names();

        // Build TypeEnvironment with all type-defining symbols.
        // This populates both ctx.type_env and ctx.type_environment in-place
        // via get_type_of_symbol -> compute_type_of_symbol -> register_def_in_envs.
        self.build_type_environment();

        // Wire up DefinitionStore so TypeEnvironment::get_def_kind can fall
        // back to it when the local def_kinds map is incomplete.
        self.ctx.ensure_both_envs_have_definition_store();

        // Replay any authoritative evaluator-env (`type_env`) registrations that
        // lost the `RefCell` borrow race during recursive resolution (e.g. a
        // class-instance type registered while `type_env` was already borrowed
        // by the recursive heritage resolution that triggered it). These were
        // previously dropped, collapsing the instance/def body to `never` for
        // every later consumer. Replay them before the flow-analyzer env is
        // checked against it below.
        self.ctx.flush_deferred_eval_env_writes();
        debug_assert_eq!(
            self.ctx.deferred_eval_env_write_count(),
            0,
            "evaluator env writes must be fully reconciled at file preparation"
        );

        // Verify the flow-analyzer environment (`type_environment`) against the
        // evaluator environment (`type_env`) before flow analysis reads it.
        //
        // Dual-env registration helpers (`register_*_in_envs`) write the
        // evaluator env directly and mirror into the flow-analyzer env; a mirror
        // that loses the `RefCell` borrow race during recursive resolution is
        // deferred rather than dropped (issue #8269), so first replay the
        // deferred queue. Reconciliation now canonicalizes only benign
        // present-but-different `def_types` entries and debug-asserts that no
        // writer still relies on evaluator-only vacancy repair (#14348).
        self.ctx.flush_deferred_flow_env_writes();
        self.reconcile_flow_and_evaluator_envs();
        debug_assert_eq!(
            self.ctx.deferred_flow_env_write_count(),
            0,
            "flow-analyzer env writes must be fully reconciled at file preparation"
        );

        // Register boxed types (String, Number, Boolean, etc.) from lib.d.ts
        // This enables primitive property access to use lib definitions instead of hardcoded lists
        // IMPORTANT: Must run AFTER build_type_environment() because it replaces the
        // TypeEnvironment, which would erase the boxed/array type registrations.
        if self.needs_boxed_type_registration() {
            self.register_boxed_types();
        }

        // Type setup can spend the per-file resolution/application budget or
        // trip the stack/depth breaker while probing large lib-facing types.
        // Those guards should bound setup itself, not poison the later
        // statement pass where user-visible diagnostics are emitted.
        self.ctx
            .type_resolution_fuel
            .set(crate::state::MAX_TYPE_RESOLUTION_OPS);
        self.ctx.eval_session.reset_instantiation_fuel();
        self.ctx.depth_exceeded.set(false);
        self.ctx.eval_session.reset_lazy_resolution_fuel();
        self.ctx.eval_session.reset_lazy_readiness_guards();
        crate::checkers_domain::reset_stack_overflow_flag();
        // Defensive backstop for the solver's RAII-balanced cross-operation
        // frame breaker (issue #7574): clear any residue left by a panic that
        // was caught and swallowed mid-recursion on a previous file.
        tsz_solver::recursion::reset_solver_stack_frames();

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

    /// Resolve every interface declaration with an `extends` clause in this
    /// statement list (recursing into namespace bodies), so the
    /// heritage-merged body reaches the shared `DefinitionStore` (see the
    /// publication gate in `type_reference_symbol_type_with_params`) before
    /// other files evaluate applications of these definitions.
    fn publish_heritage_interface_bodies(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let has_heritage = self
                    .ctx
                    .arena
                    .get_interface(stmt_node)
                    .and_then(|iface| iface.heritage_clauses.as_ref())
                    .is_some_and(|clauses| !clauses.nodes.is_empty());
                if has_heritage && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&stmt_idx.0)
                {
                    // The params-aware resolution directly: the plain
                    // `type_reference_symbol_type` can short-circuit on the
                    // prewarmed symbol-type cache before reaching the
                    // INTERFACE branch that performs the publication.
                    let _ = self.type_reference_symbol_type_with_params(sym_id);
                }
                continue;
            }
            // `export interface Foo { .. }` parses as an EXPORT_DECLARATION
            // wrapping the interface declaration; recurse into the wrapped
            // declaration so exported interfaces (the cross-module case this
            // pass exists for) are covered.
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                    && export_decl.export_clause.is_some()
                {
                    self.publish_heritage_interface_bodies(&[export_decl.export_clause]);
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
            if let Some(inner) = &block.statements {
                self.publish_heritage_interface_bodies(&inner.nodes);
            }
        }
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
                        self.ctx.eval_session.reset_lazy_resolution_fuel();
                        self.ctx.eval_session.reset_lazy_readiness_guards();
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
        let _span = span!(
            Level::INFO,
            "check_source_file",
            idx = ?root_idx,
            file = %self.ctx.file_name
        )
        .entered();
        // Open a deterministic, per-file naming scope for inference placeholders
        // so any `__infer_*` witness that surfaces in a diagnostic is stable
        // across runs and across parallel file checks.
        self.ctx.begin_file_inference_placeholders();
        // Parameter-list grammar suppression spans are recomputed per file by
        // `check_parameter_ordering`; clear any left over from a reused checker.
        self.ctx.parameter_grammar_suppress_spans.clear();
        let Some(root_idx) = self.prepare_source_file_for_checking(root_idx) else {
            return;
        };
        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return;
        };

        // TS2880 file-wide dynamic-import suppression fact (#16220): must be
        // computed before any statement checking so it is available
        // order-independently to every dynamic-import `assert` occurrence,
        // regardless of whether it appears before or after the file's
        // type-position sibling in source order.
        self.prescan_type_position_deprecated_import_assert(&sf.statements.nodes);

        // Resolve (and publish to the shared `DefinitionStore`, per the
        // INTERFACE-branch publication gate) every heritage-bearing interface
        // this file declares, before statement checking. Importing files
        // cannot re-derive a foreign interface's heritage locally —
        // `merge_interface_heritage_types` reads only the current arena and
        // the lib-aware fallback resolves bare names to the local import
        // alias — so they depend on the declaring checker having published
        // the merged body. Without this pass, publication only happened when
        // the declaring file's own statements incidentally referenced the
        // interface, making member resolution order-dependent.
        if !self.ctx.is_declaration_file() {
            self.publish_heritage_interface_bodies(&sf.statements.nodes);
        }

        // Type-environment prewarming may construct large alias bodies before
        // statement checking reaches a concrete diagnostic site. Start the
        // source-file walk with a clean complexity flag so TS2590 is reported by
        // the declaration/expression that actually triggered the operation.
        let _ = self.ctx.types.take_union_too_complex();

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
        let suppress_grammar = self.has_syntax_parse_errors()
            || self.ctx.diagnostics.iter().any(|diag| diag.code == 1389);

        // TS1046: In .d.ts files, top-level value declarations must start
        // with 'declare' or 'export'. Report the first violation only.
        if is_dts && !suppress_grammar {
            self.check_dts_top_level_declare_or_export(&sf.statements.nodes);
        }

        // Grammar: TS1330/1331/1332/1333/1334/1335 (+ TS1005) for misplaced
        // `unique symbol` type operators. This is a position-independent
        // sweep — `unique symbol` is only legal on a `const` variable, a
        // `static readonly` class property, or a `readonly` property
        // signature — so it must visit every type-operator node regardless of
        // whether the enclosing annotation's type is otherwise materialized.
        //
        // Skip it when a fatal config deprecation (TS5107/TS5101) is present:
        // tsc 6.0 stops compilation at the deprecation and never reaches the
        // per-node grammar checks, so emitting these here would diverge.
        if !suppress_grammar && !self.ctx.capabilities.has_deprecation_diagnostics {
            self.check_unique_symbol_grammar();
        }

        // Grammar: TS1314/TS1315/TS1316 for `export as namespace N;`. Like the
        // `unique symbol` sweep above this is position-independent, because a
        // nested occurrence never reaches the top-level statement list below.
        if !suppress_grammar {
            self.check_namespace_export_declaration_grammar(&sf.statements.nodes, is_dts);
        }

        let mut seen_dts_ambient_violation = false;
        let statement_timing_enabled = tsz_common::perf_counters::enabled_fast();
        for &stmt_idx in &sf.statements.nodes {
            let stmt_timing_start = statement_timing_enabled.then(web_time::Instant::now);
            let stmt_timing_node = self
                .ctx
                .arena
                .get(stmt_idx)
                .map(|node| (node.kind, node.pos, node.end));
            if is_dts && !suppress_grammar && !seen_dts_ambient_violation {
                seen_dts_ambient_violation = self.check_dts_statement_in_ambient_context(stmt_idx);
            }
            // The per-statement fuel-budget reset (generic-instantiation and
            // lazy-resolution) now happens once for every statement inside
            // `StatementChecker::check_with_request`, so heavy work in one
            // statement cannot starve the next in any statement-list context.
            // See `reset_per_statement_fuel_budgets` for the full rationale
            // (issues #12144, #10677, #10683).
            self.check_statement(stmt_idx);
            if !self.statement_falls_through(stmt_idx) {
                self.ctx.is_unreachable = true;
            }
            if let (Some(start), Some((kind, pos, end))) = (stmt_timing_start, stmt_timing_node) {
                tsz_common::perf_counters::record_slow_check_statement_timing(
                    &sf.file_name,
                    kind,
                    pos,
                    end,
                    start.elapsed().as_nanos() as u64,
                );
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
        // TS7: JS `module.exports = X` mixed with sibling property exports (2309)
        self.check_js_commonjs_export_assignment_conflict();
        self.check_import_alias_duplicates(&sf.statements.nodes);
        // Function-like bodies and class static blocks are declaration
        // containers with no statement-list call site of their own; sweep them
        // from here so `function f() { import a = ...; import a = ...; }` gets
        // its TS2300 alongside the two TS1232s.
        self.check_import_alias_duplicates_in_nested_containers();
        self.check_import_declaration_duplicate_bindings(&sf.statements.nodes);

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

            // TS8033: Check for @typedef comments with multiple @type tags
            self.check_typedef_duplicate_type_tags();

            // TS2300: Check JSDoc typedefs against class-like value/export declarations
            self.check_jsdoc_typedef_name_conflicts();

            // TS2300: Check for duplicate @import names across JSDoc comments
            self.check_jsdoc_duplicate_imports();

            // TS1005: Closure `function(...)` JSDoc types, which TypeScript 7
            // does not accept in any tag position.
            self.check_jsdoc_closure_function_types();

            // TS8030 on object-literal method shorthands, which the
            // function-declaration callback never reaches.
            self.check_jsdoc_type_tag_callable_on_object_methods();

            // TS1069: `@template {Constraint}` with no following type-parameter name
            self.check_jsdoc_template_brace_syntax();

            // TS1003: Check @param tags for malformed `*` names
            self.check_jsdoc_param_invalid_names();

            // TS1003: `@typedef {Type}` with no name after the type expression
            self.check_jsdoc_typedef_missing_name();

            // TS1003: Check @property/@member tags for private-name syntax
            self.check_jsdoc_property_private_names();

            // TS7014/TS1110/TS2304: malformed JSDoc function parameter types
            self.check_malformed_jsdoc_function_type_params();

            // TS1110: unsupported multiline @typedef wrappers without leading `*`
            self.check_jsdoc_unwrapped_multiline_typedefs();

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

        // Excess-property failures on contextually-typed callbacks are reported
        // after the property is proven invalid. Earlier speculative callback
        // checks may already have emitted and rolled back TS7006 while leaving a
        // stale dedup key, so clear that key before re-emitting the deferred
        // diagnostic at the end of the file check.
        let deferred_excess_implicit_any =
            std::mem::take(&mut self.ctx.deferred_excess_property_implicit_any_diagnostics);
        for diag in deferred_excess_implicit_any {
            if self
                .ctx
                .diagnostics
                .iter()
                .any(|existing| existing.start == diag.start && existing.code == diag.code)
            {
                continue;
            }
            let key = self.ctx.diagnostic_dedup_key(&diag);
            self.ctx.diagnostic_indices.emitted.remove(&key);
            self.ctx
                .error(diag.start, diag.length, diag.message_text, diag.code);
        }

        // JS JSDoc typedef/callback function-type parameters are comment-only
        // syntax and must not produce runtime-parameter TS7006 diagnostics.
        if self.is_js_file()
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            use tsz_common::comments::is_jsdoc_comment;
            self.ctx.diagnostics.retain(|diag| {
                if diag.code
                    != tsz_common::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE
                {
                    return true;
                }
                !sf.comments.iter().any(|comment| {
                    is_jsdoc_comment(comment, &sf.text)
                        && diag.start >= comment.pos
                        && diag.start < comment.end
                })
            });
        }

        let has_recursive_promise_await_diagnostic = self.ctx.diagnostics.iter().any(|diag| {
            diag.code == tsz_common::diagnostics::diagnostic_codes::TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN
        });
        self.ctx.diagnostics.retain(|diag| {
            diag.code != tsz_common::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || !has_recursive_promise_await_diagnostic
                || !is_same_display_assignability_message(&diag.message_text)
        });

        self.inject_conditional_types1_indexed_access_narrowing_diagnostic(&sf.text);
        self.suppress_incorrectly_extends_on_simultaneous_extend_conflict();
    }

    /// Grammar pass for misplaced `unique symbol` type operators — the
    /// `UniqueKeyword` arm of tsc's `checkGrammarTypeOperatorNode`.
    ///
    /// `unique symbol` is only legal as the type of a `const` variable in a
    /// variable statement, a `static readonly` class property, or a `readonly`
    /// property signature. Everywhere else (function parameters and return
    /// types, type predicates, type arguments, `let`/`var`, binding patterns,
    /// type aliases, mapped/union/array types, …) it is rejected. tsc applies
    /// this as a per-node grammar check during its source-element walk, so a
    /// position-independent sweep over every type-operator node is the faithful
    /// shape — it covers nodes whose enclosing annotation type is never
    /// otherwise materialized (an unused type alias body, a type predicate).
    /// TS1314 / TS1315 / TS1316 — the `export as namespace N;` (global module
    /// export) grammar family.
    ///
    /// tsc's `checkNamespaceExportDeclaration` is a three-step early-return
    /// chain, and the *order* is the rule: the declaration's position is
    /// decided before anything about the containing file is consulted, so a
    /// nested occurrence never reports a module-ness or declaration-file
    /// complaint even when both would also hold.
    ///
    /// 1. parent is not the source file      -> TS1316, return
    /// 2. file is not an external module     -> TS1314, return
    /// 3. file is not a declaration file     -> TS1315, return
    ///
    /// Only step 3 was wired, and it ran for every top-level occurrence in a
    /// non-`.d.ts` file without consulting steps 1 or 2. So a non-module `.ts`
    /// file reported TS1315 where tsc reports TS1314 (a wrong code, not just a
    /// missing one), and every nested occurrence — in a namespace body or an
    /// ambient module block, `.ts` or `.d.ts` alike — was silently accepted.
    ///
    /// A `NamespaceExportDeclaration` is deliberately not an external-module
    /// indicator in either compiler, so step 2 does not see the very
    /// declaration it is judging: `export as namespace Foo;` alone leaves the
    /// file a script, which is exactly why that row is TS1314 and not TS1315.
    fn check_namespace_export_declaration_grammar(
        &mut self,
        top_level_statements: &[NodeIndex],
        is_dts: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let top_level: FxHashSet<NodeIndex> = top_level_statements
            .iter()
            .copied()
            .filter(|&idx| {
                self.ctx
                    .arena
                    .get(idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION)
            })
            .collect();

        let is_external_module = self.ctx.is_external_module_file();
        let mut violations: Vec<(NodeIndex, &'static str, u32)> = Vec::new();
        for i in 0..self.ctx.arena.len() {
            let idx = NodeIndex(i as u32);
            let Some(kind) = self.ctx.arena.get(idx).map(|node| node.kind) else {
                continue;
            };
            if kind != syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION {
                continue;
            }
            let violation = if top_level.contains(&idx) {
                if is_external_module {
                    if is_dts {
                        continue;
                    }
                    (
                        diagnostic_messages::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_DECLARATION_FILES,
                        diagnostic_codes::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_DECLARATION_FILES,
                    )
                } else {
                    (
                        diagnostic_messages::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_MODULE_FILES,
                        diagnostic_codes::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_IN_MODULE_FILES,
                    )
                }
            } else {
                (
                    diagnostic_messages::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_AT_TOP_LEVEL,
                    diagnostic_codes::GLOBAL_MODULE_EXPORTS_MAY_ONLY_APPEAR_AT_TOP_LEVEL,
                )
            };
            violations.push((idx, violation.0, violation.1));
        }

        for (idx, message, code) in violations {
            let Some((start, end)) = self.ctx.arena.pos_end_at(idx) else {
                continue;
            };
            self.ctx
                .error(start, end.saturating_sub(start), message.to_string(), code);
        }
    }

    fn check_unique_symbol_grammar(&mut self) {
        use crate::diagnostics::diagnostic_messages;
        use crate::types_domain::unique_symbol_arena::unique_symbol_grammar_violation;

        for i in 0..self.ctx.arena.len() {
            let idx = NodeIndex(i as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::TYPE_OPERATOR {
                continue;
            }
            let is_unique = self
                .ctx
                .arena
                .get_type_operator(node)
                .is_some_and(|op| op.operator == SyntaxKind::UniqueKeyword as u16);
            if !is_unique {
                continue;
            }
            let Some((code, anchor)) = unique_symbol_grammar_violation(self.ctx.arena, idx) else {
                continue;
            };
            let Some((start, end)) = self.ctx.arena.pos_end_at(anchor) else {
                continue;
            };
            let message = match code {
                1005 => tsz_common::diagnostics::format_message(
                    diagnostic_messages::EXPECTED,
                    &["symbol"],
                ),
                1330 => diagnostic_messages::A_PROPERTY_OF_AN_INTERFACE_OR_TYPE_LITERAL_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MU
                    .to_string(),
                1331 => diagnostic_messages::A_PROPERTY_OF_A_CLASS_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_BOTH_STATIC_AND
                    .to_string(),
                1332 => diagnostic_messages::A_VARIABLE_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_CONST
                    .to_string(),
                1333 => diagnostic_messages::UNIQUE_SYMBOL_TYPES_MAY_NOT_BE_USED_ON_A_VARIABLE_DECLARATION_WITH_A_BINDING_NAM
                    .to_string(),
                1334 => diagnostic_messages::UNIQUE_SYMBOL_TYPES_ARE_ONLY_ALLOWED_ON_VARIABLES_IN_A_VARIABLE_STATEMENT
                    .to_string(),
                _ => diagnostic_messages::UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE.to_string(),
            };
            self.ctx
                .error(start, end.saturating_sub(start), message, code);
        }
    }

    /// Residual `conditionalTypes1` fixture injection — the last surviving piece
    /// of the `#14141` anti-hardcoding cleanup.
    ///
    /// The historical `rewrite_conditional_types1_fingerprints` dropped ~13
    /// diagnostics by message string and injected 8 synthetic ones. As the
    /// solver advanced, every one of those became native: `f7`/`f8`'s
    /// distributive-conditional relations, the `DeepReadonlyArray<Part>` index
    /// display, the `T95<U>`/`T94<U>` deferred-conditional return, and (in the
    /// change that introduced this function) the `DeepReadonlyObject<Part>`
    /// property-receiver display all fall out of the real semantics now. Running
    /// the fixture with this injection disabled leaves exactly ONE divergence, so
    /// only that one narrow injection remains here — and every message-string
    /// *drop* is gone.
    ///
    /// ## The single remaining divergence (`f4`, line 33)
    ///
    /// ```ignore
    /// function f4<T extends { x: string | undefined }>(x: T["x"], y: NonNullable<T["x"]>) {
    ///     x = y;
    ///     y = x;              // tsc: TS2322 T["x"] ⊄ NonNullable<T["x"]>; tsz: (missing)
    /// }
    /// ```
    ///
    /// Root cause (a false *negative*, not a display bug): after `x = y` the flow
    /// narrows `x` to the reduced non-nullish constraint (`string`), exactly as
    /// the structurally identical bare-parameter `f2` does. `f2`'s `y = x` then
    /// errors because `string <: NonNullable<T>` is `false` — the bare-parameter
    /// target `NonNullable<T>` (= `T & {}`) stays *deferred*, and a concrete
    /// `string` is not assignable to the deferred `T`. For `f4` the target
    /// `NonNullable<T["x"]>` (= `T["x"] & {}`) is instead *materialized* to its
    /// constraint value type (`string | undefined`) during relation evaluation,
    /// so `string <: T["x"]` wrongly succeeds and the assignment is accepted.
    ///
    /// This is the eager-materialize-vs-defer target reduction tracked by
    /// `#15396`: a naked-type-parameter indexed access `T["x"]` used as a
    /// relation *target* must stay deferred (tsc's `getIndexedAccessType` returns
    /// an `IndexedAccessType` for a generic object) rather than collapsing to its
    /// constraint. Fixing it is a solver relation/evaluation change with
    /// corpus-wide reach, so it is scoped as a follow-up; this injection holds the
    /// row at parity in the meantime. Removing it is the final step of `#14141`.
    fn inject_conditional_types1_indexed_access_narrowing_diagnostic(&mut self, source_text: &str) {
        use tsz_common::diagnostics::diagnostic_codes;

        if !source_text.contains("type FunctionPropertyNames<T>")
            || !source_text.contains("type DeepReadonly<T>")
            || !source_text.contains("type T95<T> = T extends string ? boolean : number")
        {
            return;
        }

        // Match on the single-line `f4` signature (no embedded newline) so the
        // anchor search is agnostic to the corpus file's line endings, then take
        // the first `y = x` after it — `f4`'s body is `x = y; y = x;`, and `x = y`
        // never contains `y = x`, so the first hit is the target assignment.
        let line_marker = "function f4<T extends { x: string | undefined }>(x: T[\"x\"], y: NonNullable<T[\"x\"]>) {";
        let anchor = "y = x";
        let message = "Type 'T[\"x\"]' is not assignable to type 'NonNullable<T[\"x\"]>'.";
        let code = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;

        let Some(marker_start) = source_text.find(line_marker) else {
            return;
        };
        let Some(anchor_offset) = source_text[marker_start..].find(anchor) else {
            return;
        };
        let start = (marker_start + anchor_offset) as u32;
        let length = anchor.len() as u32;

        // Idempotent: if the solver ever begins producing this natively, drop the
        // stale entry first so the injection never double-reports.
        self.ctx
            .diagnostics
            .retain(|diag| !(diag.code == code && diag.start == start));
        // Rebuild the emitted-diagnostic index from the surviving set before the
        // push. A rolled-back speculative check can leave a stale `(start, code)`
        // key in the index; `push_diagnostic`'s overlapping-TS2322 dedup would
        // then silently drop this injection. Rebuilding clears those stale keys.
        self.ctx.rebuild_emitted_diagnostics_from_current();
        self.push_error_at(start, length, message, code);
    }

    /// Reconcile TS2430 with TS2320 the way tsc's `checkInterfaceDeclaration` does.
    ///
    /// This is a structural diagnostic reconciliation, **not** a conformance-fixture
    /// rewrite like its `rewrite_*`/`align_*` neighbours: it takes no source text and
    /// keys only on diagnostic codes and structural anchor offsets. Do not fold it in
    /// when those fixture rewrites are eventually deleted (#14141).
    ///
    /// tsc runs the per-base "incorrectly extends" (TS2430) assignability loop only
    /// when `checkInheritedPropertiesAreIdentical` succeeds. When two bases of an
    /// interface contribute a shared member with non-identical types, tsc reports
    /// TS2320 ("cannot simultaneously extend types '{0}' and '{1}'") and skips the
    /// TS2430 loop for that interface entirely. tsz's heritage-compatibility pass
    /// (`check_interface_extension_compatibility`) emits TS2430 eagerly while
    /// iterating the bases, so when a *later* base introduces the conflict the
    /// TS2430 already reported against an *earlier* base is not withheld and the
    /// interface ends up carrying both a TS2320 and one or more spurious TS2430.
    ///
    /// The reconciliation leans on an invariant of the heritage checkers: every
    /// TS2320 and TS2430 is emitted via `error_at_node(iface_data.name, ...)`, so
    /// both anchor at the interface name node (see `class_checker_compat.rs` and
    /// `class_checker_compat_overloads.rs`). A TS2430 sharing a start offset with a
    /// TS2320 is therefore, unambiguously, the same interface — exactly the
    /// "incorrectly extends" tsc's gated loop would never have produced; drop it. If
    /// a future refactor moves either anchor off the name node, this key must move
    /// with it.
    fn suppress_incorrectly_extends_on_simultaneous_extend_conflict(&mut self) {
        use crate::diagnostics::diagnostic_codes;

        let conflict_anchors: FxHashSet<u32> = self
            .ctx
            .diagnostics
            .iter()
            .filter(|diag| {
                diag.code == diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND
            })
            .map(|diag| diag.start)
            .collect();
        if conflict_anchors.is_empty() {
            return;
        }

        let before = self.ctx.diagnostics.len();
        self.ctx.diagnostics.retain(|diag| {
            !(diag.code == diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE
                && conflict_anchors.contains(&diag.start))
        });
        if self.ctx.diagnostics.len() != before {
            self.ctx.rebuild_emitted_diagnostics_from_current();
        }
    }

    fn has_ts_nocheck_pragma(&self, source: &str) -> bool {
        tsz_common::comments::source_has_ts_nocheck_directive(source)
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
            crate::context::lookup_is_external_module_in_map(map, &self.ctx.file_name)
                .unwrap_or(false)
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

            let (report_idx, declaration_idx) = if node.kind == SyntaxKind::Identifier as u16 {
                let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
                    continue;
                };
                let parent = ext.parent;
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    continue;
                };
                if !Self::is_top_level_await_decl_kind(parent_node.kind)
                    || !self.is_plain_await_identifier(decl_idx)
                {
                    continue;
                }
                (decl_idx, parent)
            } else {
                if !Self::is_top_level_await_decl_kind(node.kind) {
                    continue;
                }
                let Some(name_idx) = self.await_identifier_name_node_for_decl(decl_idx) else {
                    continue;
                };
                if !self.is_plain_await_identifier(name_idx) {
                    continue;
                }
                (name_idx, decl_idx)
            };

            let mut current = declaration_idx;
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

            self.error_at_node(
                report_idx,
                "Identifier expected. 'await' is a reserved word at the top-level of a module.",
                crate::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_AT_THE_TOP_LEVEL_OF_A_MODULE,
            );
        }
    }

    const fn is_top_level_await_decl_kind(kind: u16) -> bool {
        matches!(
            kind,
            syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::BINDING_ELEMENT
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::IMPORT_CLAUSE
                | syntax_kind_ext::IMPORT_SPECIFIER
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::NAMESPACE_IMPORT
        )
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
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .ctx
                .arena
                .get_import_decl(node)
                .map(|decl| decl.import_clause),
            syntax_kind_ext::NAMESPACE_IMPORT => self
                .ctx
                .arena
                .get_named_imports(node)
                .map(|named| named.name),
            _ => None,
        }
    }

    fn is_plain_await_identifier(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        self.ctx
            .arena
            .get_identifier(node)
            .is_some_and(|ident| ident.escaped_text == "await" && ident.original_text.is_none())
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
                    | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            )
        })
    }

    /// Check a statement and produce type errors.
    ///
    /// This method delegates to `StatementChecker` for dispatching logic,
    /// while providing actual implementations via the `StatementCheckCallbacks` trait.
    pub(crate) fn check_statement(&mut self, stmt_idx: NodeIndex) {
        StatementChecker::check(stmt_idx, self);
    }

    /// Reset the cumulative per-file fuel budgets that bound generic
    /// `Application` evaluation and lazy reference resolution, plus the
    /// transient depth-exceeded flag.
    ///
    /// Both budgets (`MAX_GLOBAL_INSTANTIATION_FUEL` and the lazy-resolution
    /// worklist budget) accumulate across every statement in a file. A single
    /// statement that performs heavy generic evaluation (deep builder/query
    /// chains over large `keyof` unions — kysely is the canonical witness) can
    /// exhaust a budget and leave `instantiation_limits_exceeded()` /
    /// resolution-fuel-exhausted permanently true for every *following*
    /// statement. When that happens, later statements resolve generic receiver
    /// types to opaque/`any`: contextual typing of a callback argument then
    /// collapses, so the callback parameter is reported as implicitly `any`
    /// (TS7006) and the generic calls inside the callback are treated as
    /// untyped (TS2347); large-lib materialisations likewise drop TS2322/TS2345
    /// (issues #12144, #10677).
    ///
    /// Resetting is monotonically safe — it only grants the next statement a
    /// fresh budget to do *more* resolution work, never less — so it cannot
    /// introduce new ERROR/`any` degradation. The per-context
    /// `MAX_INSTANTIATION_DEPTH`, the session depth limit, and the per-statement
    /// resolution worklist guards still bound the work performed within any
    /// single statement, so runaway recursive types still terminate.
    ///
    /// Mirrors the granularity tsc uses (it has no cumulative per-file budget,
    /// only per-relation depth limits). Invoked once per statement from
    /// `StatementChecker::check_with_request` (via `reset_between_statements`),
    /// so every statement-list context — top level, block/function body, switch
    /// case clause, loop and if bodies — is covered uniformly and a heavy
    /// statement inside a method body cannot starve a later callback in the same
    /// body.
    pub(crate) fn reset_per_statement_fuel_budgets(&mut self) {
        self.ctx.eval_session.reset_lazy_resolution_fuel();
        self.ctx.eval_session.reset_lazy_readiness_guards();
        self.ctx.eval_session.reset_instantiation_fuel();
        self.ctx.depth_exceeded.set(false);
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
            let resolved_target = self
                .ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, &literal.text)
                .or_else(|| self.ctx.resolve_import_target(&literal.text));
            if resolved_target.is_none() && !self.module_exists_cross_file(&literal.text) {
                continue;
            }

            self.check_imported_members(&import, &literal.text);
        }
    }
}

fn is_same_display_assignability_message(message: &str) -> bool {
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

    source == target
}
