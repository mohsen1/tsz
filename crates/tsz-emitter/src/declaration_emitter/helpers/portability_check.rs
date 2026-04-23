//! Non-portable type reference checking and diagnostics

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
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
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    /// Emit required imports at the beginning of the .d.ts file.
    ///
    /// This should be called before emitting other declarations.
    pub(crate) fn emit_required_imports(&mut self) {
        if self.import_plan.required.is_empty() {
            debug!("[DEBUG] emit_required_imports: no required imports");
            return;
        }

        let modules = std::mem::take(&mut self.import_plan.required);
        self.emit_import_modules(&modules);
        self.import_plan.required = modules;
    }

    // =========================================================================
    // TS2883: Non-portable inferred type references
    // =========================================================================

    /// Check if an inferred type references symbols from non-portable module paths
    /// (e.g., nested `node_modules` or private package subpaths).
    ///
    /// If non-portable references are found, emits TS2883 diagnostics.
    ///
    /// - `type_id`: the inferred type to check
    /// - `decl_name`: the declaration name (e.g., "x", "default", "special")
    /// - `file`: the source file path for the diagnostic
    /// - `pos`: the position of the declaration name in source
    /// - `length`: the length of the declaration name in source
    pub(crate) fn check_non_portable_type_references(
        &mut self,
        type_id: tsz_solver::types::TypeId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) {
        if self.skip_portability_check {
            return;
        }

        self.emit_non_portable_type_diagnostic(type_id, decl_name, file, pos, length);
    }

    pub(crate) fn emit_non_portable_type_diagnostic(
        &mut self,
        type_id: tsz_solver::types::TypeId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        let Some((from_path, type_name)) = self.find_non_portable_type_reference(type_id) else {
            return false;
        };

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, &from_path, &type_name],
        ));
        true
    }

    /// When the inferred type is `any`/`error` and the initializer is a call
    /// expression,
    /// trace through the callee's declared return type to check for non-portable
    /// type references. This handles cases like:
    ///   `export const special = getSpecial();`
    /// where `getSpecial()` returns `MySpecialType` from a nested `node_modules`.
    pub(crate) fn check_call_expression_return_type_portability(
        &mut self,
        initializer: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) {
        if self.skip_portability_check {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return;
        }

        let Some(call) = self.arena.get_call_expr(init_node) else {
            return;
        };

        // Get the callee's type from the type cache
        let Some(interner) = self.type_interner else {
            return;
        };
        let callee_type_id_opt = self.get_node_type_or_names(&[call.expression]);
        let Some(callee_type_id) = callee_type_id_opt else {
            return;
        };

        // Extract the return type of the callee function
        let return_type_id_opt =
            tsz_solver::type_queries::get_return_type(interner, callee_type_id);
        let Some(return_type_id) = return_type_id_opt else {
            return;
        };

        // Skip if return type is also `any` or `error` — no useful information
        if return_type_id == tsz_solver::types::TypeId::ANY
            || return_type_id == tsz_solver::types::TypeId::ERROR
        {
            // The return type is unresolved. Fall back to checking the callee
            // function's declaration-level return type annotation by tracing
            // through the binder's symbol declarations.
            self.check_callee_declaration_return_type_portability(
                call.expression,
                decl_name,
                file,
                pos,
                length,
            );
            return;
        }

        // Run the full portability check on the return type
        self.emit_non_portable_type_diagnostic(return_type_id, decl_name, file, pos, length);
    }

    /// When the callee's return type can't be resolved through the solver,
    /// trace through the binder's symbol declarations to find the return type
    /// annotation and check the referenced type symbols for portability.
    fn check_callee_declaration_return_type_portability(
        &mut self,
        callee_expr: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) {
        let Some(binder) = self.binder else {
            return;
        };

        // Get the callee's symbol
        let Some(sym_id) = self.value_reference_symbol(callee_expr) else {
            return;
        };

        // Resolve alias (import) to the target symbol
        let resolved = self.resolve_portability_symbol(sym_id, binder);
        let Some(symbol) = binder.symbols.get(resolved) else {
            return;
        };

        // Find the actual function symbol and its source arena.
        //
        // `binder.symbol_arenas` may be empty in the fast-path per-file binder
        // (it is set to `Default::default()` to avoid cloning the full cross-program
        // map into every file binder).  Use `self.global_symbol_arenas` — the full
        // program-level map set on the emitter at emit time — as the reliable source.
        //
        // If `resolved` is still an ALIAS (import didn't resolve to the target), scan
        // `module_exports` to find the same-named symbol from a foreign module.
        let (_, func_symbol, func_arena_arc) = {
            let current_arena_ptr = self.arena as *const _ as usize;

            // Try resolved symbol first via global_symbol_arenas.
            // If the arena IS the current file's arena, the resolved symbol is the
            // local alias itself — fall through to module_exports scan below.
            let use_resolved = self
                .global_symbol_arenas
                .get(&resolved)
                .filter(|arena| std::sync::Arc::as_ptr(arena) as usize != current_arena_ptr);

            if let Some(arena) = use_resolved {
                (resolved, symbol, arena)
            } else {
                // Alias wasn't resolved to its target. Walk module_exports to find the
                // same-named symbol in the specific module this alias imports from.
                // Restricting to `import_module` prevents false positives from
                // same-named exports in unrelated modules.
                let import_module = symbol.import_module.as_deref().unwrap_or("");
                let mut found = None;
                for (path, table) in binder.module_exports.iter() {
                    // Only search in modules whose path ends with the import specifier.
                    // e.g. import_module="foo" matches ".../node_modules/foo/index.d.ts"
                    if !import_module.is_empty() {
                        let node_modules_segment = format!("node_modules/{import_module}");
                        if !path.contains(&node_modules_segment) {
                            continue;
                        }
                    }
                    if let Some(export_sym_id) = table.get(symbol.escaped_name.as_str()) {
                        if export_sym_id == resolved {
                            continue;
                        }
                        if let Some(arena) = self.global_symbol_arenas.get(&export_sym_id) {
                            let ptr = std::sync::Arc::as_ptr(arena) as usize;
                            if ptr != current_arena_ptr {
                                if let Some(sym) = binder.symbols.get(export_sym_id) {
                                    found = Some((export_sym_id, sym, arena));
                                    break;
                                }
                            }
                        }
                    }
                }
                let Some((fid, fsym, farena)) = found else {
                    return;
                };
                (fid, fsym, farena)
            }
        };

        // Find the function declaration and look for a return type annotation
        for &decl_idx in &func_symbol.declarations {
            let source_arena = func_arena_arc.as_ref();

            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };

            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };

            if func.type_annotation.is_none() {
                continue;
            }

            let Some(type_node) = source_arena.get(func.type_annotation) else {
                continue;
            };

            if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                let Some(type_ref) = source_arena.get_type_ref(type_node) else {
                    continue;
                };
                let type_name_idx = type_ref.type_name;
                let type_name_text = source_arena
                    .get(type_name_idx)
                    .and_then(|n| source_arena.get_identifier(n))
                    .map(|id| id.escaped_text.clone());
                let Some(type_name_text) = type_name_text else {
                    continue;
                };

                // Look up the return-type symbol via cross_file_node_symbols (keyed by
                // arena pointer). The `binder.get_node_symbol` path only covers the
                // local file's nodes; cross_file_node_symbols covers all arenas.
                let arena_ptr = std::sync::Arc::as_ptr(func_arena_arc) as usize;
                let type_sym_id = binder
                    .cross_file_node_symbols
                    .get(&arena_ptr)
                    .and_then(|sym_map| sym_map.get(&type_name_idx.0).copied())
                    // Fallback 1: same-file module exports (for types re-exported from
                    // the function's source file).
                    .or_else(|| {
                        let func_file_path =
                            self.arena_to_path.get(&arena_ptr).map(String::as_str)?;
                        let table = binder.module_exports.get(func_file_path)?;
                        let tsym = table.get(type_name_text.as_str())?;
                        let tarena = self.global_symbol_arenas.get(&tsym)?;
                        let current_arena_ptr = self.arena as *const _ as usize;
                        (std::sync::Arc::as_ptr(tarena) as usize != current_arena_ptr)
                            .then_some(tsym)
                    })
                    // Fallback 2: scan for an ALIAS symbol with this name declared in
                    // the function's source file. This covers types that are imported
                    // but not re-exported (e.g., `import { MySpecialType } from "nested"`
                    // inside `foo/index.d.ts`). Only bare-specifier imports are considered
                    // — relative imports cannot be non-portable.
                    .or_else(|| {
                        binder.symbols.iter().find_map(|candidate| {
                            if candidate.escaped_name.as_str() != type_name_text.as_str() {
                                return None;
                            }
                            if !candidate.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
                                return None;
                            }
                            let import_mod = candidate.import_module.as_deref()?;
                            // Only bare specifiers can point to nested node_modules.
                            if import_mod.is_empty()
                                || import_mod.starts_with('.')
                                || import_mod.starts_with('/')
                            {
                                return None;
                            }
                            // Symbol must be declared in the same file as the function.
                            let sym_arena = self.global_symbol_arenas.get(&candidate.id)?;
                            (std::sync::Arc::as_ptr(sym_arena) as usize == arena_ptr)
                                .then_some(candidate.id)
                        })
                    });

                if let Some(type_sym_id) = type_sym_id {
                    // Only check portability for ALIAS symbols (import bindings).
                    let is_alias = binder
                        .symbols
                        .get(type_sym_id)
                        .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::ALIAS));
                    if !is_alias {
                        continue;
                    }

                    let current_file_path = self.current_file_path.as_deref().unwrap_or("");
                    let mut visited_types = rustc_hash::FxHashSet::default();
                    let mut visited_symbols = rustc_hash::FxHashSet::default();
                    let mut visited_declaration_symbols = rustc_hash::FxHashSet::default();
                    let mut visited_nodes = rustc_hash::FxHashSet::default();

                    // Standard portability check first.
                    if let Some((from_path, _type_name)) = self.check_symbol_portability(
                        type_sym_id,
                        binder,
                        current_file_path,
                        &mut visited_types,
                        &mut visited_symbols,
                        &mut visited_declaration_symbols,
                        &mut visited_nodes,
                    ) {
                        use tsz_common::diagnostics::Diagnostic;
                        self.diagnostics.push(Diagnostic::from_code(
                            2883,
                            file,
                            pos,
                            length,
                            &[decl_name, &from_path, &type_name_text],
                        ));
                        return;
                    }

                    // Fallback: the standard check fails when the alias's bare-specifier
                    // import cannot be resolved from the CONSUMER'S perspective (e.g.,
                    // `foo/index.d.ts` imports `MySpecialType from "nested"` but from
                    // `entry.ts`, `"nested"` is not visible at the top-level).
                    // Directly check whether `import_module` resolves to a NESTED
                    // sub-node_modules package relative to the function's source package.
                    if let Some(from_path) =
                        self.check_nested_transitive_import(type_sym_id, binder)
                    {
                        use tsz_common::diagnostics::Diagnostic;
                        self.diagnostics.push(Diagnostic::from_code(
                            2883,
                            file,
                            pos,
                            length,
                            &[decl_name, &from_path, &type_name_text],
                        ));
                        return;
                    }
                }
            }
        }
    }

    pub(crate) fn emit_non_portable_expression_symbol_diagnostic(
        &mut self,
        expr_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_diagnostic(sym_id, decl_name, file, pos, length)
        {
            return true;
        }

        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_initializer_diagnostic(
                sym_id, decl_name, file, pos, length,
            )
        {
            return true;
        }

        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_declaration_diagnostic(
                sym_id, decl_name, file, pos, length,
            )
        {
            return true;
        }

        if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(object) = self.arena.get_literal_expr(expr_node)
        {
            for &member_idx in &object.elements.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = self.arena.get_property_assignment(member_node) else {
                            continue;
                        };
                        if self.emit_non_portable_expression_symbol_diagnostic(
                            prop.initializer,
                            decl_name,
                            file,
                            pos,
                            length,
                        ) {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                        let Some(prop) = self.arena.get_shorthand_property(member_node) else {
                            continue;
                        };
                        if self.emit_non_portable_expression_symbol_diagnostic(
                            prop.name, decl_name, file, pos, length,
                        ) || (prop.object_assignment_initializer.is_some()
                            && self.emit_non_portable_expression_symbol_diagnostic(
                                prop.object_assignment_initializer,
                                decl_name,
                                file,
                                pos,
                                length,
                            ))
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }

        if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(expr_node)
            && self.emit_non_portable_expression_symbol_diagnostic(
                call.expression,
                decl_name,
                file,
                pos,
                length,
            )
        {
            return true;
        }

        if expr_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            && let Some(tagged) = self.arena.get_tagged_template(expr_node)
            && self.emit_non_portable_expression_symbol_diagnostic(
                tagged.tag, decl_name, file, pos, length,
            )
        {
            return true;
        }

        false
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_symbol_initializer_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let source_arena = binder
            .symbol_arenas
            .get(&sym_id)
            .map(|arena| arena.as_ref())
            .unwrap_or(self.arena);

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            if let Some(var_decl) = source_arena.get_variable_declaration(decl_node)
                && var_decl.initializer.is_some()
            {
                if let Some(type_id) = self
                    .get_node_type_or_names(&[var_decl.initializer])
                    .or_else(|| self.get_type_via_symbol(var_decl.initializer))
                    && self.emit_non_portable_type_diagnostic(type_id, decl_name, file, pos, length)
                {
                    return true;
                }
                if self.emit_non_portable_expression_declared_return_diagnostic(
                    var_decl.initializer,
                    decl_name,
                    file,
                    pos,
                    length,
                ) {
                    return true;
                }
                if self.emit_non_portable_expression_symbol_diagnostic(
                    var_decl.initializer,
                    decl_name,
                    file,
                    pos,
                    length,
                ) {
                    return true;
                }
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_expression_declared_return_diagnostic(
        &mut self,
        expr_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        let sym_id = match expr_node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION => self
                .arena
                .get_call_expr(expr_node)
                .and_then(|call| self.value_reference_symbol(call.expression)),
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => self
                .arena
                .get_tagged_template(expr_node)
                .and_then(|tagged| self.value_reference_symbol(tagged.tag)),
            _ => None,
        };

        let Some(sym_id) = sym_id else {
            return false;
        };
        self.emit_non_portable_callable_symbol_declared_return_diagnostic(
            sym_id, decl_name, file, pos, length,
        )
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_callable_symbol_declared_return_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let sym_id = self.resolve_portability_declaration_symbol(sym_id, binder);
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&sym_id) else {
            return false;
        };
        let Some(source_file) = self.arena_source_file(source_arena.as_ref()) else {
            return false;
        };
        if !source_file.is_declaration_file {
            return false;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };

            if let Some(function) = source_arena.get_function(decl_node)
                && function.type_annotation.is_some()
                && {
                    let return_type_node = source_arena
                        .get(function.type_annotation)
                        .and_then(|type_node| source_arena.get_function_type(type_node))
                        .map_or(function.type_annotation, |function_type| {
                            function_type.type_annotation
                        });
                    self.emit_non_portable_type_node_diagnostic_from_arena(
                        source_arena.as_ref(),
                        return_type_node,
                        decl_name,
                        file,
                        pos,
                        length,
                    )
                }
            {
                return true;
            }

            if let Some(signature) = source_arena.get_signature(decl_node) {
                let return_type_node = if decl_node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
                    let Some(type_node) = source_arena.get(signature.type_annotation) else {
                        continue;
                    };
                    source_arena
                        .get_function_type(type_node)
                        .map_or(signature.type_annotation, |function_type| {
                            function_type.type_annotation
                        })
                } else {
                    signature.type_annotation
                };
                if self.emit_non_portable_type_node_diagnostic_from_arena(
                    source_arena.as_ref(),
                    return_type_node,
                    decl_name,
                    file,
                    pos,
                    length,
                ) {
                    return true;
                }
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_symbol_declaration_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let references = self.collect_non_portable_references_in_symbol_declaration(sym_id);
        if references.is_empty() {
            return false;
        }

        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    pub(in crate::declaration_emitter) fn collect_non_portable_references_in_symbol_declaration(
        &self,
        sym_id: SymbolId,
    ) -> Vec<(String, String)> {
        let resolved_sym = if let Some(binder) = self.binder {
            self.resolve_portability_declaration_symbol(sym_id, binder)
        } else {
            sym_id
        };
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        let mut visited_declaration_symbols = rustc_hash::FxHashSet::default();
        let mut visited_nodes = rustc_hash::FxHashSet::default();
        let mut visited_types = rustc_hash::FxHashSet::default();
        self.collect_non_portable_references_in_symbol_declaration_with_state(
            resolved_sym,
            &mut visited_types,
            &mut visited_symbols,
            &mut visited_declaration_symbols,
            &mut visited_nodes,
        )
    }

    pub(in crate::declaration_emitter) fn collect_non_portable_references_in_symbol_declaration_with_state(
        &self,
        sym_id: SymbolId,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> Vec<(String, String)> {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut results = Vec::new();
        self.collect_non_portable_references_in_symbol_declaration_inner(
            sym_id,
            false,
            &mut results,
            &mut seen,
            visited_types,
            visited_symbols,
            visited_declaration_symbols,
            visited_nodes,
        );
        results
    }

    pub(in crate::declaration_emitter) fn resolve_portability_import_alias(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let symbol = binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }

        let module_specifier = symbol.import_module.as_deref()?;
        let export_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        let current_path = self.current_file_path.as_deref()?;

        for module_path in self.matching_module_export_paths(binder, current_path, module_specifier)
        {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            if let Some(resolved) = exports.get(export_name)
                && resolved != sym_id
            {
                return Some(resolved);
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.symbols.iter().find_map(|candidate| {
                if candidate.id == sym_id || candidate.escaped_name != export_name {
                    return None;
                }
                let source_path = self.get_symbol_source_path(candidate.id, binder)?;
                let package_specifier =
                    self.package_specifier_for_node_modules_path(current_path, &source_path)?;
                if package_specifier == module_specifier
                    || package_specifier.starts_with(&format!("{module_specifier}/"))
                {
                    self.package_root_export_reference_path(
                        candidate.id,
                        &candidate.escaped_name,
                        binder,
                        current_path,
                    )
                    .is_some()
                    .then_some(candidate.id)
                } else {
                    None
                }
            });
        }

        None
    }

    pub(in crate::declaration_emitter) fn matching_module_export_paths<'b>(
        &self,
        binder: &'b BinderState,
        current_path: &str,
        module_specifier: &str,
    ) -> Vec<&'b str> {
        let mut matches: Vec<_> = binder
            .module_exports
            .keys()
            .filter_map(|module_path| {
                let matches = if module_specifier.starts_with('.')
                    || module_specifier.starts_with('/')
                {
                    Some(self.strip_ts_extensions(
                        &self.calculate_relative_path(current_path, module_path),
                    ))
                    .as_deref()
                        == Some(module_specifier)
                } else {
                    self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                };
                matches.then_some(module_path.as_str())
            })
            .collect();

        matches.sort_by(|left, right| {
            self.module_export_path_rank(left, module_specifier)
                .cmp(&self.module_export_path_rank(right, module_specifier))
                .then_with(|| left.cmp(right))
        });
        matches
    }

    pub(in crate::declaration_emitter) fn node_modules_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return false;
        };

        let pkg_start = nm_idx + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(|component| {
            matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@')))
        }) {
            2
        } else {
            1
        };
        if components.len() < pkg_start + pkg_len {
            return false;
        }

        let package_name = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let subpath_start = pkg_start + pkg_len;
        if subpath_start >= components.len() {
            return false;
        }

        let relative_path = components[subpath_start..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        let Some(runtime_subpath) = self.declaration_runtime_relative_path(&relative_path) else {
            return false;
        };
        let mut runtime_subpath = runtime_subpath.trim_start_matches("./").to_string();
        if runtime_subpath.ends_with("/index.js") {
            runtime_subpath.truncate(runtime_subpath.len() - "/index.js".len());
        } else if runtime_subpath == "index.js" {
            runtime_subpath.clear();
        }
        if module_specifier == package_name {
            if !runtime_subpath.is_empty() {
                return false;
            }

            let package_root = components[..subpath_start].iter().fold(
                std::path::PathBuf::new(),
                |mut path, component| {
                    path.push(component.as_os_str());
                    path
                },
            );
            return self
                .reverse_export_specifier_for_runtime_path(&package_root, "./index.ts")
                .is_some()
                || self
                    .reverse_export_specifier_for_runtime_path(&package_root, "./index.d.ts")
                    .is_some();
        }
        let candidate = if runtime_subpath.is_empty() {
            package_name
        } else {
            format!("{package_name}/{runtime_subpath}")
        };
        module_specifier == candidate
    }

    pub(in crate::declaration_emitter) fn module_export_path_rank(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> (usize, usize) {
        use std::path::{Component, Path};

        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return (0, module_path.len());
        }

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return (usize::MAX, module_path.len());
        };

        let pkg_start = nm_idx + 1;
        let pkg_len = if module_specifier.starts_with('@') {
            2
        } else {
            1
        };
        let depth_after_package = components.len().saturating_sub(pkg_start + pkg_len);
        (depth_after_package, module_path.len())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::declaration_emitter) fn collect_non_portable_references_in_symbol_declaration_inner(
        &self,
        sym_id: SymbolId,
        skip_self_portability: bool,
        results: &mut Vec<(String, String)>,
        seen: &mut rustc_hash::FxHashSet<(String, String)>,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let resolved_sym = self.resolve_portability_declaration_symbol(sym_id, binder);
        if !visited_declaration_symbols.insert(resolved_sym) {
            return;
        }
        let Some(symbol) = binder.symbols.get(resolved_sym) else {
            return;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&resolved_sym) else {
            return;
        };
        let Some(source_path) = self.get_symbol_source_path(resolved_sym, binder) else {
            return;
        };

        if !skip_self_portability
            && let Some(current_file_path) = self.current_file_path.as_deref()
            && let Some(result) = self.check_symbol_portability(
                resolved_sym,
                binder,
                current_file_path,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            )
            && seen.insert(result.clone())
        {
            results.push(result);
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };

            if let Some(alias) = source_arena.get_type_alias(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    alias.type_node,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(function) = source_arena.get_function(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    function.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
                for &param_idx in &function.parameters.nodes {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        param_idx,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            }

            if let Some(interface) = source_arena.get_interface(decl_node) {
                if let Some(heritage) = &interface.heritage_clauses {
                    for &clause_idx in &heritage.nodes {
                        self.collect_non_portable_references_in_type_node(
                            source_arena.as_ref(),
                            clause_idx,
                            &source_path,
                            results,
                            seen,
                            visited_types,
                            visited_symbols,
                            visited_declaration_symbols,
                            visited_nodes,
                        );
                    }
                }
                for &member_idx in &interface.members.nodes {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        member_idx,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            }

            if let Some(sig) = source_arena.get_signature(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    sig.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(func_type) = source_arena.get_function_type(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    func_type.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(var_decl) = source_arena.get_variable_declaration(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    var_decl.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(prop_decl) = source_arena.get_property_decl(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    prop_decl.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(param) = source_arena.get_parameter(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    param.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::declaration_emitter) fn collect_non_portable_references_in_type_node(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        source_path: &str,
        results: &mut Vec<(String, String)>,
        seen: &mut rustc_hash::FxHashSet<(String, String)>,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) {
        let arena_addr = arena as *const NodeArena as usize;
        if !node_idx.is_some() || !visited_nodes.insert((arena_addr, node_idx.0)) {
            return;
        }
        let Some(node) = arena.get(node_idx) else {
            return;
        };

        if let Some(indexed) = arena.get_indexed_access_type(node) {
            let mut collected_object_refs = false;
            if let Some(sym_id) =
                self.first_bound_symbol_in_type_subtree(arena, indexed.object_type)
            {
                if let Some(binder) = self.binder
                    && let Some(symbol) = binder.symbols.get(sym_id)
                    && let Some(import_module) = symbol.import_module.as_deref()
                    && let Some(current_file_path) = self.current_file_path.as_deref()
                {
                    let mut module_paths =
                        self.matching_module_export_paths(binder, current_file_path, import_module);
                    module_paths.sort_by(|left, right| {
                        self.module_export_path_rank(right, import_module)
                            .cmp(&self.module_export_path_rank(left, import_module))
                            .then_with(|| right.cmp(left))
                    });
                    for module_path in module_paths {
                        let Some(exports) = binder.module_exports.get(module_path) else {
                            continue;
                        };
                        let Some(exported_sym_id) = exports.get(symbol.escaped_name.as_str())
                        else {
                            continue;
                        };
                        if !self.collect_symbol_member_type_references(
                            exported_sym_id,
                            results,
                            seen,
                            visited_types,
                            visited_symbols,
                            visited_declaration_symbols,
                            visited_nodes,
                        ) {
                            self.collect_non_portable_references_in_symbol_declaration_inner(
                                exported_sym_id,
                                true,
                                results,
                                seen,
                                visited_types,
                                visited_symbols,
                                visited_declaration_symbols,
                                visited_nodes,
                            );
                        }
                        collected_object_refs = true;
                    }
                }
                if !collected_object_refs
                    && !self.collect_symbol_member_type_references(
                        sym_id,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    )
                {
                    self.collect_non_portable_references_in_symbol_declaration_inner(
                        sym_id,
                        true,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            } else {
                self.collect_non_portable_references_in_type_node(
                    arena,
                    indexed.object_type,
                    source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }
            self.collect_non_portable_references_in_type_node(
                arena,
                indexed.index_type,
                source_path,
                results,
                seen,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            );
            return;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(printed_type_text) = self.emit_type_node_text(node_idx)
            && printed_type_text.starts_with("import(\"")
            && let Some(sym_id) = self.find_symbol_for_import_type_text(&printed_type_text)
        {
            if let Some(binder) = self.binder
                && let Some(current_file_path) = self.current_file_path.as_deref()
                && let Some(result) = self.check_symbol_portability(
                    sym_id,
                    binder,
                    current_file_path,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                )
                && seen.insert(result.clone())
            {
                results.push(result);
            }

            self.collect_non_portable_references_in_symbol_declaration_inner(
                sym_id,
                false,
                results,
                seen,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            );
        }

        if let Some(result) =
            self.non_portable_namespace_member_reference(arena, node_idx, source_path)
            && seen.insert(result.clone())
        {
            results.push(result);
        }

        if let Some(identifier) = arena.get_identifier(node) {
            let skip_direct_identifier_portability =
                self.is_indexed_access_object_subtree_node(arena, node_idx);
            let sym_id = self
                .find_symbol_in_arena_by_name(arena, &identifier.escaped_text)
                .or_else(|| {
                    self.binder
                        .and_then(|binder| binder.get_node_symbol(node_idx))
                });
            if let Some(sym_id) = sym_id {
                if let Some(binder) = self.binder
                    && let Some(current_file_path) = self.current_file_path.as_deref()
                    && !skip_direct_identifier_portability
                {
                    let result = self.check_symbol_portability(
                        sym_id,
                        binder,
                        current_file_path,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                    if let Some(result) = result
                        && seen.insert(result.clone())
                    {
                        results.push(result);
                    }
                }

                self.collect_non_portable_references_in_symbol_declaration_inner(
                    sym_id,
                    skip_direct_identifier_portability,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }
        }

        for child_idx in arena.get_children(node_idx) {
            self.collect_non_portable_references_in_type_node(
                arena,
                child_idx,
                source_path,
                results,
                seen,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            );
        }
    }

    pub(in crate::declaration_emitter) fn first_bound_symbol_in_type_subtree(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Option<SymbolId> {
        if !node_idx.is_some() {
            return None;
        }

        if let Some(binder) = self.binder
            && let Some(sym_id) = binder.get_node_symbol(node_idx)
        {
            return Some(sym_id);
        }

        let node = arena.get(node_idx)?;
        if let Some(identifier) = arena.get_identifier(node) {
            return self.find_symbol_in_arena_by_name(arena, &identifier.escaped_text);
        }

        for child_idx in arena.get_children(node_idx) {
            if let Some(sym_id) = self.first_bound_symbol_in_type_subtree(arena, child_idx) {
                return Some(sym_id);
            }
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::declaration_emitter) fn collect_symbol_member_type_references(
        &self,
        sym_id: SymbolId,
        results: &mut Vec<(String, String)>,
        seen: &mut rustc_hash::FxHashSet<(String, String)>,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let resolved_sym = self.resolve_portability_declaration_symbol(sym_id, binder);
        let Some(symbol) = binder.symbols.get(resolved_sym) else {
            return false;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&resolved_sym) else {
            return false;
        };
        let Some(source_path) = self.get_symbol_source_path(resolved_sym, binder) else {
            return false;
        };

        let count_before = results.len();
        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = source_arena.get_interface(decl_node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = source_arena.get(member_idx) else {
                    continue;
                };
                if let Some(signature) = source_arena.get_signature(member_node) {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        signature.type_annotation,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                } else if let Some(prop_decl) = source_arena.get_property_decl(member_node) {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        prop_decl.type_annotation,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            }
        }

        results.len() > count_before
    }

    pub(in crate::declaration_emitter) fn collect_indexed_access_object_type_names(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        names: &mut rustc_hash::FxHashSet<String>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) {
        let arena_addr = arena as *const NodeArena as usize;
        if !node_idx.is_some() || !visited_nodes.insert((arena_addr, node_idx.0)) {
            return;
        }

        let Some(node) = arena.get(node_idx) else {
            return;
        };
        if let Some(indexed) = arena.get_indexed_access_type(node) {
            if let Some(sym_id) =
                self.first_bound_symbol_in_type_subtree(arena, indexed.object_type)
                && let Some(binder) = self.binder
                && let Some(symbol) = binder.symbols.get(sym_id)
            {
                names.insert(symbol.escaped_name.clone());
            } else if let Some(name) =
                Self::rightmost_name_text_in_arena(arena, indexed.object_type)
            {
                names.insert(name);
            }
        }

        for child_idx in arena.get_children(node_idx) {
            self.collect_indexed_access_object_type_names(arena, child_idx, names, visited_nodes);
        }
    }

    pub(in crate::declaration_emitter) fn is_indexed_access_object_subtree_node(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        let mut current_idx = node_idx;
        while let Some(ext) = arena.get_extended(current_idx) {
            let parent_idx = ext.parent;
            let Some(parent) = arena.get(parent_idx) else {
                break;
            };
            if let Some(indexed) = arena.get_indexed_access_type(parent)
                && indexed.object_type == current_idx
            {
                return true;
            }
            current_idx = parent_idx;
        }
        false
    }

    pub(crate) fn emit_non_portable_initializer_declaration_diagnostics(
        &mut self,
        expr_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(root_expr) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let mut current = root_expr;
        loop {
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                let Some(call) = self.arena.get_call_expr(node) else {
                    return false;
                };
                current = call.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                let Some(tagged) = self.arena.get_tagged_template(node) else {
                    return false;
                };
                current = tagged.tag;
                continue;
            }
            break;
        }

        let Some(sym_id) = self.value_reference_symbol(current) else {
            return false;
        };
        if self.emit_non_portable_callable_symbol_declared_return_diagnostic(
            sym_id, decl_name, file, pos, length,
        ) {
            return true;
        }
        self.emit_non_portable_symbol_declaration_diagnostic(sym_id, decl_name, file, pos, length)
    }

    pub(crate) fn emit_non_portable_import_type_text_diagnostics(
        &mut self,
        printed_type_text: &str,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(sym_id) = self.find_symbol_for_import_type_text(printed_type_text) else {
            if let Some((from_path, type_name)) =
                self.private_import_type_package_root_reference(printed_type_text)
            {
                self.emit_non_portable_named_reference_diagnostic(
                    decl_name, file, pos, length, &from_path, &type_name,
                );
                return true;
            }
            return false;
        };
        let mut references = self.collect_non_portable_references_in_symbol_declaration(sym_id);
        if references.is_empty()
            && let Some(binder) = self.binder
            && let Some(symbol) = binder.symbols.get(sym_id)
            && let Some(source_path) = self.get_symbol_source_path(sym_id, binder)
            && source_path.contains("node_modules")
            && self
                .package_root_export_reference_path(
                    sym_id,
                    symbol.escaped_name.as_str(),
                    binder,
                    file,
                )
                .is_none()
            // Skip if the source file is a direct-dependency entry point
            // (accessible via bare specifier). package_root_export_reference_path
            // filters same-file entries, so we check explicitly here.
            && self
                .package_specifier_for_node_modules_path(file, &source_path)
                .is_none_or(|spec| spec.contains('/'))
        {
            references.push((
                self.strip_ts_extensions(&self.calculate_relative_path(file, &source_path)),
                symbol.escaped_name.clone(),
            ));
        }
        if self.import_type_uses_private_package_subpath(printed_type_text)
            && let Some(parsed_reference) = self.parse_import_type_text(printed_type_text)
            && !references.contains(&parsed_reference)
        {
            references.insert(0, parsed_reference);
        }
        if let Some(root_reference) =
            self.private_import_type_package_root_reference(printed_type_text)
            && !references.contains(&root_reference)
        {
            references.push(root_reference);
        }
        if references.is_empty() {
            return false;
        }
        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    pub(in crate::declaration_emitter) fn emit_non_serializable_property_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        let Some(property_name) =
            self.find_non_serializable_property_name_in_printed_type(printed_type_text)
        else {
            return false;
        };

        self.diagnostics.push(Diagnostic::from_code(
            4118,
            file,
            pos,
            length,
            &[&property_name],
        ));
        true
    }

    pub(crate) fn emit_non_serializable_import_type_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        if self
            .find_unexported_import_type_reference_in_printed_type(printed_type_text)
            .is_none()
        {
            return false;
        }

        self.diagnostics
            .push(Diagnostic::from_code(7056, file, pos, length, &[]));
        true
    }

    pub(in crate::declaration_emitter) fn emit_truncation_diagnostic_if_needed(
        &mut self,
        expr_idx: NodeIndex,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        // Skip truncation check for property access expressions (e.g., Foo.m1).
        // These are not truncation candidates - their types are typically short
        // function type references like () => void, not complex literal types.
        if let Some(node) = self.arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            return false;
        }

        const NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH: usize = 1_000_000;

        if let Some(estimated_length) = self.estimated_truncation_candidate_length(expr_idx)
            && estimated_length > NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH
        {
            self.diagnostics
                .push(tsz_common::diagnostics::Diagnostic::from_code(
                    7056,
                    file,
                    pos,
                    length,
                    &[],
                ));
            return true;
        }

        let Some(type_text) = self.truncation_candidate_type_text(expr_idx) else {
            return false;
        };

        if type_text.chars().count() <= NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH {
            return false;
        }

        self.diagnostics
            .push(tsz_common::diagnostics::Diagnostic::from_code(
                7056,
                file,
                pos,
                length,
                &[],
            ));
        true
    }

    pub(crate) fn emit_serialized_type_text_truncation_diagnostic_if_needed(
        &mut self,
        type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        const NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH: usize = 1_000_000;

        if type_text.chars().count() <= NO_TRUNCATION_MAXIMUM_TRUNCATION_LENGTH {
            return false;
        }

        self.diagnostics
            .push(tsz_common::diagnostics::Diagnostic::from_code(
                7056,
                file,
                pos,
                length,
                &[],
            ));
        true
    }

    pub(crate) fn emit_non_serializable_local_alias_diagnostic(
        &mut self,
        printed_type_text: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        // When isolated declarations is enabled, the checker will emit more
        // specific errors (TS9010, TS9038, etc.). Skip TS7056 to avoid masking them.
        if self.isolated_declarations {
            return false;
        }

        if !self.printed_type_uses_non_emittable_local_alias_root(printed_type_text) {
            return false;
        }

        self.diagnostics
            .push(Diagnostic::from_code(7056, file, pos, length, &[]));
        true
    }
}
